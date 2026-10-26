//! 同一游戏安装的独立 Proton/prefix 环境.
use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{symlink, DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::distribution::Distribution;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtonPin {
    pub path: PathBuf,
    pub identity: String,
}

impl ProtonPin {
    pub fn capture(path: &Path) -> Result<Self> {
        let path = path.canonicalize().context("读取 Proton 目录失败")?;
        let mut digest = Md5::new();
        for name in ["proton", "toolmanifest.vdf", "version"] {
            let file = path.join(name);
            if name == "version" && !file.try_exists()? {
                continue;
            }
            let mut input = File::open(&file)
                .with_context(|| format!("Proton 文件不可用: {}", file.display()))?;
            if !input.metadata()?.is_file() || input.metadata()?.len() > 4 * 1024 * 1024 {
                bail!("Proton 版本文件无效: {}", file.display());
            }
            let mut bytes = Vec::new();
            input.read_to_end(&mut bytes)?;
            digest.update(name.as_bytes());
            digest.update((bytes.len() as u64).to_le_bytes());
            digest.update(bytes);
        }
        Ok(Self {
            path,
            identity: digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        })
    }

    pub fn verify(&self) -> Result<()> {
        let current = Self::capture(&self.path)?;
        if current != *self {
            bail!(
                "固定的 Proton 已变化: {}. 请重新选择版本并创建环境",
                self.path.display()
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEnvironment {
    pub id: String,
    pub name: String,
    pub proton: ProtonPin,
    pub install_root: PathBuf,
    pub distribution: Distribution,
    pub created_at: u64,
}

impl RuntimeEnvironment {
    pub fn matches(&self, install_root: &Path, distribution: Distribution) -> bool {
        distribution == self.distribution
            && install_root
                .canonicalize()
                .is_ok_and(|root| root == self.install_root)
    }

    pub fn directory(&self, data: &Path) -> Result<PathBuf> {
        if self.id.len() != 32
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            bail!("运行环境 ID 无效");
        }
        Ok(data.join("environments").join(&self.id))
    }

    pub fn prefix(&self, data: &Path) -> Result<PathBuf> {
        let directory = self.directory(data)?;
        validate_owned(&directory, &self.id)?;
        let prefix = directory.join("prefix");
        if fs::symlink_metadata(&prefix)?.file_type().is_symlink()
            || !prefix.join("drive_c").is_dir()
        {
            bail!("受管 prefix 无效: {}", prefix.display());
        }
        Ok(prefix)
    }
}

fn validate_owned(directory: &Path, id: &str) -> Result<()> {
    if !fs::symlink_metadata(directory)?.is_dir()
        || fs::read_to_string(directory.join(".oddity-environment"))? != id
        || directory.canonicalize()?
            != directory
                .parent()
                .context("环境路径无效")?
                .canonicalize()?
                .join(directory.file_name().context("环境路径无效")?)
    {
        bail!("运行环境目录身份不匹配, 拒绝操作");
    }
    Ok(())
}

/// 同时覆盖启动和复制, 目录别名共用一个锁.
#[derive(Debug)]
pub struct PrefixLock(File);

impl Drop for PrefixLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

pub fn lock_prefix(prefix: &Path) -> Result<PrefixLock> {
    let file = File::open(prefix).context("打开 Wine prefix 失败")?;
    if !file.metadata()?.is_dir() {
        bail!("Wine prefix 不是目录");
    }
    file.try_lock().context("Wine prefix 正在被其他任务使用")?;
    let lock = PrefixLock(file);
    ensure_idle(prefix)?;
    Ok(lock)
}

fn ensure_idle(prefix: &Path) -> Result<()> {
    let prefix = prefix.canonicalize()?;
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == std::process::id() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.uid() != unsafe { libc::geteuid() } {
            continue;
        }
        let Ok(bytes) = fs::read(entry.path().join("environ")) else {
            continue;
        };
        for variable in bytes.split(|byte| *byte == 0) {
            if let Some(value) = variable.strip_prefix(b"WINEPREFIX=") {
                use std::os::unix::ffi::OsStrExt;
                let path = Path::new(std::ffi::OsStr::from_bytes(value));
                if path.canonicalize().is_ok_and(|path| path == prefix) {
                    bail!("Wine prefix 仍被进程 {pid} 使用, 请先退出该环境的程序");
                }
            }
        }
    }
    Ok(())
}

struct Incomplete(PathBuf);
impl Drop for Incomplete {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn create(
    data: &Path,
    source: Option<(&Path, &Path)>,
    proton: &Path,
    install_root: &Path,
    distribution: Distribution,
    name: &str,
) -> Result<RuntimeEnvironment> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        bail!("环境名称应为 1-64 个字符, 且不能包含控制字符");
    }
    let pin = ProtonPin::capture(proton)?;
    let install_root = install_root.canonicalize()?;
    let root = data.join("environments");
    fs::create_dir_all(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let root = root.canonicalize()?;
    let mut random = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let id: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let final_dir = root.join(&id);
    let staging = root.join(format!(".creating-{id}"));
    fs::DirBuilder::new().mode(0o700).create(&staging)?;
    let cleanup = Incomplete(staging.clone());
    if let Some((source, source_proton)) = source {
        let _prefix_lock = lock_prefix(source)?;
        let source = source.canonicalize()?;
        if !source.join("drive_c").is_dir() {
            bail!("源 Wine prefix 尚未初始化");
        }
        if root.starts_with(&source) {
            bail!("源 prefix 不能包含受管环境目录");
        }
        let context = CopyContext {
            source: &source,
            destination: &final_dir.join("prefix"),
            install: &install_root,
            protons: RefCell::new(vec![ProtonPin::capture(source_proton)?, pin.clone()]),
        };
        let required = context.size(&source)?;
        if available_space(&root)? < required {
            bail!(
                "复制 prefix 的可用空间不足, 至少需要 {} MiB",
                required.div_ceil(1024 * 1024)
            );
        }
        context.copy(&source, &staging.join("prefix"))?;
        for proton in context.protons.borrow().iter() {
            proton.verify()?;
        }
    } else {
        // 首次启动由 Proton 初始化注册表和系统文件.
        fs::create_dir_all(staging.join("prefix/drive_c"))?;
    }
    pin.verify()?;
    fs::write(staging.join(".oddity-environment"), &id)?;
    fs::rename(&staging, &final_dir)?;
    drop(cleanup);
    Ok(RuntimeEnvironment {
        id,
        name: name.into(),
        proton: pin,
        install_root,
        distribution,
        created_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    })
}

pub fn discard(data: &Path, environment: &RuntimeEnvironment) -> Result<()> {
    prepare_discard(data, environment)?.commit()
}

pub struct PendingDiscard {
    directory: PathBuf,
    trash: PathBuf,
    restore: bool,
    _lock: PrefixLock,
}

pub fn prepare_discard(data: &Path, environment: &RuntimeEnvironment) -> Result<PendingDiscard> {
    let directory = environment.directory(data)?;
    validate_owned(&directory, &environment.id)?;
    let prefix = environment.prefix(data)?;
    let lock = lock_prefix(&prefix)?;
    let trash = directory.with_file_name(format!(".deleting-{}", environment.id));
    if trash.try_exists()? {
        bail!("存在待清理的运行环境目录: {}", trash.display());
    }
    fs::rename(&directory, &trash).context("准备删除运行环境失败")?;
    Ok(PendingDiscard {
        directory,
        trash,
        restore: true,
        _lock: lock,
    })
}

impl PendingDiscard {
    pub fn commit(mut self) -> Result<()> {
        self.restore = false;
        // remove_dir_all 不跟随目录树中的符号链接.
        fs::remove_dir_all(&self.trash)
            .with_context(|| format!("环境已移除, 残留文件清理失败: {}", self.trash.display()))
    }
}

impl Drop for PendingDiscard {
    fn drop(&mut self) {
        if self.restore {
            if let Err(error) = fs::rename(&self.trash, &self.directory) {
                eprintln!(
                    "恢复运行环境目录失败: {}: {error}",
                    self.directory.display()
                );
            }
        }
    }
}

/// 配置提交前退出的删除任务仍有登记, 启动时恢复原目录名称.
pub fn recover_pending(data: &Path, environment: &RuntimeEnvironment) -> Result<()> {
    let directory = environment.directory(data)?;
    let trash = directory.with_file_name(format!(".deleting-{}", environment.id));
    if directory.try_exists()? || !trash.try_exists()? {
        return Ok(());
    }
    validate_owned(&trash, &environment.id)?;
    let _lock = lock_prefix(&trash.join("prefix"))?;
    fs::rename(trash, directory).context("恢复未提交的环境删除失败")
}

struct CopyContext<'a> {
    source: &'a Path,
    destination: &'a Path,
    install: &'a Path,
    protons: RefCell<Vec<ProtonPin>>,
}

impl CopyContext<'_> {
    fn size(&self, path: &Path) -> Result<u64> {
        let metadata = fs::symlink_metadata(path)?;
        if path == self.install || metadata.file_type().is_symlink() {
            return Ok(0);
        }
        if metadata.is_file() {
            return Ok(metadata.len());
        }
        if !metadata.is_dir() {
            bail!("prefix 包含特殊文件: {}", path.display());
        }
        let mut total = 0u64;
        for entry in fs::read_dir(path)? {
            total = total
                .checked_add(self.size(&entry?.path())?)
                .context("prefix 体积超出范围")?;
        }
        Ok(total)
    }

    fn copy(&self, source: &Path, destination: &Path) -> Result<()> {
        let metadata = fs::symlink_metadata(source)?;
        if source == self.install {
            symlink(self.install, destination)?;
        } else if metadata.file_type().is_symlink() {
            self.copy_link(source, destination)?;
        } else if metadata.is_dir() {
            fs::DirBuilder::new().mode(0o700).create(destination)?;
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                self.copy(&entry.path(), &destination.join(entry.file_name()))?;
            }
        } else if metadata.is_file() {
            let input = File::open(source)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(destination)?;
            // FICLONE 只共享不可变数据块, 后续写入不会修改源文件.
            if unsafe {
                libc::ioctl(
                    output.as_raw_fd(),
                    0x40049409 as libc::c_ulong,
                    input.as_raw_fd(),
                )
            } != 0
            {
                let mut input = input;
                std::io::copy(&mut input, &mut output)?;
            }
            output.flush()?;
            fs::set_permissions(
                destination,
                fs::Permissions::from_mode(metadata.mode() & 0o777),
            )?;
        } else {
            bail!("prefix 包含特殊文件: {}", source.display());
        }
        Ok(())
    }

    fn copy_link(&self, source: &Path, destination: &Path) -> Result<()> {
        let relative = source.strip_prefix(self.source)?;
        if relative == Path::new("drive_c/Program Files (x86)/Neverness To Everness") {
            // 同一个原始 prefix 可能上次用于另一地区, 副本绑定本轮安装.
            symlink(self.install, destination)?;
            return Ok(());
        }
        let target = fs::read_link(source)?;
        let mut absolute = normalize(&source.parent().context("链接路径无效")?.join(&target));
        if let Ok(resolved) = absolute.canonicalize() {
            absolute = resolved;
        }
        if let Ok(relative) = absolute.strip_prefix(self.source) {
            symlink(self.destination.join(relative), destination)?;
        } else if absolute.starts_with(self.install) || self.proton_link(&absolute)? {
            symlink(&absolute, destination)?;
        } else {
            if relative == Path::new("dosdevices/z:") && absolute == Path::new("/") {
                symlink("/", destination)?;
            } else if relative.starts_with("dosdevices") {
                // 实验环境不继承其他宿主盘符及串口映射.
            } else if relative.starts_with("drive_c/users")
                && fs::metadata(source).is_ok_and(|meta| meta.is_dir())
            {
                // Wine 会把 Documents 等目录映射到宿主, 副本改用独立空目录.
                fs::DirBuilder::new().mode(0o700).create(destination)?;
            } else {
                bail!("prefix 含无法隔离的外部链接: {}", relative.display());
            }
        }
        Ok(())
    }

    fn proton_link(&self, target: &Path) -> Result<bool> {
        if self
            .protons
            .borrow()
            .iter()
            .any(|pin| target.starts_with(&pin.path))
        {
            return Ok(true);
        }
        // 切换过版本的 prefix 可能仍引用旧兼容层, 只接受经过验证的 Proton 资源目录.
        for root in target.ancestors().skip(1) {
            let relative = target.strip_prefix(root)?;
            if !relative.starts_with("files") && !relative.starts_with("dist") {
                continue;
            }
            if root.join("proton").is_file() && root.join("toolmanifest.vdf").is_file() {
                self.protons.borrow_mut().push(ProtonPin::capture(root)?);
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            component => result.push(component.as_os_str()),
        }
    }
    result
}

fn available_space(path: &Path) -> Result<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error()).context("读取可用磁盘空间失败");
    }
    let stat = unsafe { stat.assume_init() };
    Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    struct Fixture(TempDir);
    impl Fixture {
        fn new() -> Self {
            let dir = TempDir::new("runtime-environment");
            dir.write("source/drive_c/users/player/AppData/settings.ini", "stable");
            dir.write("source/user.reg", "registry");
            dir.write("proton-a/proton", "runner a");
            dir.write("proton-a/toolmanifest.vdf", "manifest a");
            dir.write("proton-a/version", "A");
            dir.write("proton-b/proton", "runner b");
            dir.write("proton-b/toolmanifest.vdf", "manifest b");
            dir.write("install/game.pak", "shared game");
            Self(dir)
        }

        fn create(&self) -> Result<RuntimeEnvironment> {
            create(
                &self.0.join("data"),
                Some((&self.0.join("source"), &self.0.join("proton-a"))),
                &self.0.join("proton-b"),
                &self.0.join("install"),
                Distribution::Cn,
                "新版测试",
            )
        }
    }

    #[test]
    fn cloned_files_and_registry_are_independent_and_the_game_is_shared() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        symlink(dir.join("install"), dir.join("source/drive_c/game")).unwrap();
        let profile = fixture.create().unwrap();
        let prefix = profile.prefix(&dir.join("data")).unwrap();
        assert_ne!(
            fs::metadata(prefix.join("user.reg")).unwrap().ino(),
            fs::metadata(dir.join("source/user.reg")).unwrap().ino()
        );
        fs::write(prefix.join("user.reg"), "changed").unwrap();
        fs::write(
            prefix.join("drive_c/users/player/AppData/settings.ini"),
            "experiment",
        )
        .unwrap();
        assert_eq!(dir.read_text("source/user.reg"), "registry");
        assert_eq!(
            dir.read_text("source/drive_c/users/player/AppData/settings.ini"),
            "stable"
        );
        assert_eq!(
            fs::read_link(prefix.join("drive_c/game")).unwrap(),
            dir.join("install")
        );
        assert_eq!(profile.proton.path, dir.join("proton-b"));
        assert!(profile.matches(&dir.join("install"), Distribution::Cn));
        assert!(!profile.matches(&dir.join("install"), Distribution::Global));
        discard(&dir.join("data"), &profile).unwrap();
        assert!(!prefix.exists());
        assert_eq!(dir.read_text("install/game.pak"), "shared game");
        assert_eq!(dir.read_text("source/user.reg"), "registry");
    }

    #[test]
    fn clone_retargets_internal_links_and_separates_host_user_directories() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        fs::create_dir(dir.join("source/dosdevices")).unwrap();
        dir.write("host-documents/private.txt", "host");
        symlink("../drive_c", dir.join("source/dosdevices/c:")).unwrap();
        symlink("/", dir.join("source/dosdevices/z:")).unwrap();
        symlink("/dev/ttyS0", dir.join("source/dosdevices/com1")).unwrap();
        symlink(dir.join("source"), dir.join("source/pfx")).unwrap();
        symlink(
            dir.join("host-documents"),
            dir.join("source/drive_c/users/player/Documents"),
        )
        .unwrap();
        symlink(
            dir.join("proton-a/proton"),
            dir.join("source/drive_c/runner"),
        )
        .unwrap();
        let profile = fixture.create().unwrap();
        let prefix = profile.prefix(&dir.join("data")).unwrap();
        assert_eq!(fs::read_link(prefix.join("pfx")).unwrap(), prefix);
        assert_eq!(
            fs::read_link(prefix.join("dosdevices/c:")).unwrap(),
            prefix.join("drive_c")
        );
        assert_eq!(
            fs::read_link(prefix.join("dosdevices/z:")).unwrap(),
            Path::new("/")
        );
        assert!(!prefix.join("dosdevices/com1").exists());
        let documents = prefix.join("drive_c/users/player/Documents");
        assert!(!fs::symlink_metadata(&documents)
            .unwrap()
            .file_type()
            .is_symlink());
        fs::write(documents.join("new.txt"), "copy").unwrap();
        assert!(!dir.join("host-documents/new.txt").exists());
        assert!(!documents.join("private.txt").exists());
        assert_eq!(
            fs::read_link(prefix.join("drive_c/runner")).unwrap(),
            dir.join("proton-a/proton")
        );
    }

    #[test]
    fn can_clone_a_managed_environment_again() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        let first = fixture.create().unwrap();
        let second = create(
            &dir.join("data"),
            Some((
                &first.prefix(&dir.join("data")).unwrap(),
                &first.proton.path,
            )),
            &dir.join("proton-a"),
            &dir.join("install"),
            Distribution::Cn,
            "第二次实验",
        )
        .unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(
            fs::read_to_string(second.prefix(&dir.join("data")).unwrap().join("user.reg")).unwrap(),
            "registry"
        );
    }

    #[test]
    fn cloning_across_protons_accepts_links_to_an_existing_third_build() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        dir.write("proton-c/proton", "runner c");
        dir.write("proton-c/toolmanifest.vdf", "manifest c");
        let driver = dir.write(
            "proton-a/files/lib/wine/x86_64-windows/wdfldr.sys",
            "driver from A",
        );
        let relative = "drive_c/windows/system32/drivers/wdfldr.sys";
        fs::create_dir_all(dir.join("source").join(relative).parent().unwrap()).unwrap();
        symlink(&driver, dir.join("source").join(relative)).unwrap();
        let profile = create(
            &dir.join("data"),
            Some((&dir.join("source"), &dir.join("proton-b"))),
            &dir.join("proton-c"),
            &dir.join("install"),
            Distribution::Cn,
            "B 环境克隆到 C",
        )
        .unwrap();
        let prefix = profile.prefix(&dir.join("data")).unwrap();
        assert_eq!(fs::read_link(prefix.join(relative)).unwrap(), driver);
        let second = create(
            &dir.join("data"),
            Some((&prefix, &profile.proton.path)),
            &dir.join("proton-b"),
            &dir.join("install"),
            Distribution::Cn,
            "C 再克隆到 B",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(second.prefix(&dir.join("data")).unwrap().join(relative)).unwrap(),
            "driver from A"
        );
        assert_eq!(
            fs::read_link(dir.join("source").join(relative)).unwrap(),
            driver
        );
        assert_eq!(dir.read_text("source/user.reg"), "registry");
    }

    #[test]
    fn unknown_links_abort_copy_and_remove_incomplete_directories() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        symlink("/etc", dir.join("source/drive_c/unsafe")).unwrap();
        let error = fixture.create().unwrap_err();
        assert!(format!("{error:#}").contains("无法隔离的外部链接"));
        assert_eq!(
            fs::read_dir(dir.join("data/environments")).unwrap().count(),
            0
        );
        assert_eq!(dir.read_text("source/user.reg"), "registry");
    }

    #[test]
    fn a_driver_name_does_not_allow_links_outside_a_valid_proton_installation() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        let driver = dir.write(
            "other/files/lib/wine/x86_64-windows/wdfldr.sys",
            "external file",
        );
        dir.write("other/proton", "only one marker");
        let link = dir.join("source/drive_c/windows/system32/drivers/wdfldr.sys");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(driver, link).unwrap();
        assert!(fixture
            .create()
            .unwrap_err()
            .to_string()
            .contains("无法隔离的外部链接"));
        assert_eq!(
            fs::read_dir(dir.join("data/environments")).unwrap().count(),
            0
        );
    }

    #[test]
    fn sdk_link_is_bound_to_the_selected_installation_without_changing_the_source() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        dir.write("global-install/game.pak", "other region");
        fs::create_dir_all(dir.join("source/drive_c/Program Files (x86)")).unwrap();
        let relative = "drive_c/Program Files (x86)/Neverness To Everness";
        symlink(
            dir.join("global-install"),
            dir.join("source").join(relative),
        )
        .unwrap();
        let profile = fixture.create().unwrap();
        let cloned_link = profile.prefix(&dir.join("data")).unwrap().join(relative);
        assert_eq!(fs::read_link(cloned_link).unwrap(), dir.join("install"));
        assert_eq!(
            fs::read_link(dir.join("source").join(relative)).unwrap(),
            dir.join("global-install")
        );
    }

    #[test]
    fn live_prefix_locks_block_clone_and_discard() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        let lock = lock_prefix(&dir.join("source")).unwrap();
        assert!(fixture.create().is_err());
        drop(lock);
        let profile = fixture.create().unwrap();
        let prefix = profile.prefix(&dir.join("data")).unwrap();
        let _lock = lock_prefix(&prefix).unwrap();
        assert!(discard(&dir.join("data"), &profile).is_err());
        assert!(prefix.exists());
    }

    #[test]
    fn pending_delete_rolls_back_until_committed_and_recovers_after_interruption() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        let profile = fixture.create().unwrap();
        let directory = profile.directory(&dir.join("data")).unwrap();
        let pending = prepare_discard(&dir.join("data"), &profile).unwrap();
        assert!(!directory.exists());
        drop(pending);
        assert!(profile.prefix(&dir.join("data")).is_ok());
        let trash = directory.with_file_name(format!(".deleting-{}", profile.id));
        fs::rename(&directory, &trash).unwrap();
        recover_pending(&dir.join("data"), &profile).unwrap();
        assert!(profile.prefix(&dir.join("data")).is_ok());
        assert!(!trash.exists());
        recover_pending(&dir.join("data"), &profile).unwrap();
        prepare_discard(&dir.join("data"), &profile)
            .unwrap()
            .commit()
            .unwrap();
        assert!(!directory.exists());
    }

    #[test]
    fn proton_pins_reject_changes_and_missing_versions() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        let pin = ProtonPin::capture(&dir.join("proton-a")).unwrap();
        pin.verify().unwrap();
        dir.write("proton-a/version", "new version");
        assert!(pin.verify().is_err());
        fs::remove_file(dir.join("proton-a/proton")).unwrap();
        assert!(pin.verify().is_err());
        assert!(ProtonPin::capture(&dir.join("missing")).is_err());
    }

    #[test]
    fn deletion_rejects_traversal_wrong_marker_and_replaced_directory() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        let mut profile = fixture.create().unwrap();
        let original_id = profile.id.clone();
        profile.id = "../../source".into();
        assert!(discard(&dir.join("data"), &profile).is_err());
        profile.id = original_id.clone();
        let directory = profile.directory(&dir.join("data")).unwrap();
        fs::write(directory.join(".oddity-environment"), "other").unwrap();
        assert!(discard(&dir.join("data"), &profile).is_err());
        fs::write(directory.join(".oddity-environment"), original_id).unwrap();
        let saved = directory.with_extension("saved");
        fs::rename(&directory, &saved).unwrap();
        symlink(&saved, &directory).unwrap();
        assert!(discard(&dir.join("data"), &profile).is_err());
        assert!(saved.join("prefix/user.reg").is_file());
        assert!(dir.join("source/user.reg").is_file());
    }

    #[test]
    fn installation_inside_prefix_is_shared_instead_of_copied() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        dir.write("source/drive_c/NTE/game.pak", "large resource");
        let profile = create(
            &dir.join("data"),
            Some((&dir.join("source"), &dir.join("proton-a"))),
            &dir.join("proton-b"),
            &dir.join("source/drive_c/NTE"),
            Distribution::Cn,
            "游戏在前缀中",
        )
        .unwrap();
        assert_eq!(
            fs::read_link(
                profile
                    .prefix(&dir.join("data"))
                    .unwrap()
                    .join("drive_c/NTE")
            )
            .unwrap(),
            dir.join("source/drive_c/NTE")
        );
    }

    #[test]
    fn invalid_name_and_recursive_destination_are_rejected() {
        let fixture = Fixture::new();
        let dir = &fixture.0;
        for name in ["", "\n", &"a".repeat(65)] {
            assert!(create(
                &dir.join("data"),
                Some((&dir.join("source"), &dir.join("proton-a"))),
                &dir.join("proton-b"),
                &dir.join("install"),
                Distribution::Cn,
                name
            )
            .is_err());
        }
        assert!(create(
            &dir.join("source/data"),
            Some((&dir.join("source"), &dir.join("proton-a"))),
            &dir.join("proton-b"),
            &dir.join("install"),
            Distribution::Cn,
            "recursive"
        )
        .is_err());
    }
}
