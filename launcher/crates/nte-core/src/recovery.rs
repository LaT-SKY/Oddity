//! 不依赖图形环境的应急诊断与恢复操作.
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::settings::{Settings, UserPaths};

/// 主程序持有共享锁, 恢复操作持有独占锁. 锁文件永不删除, 避免 inode 替换竞态.
#[derive(Debug)]
pub struct LauncherLock(File);

impl Drop for LauncherLock {
    fn drop(&mut self) {
        // 子进程可能短暂继承描述符, 显式解锁避免延长占用时间.
        let _ = self.0.unlock();
    }
}

fn open_lock(paths: &UserPaths, create: bool) -> Result<File> {
    if create {
        fs::create_dir_all(&paths.config_base).context("创建配置目录失败")?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(create)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(paths.instance_lock())
        .context("打开启动器进程锁失败")?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() } {
        bail!("启动器进程锁的类型或所有者不正确");
    }
    Ok(file)
}

impl LauncherLock {
    pub fn launcher(paths: &UserPaths) -> Result<Self> {
        let file = open_lock(paths, true)?;
        let mut lock = Self(file);
        // 首个进程在初始化图形环境前登记. 后续唤醒进程只共享锁, 不覆盖身份.
        match lock.0.try_lock() {
            Ok(()) => lock.register()?,
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(error) => return Err(error.into()),
        }
        lock.0
            .try_lock_shared()
            .context("应急恢复正在修改配置, 请稍后重新启动")?;
        Ok(lock)
    }

    pub fn recovery(paths: &UserPaths) -> Result<Self> {
        let file = open_lock(paths, true)?;
        file.try_lock()
            .context("Oddity 正在运行或有其他恢复任务, 请先退出后重试")?;
        Ok(Self(file))
    }

    fn register(&mut self) -> Result<()> {
        let identity = Identity {
            pid: std::process::id(),
            start: process_start(std::process::id())?,
            executable: std::env::current_exe()?,
        };
        self.0.seek(SeekFrom::Start(0))?;
        self.0.set_len(0)?;
        serde_json::to_writer(&mut self.0, &identity)?;
        self.0.sync_all()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Identity {
    pid: u32,
    start: u64,
    executable: PathBuf,
}

fn registered_identity(paths: &UserPaths) -> Result<Identity> {
    let mut text = String::new();
    open_lock(paths, false)?
        .take(4096)
        .read_to_string(&mut text)?;
    serde_json::from_str(&text).context("进程身份记录不可用, 请手动退出 Oddity")
}

fn process_start(pid: u32) -> Result<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).context("读取启动器进程身份失败")?;
    stat.rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().nth(19))
        .context("无法解析启动器进程身份")?
        .parse()
        .context("启动器进程启动时间无效")
}

pub fn running(paths: &UserPaths) -> Result<bool> {
    if !paths.instance_lock().try_exists()? {
        return Ok(false);
    }
    let lock = LauncherLock(open_lock(paths, false)?);
    match lock.0.try_lock() {
        Ok(()) => Ok(false),
        Err(std::fs::TryLockError::WouldBlock) => Ok(true),
        Err(error) => Err(error.into()),
    }
}

/// 仅向已登记且身份匹配的启动器发送 SIGTERM, 不按名称匹配或终止进程组.
pub fn stop_launcher(paths: &UserPaths) -> Result<()> {
    if !running(paths)? {
        return Ok(());
    }
    let identity = registered_identity(paths)?;
    if identity.pid <= 1 || identity.pid == std::process::id() {
        bail!("拒绝终止无效的启动器进程");
    }
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, identity.pid, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error()).context("无法锁定启动器进程, 请手动退出");
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
    let expected_name = identity
        .executable
        .file_name()
        .and_then(|name| name.to_str());
    if !matches!(
        expected_name,
        Some("oddity-backend" | ".oddity-backend-wrapped")
    ) || fs::read_link(format!("/proc/{}/exe", identity.pid))? != identity.executable
        || fs::metadata(format!("/proc/{}", identity.pid))?.uid() != unsafe { libc::geteuid() }
    {
        bail!("登记的进程不是当前用户的 Oddity, 拒绝终止");
    }
    if process_start(identity.pid)? != identity.start {
        bail!("启动器进程身份已变化, 请刷新诊断后重试");
    }
    let result = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            fd.as_raw_fd(),
            libc::SIGTERM,
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("终止启动器失败");
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !running(paths)? {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    bail!("启动器尚未退出, 未执行任何配置恢复");
}

fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn settings_document(bytes: &[u8]) -> Result<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    if !value.is_object() {
        bail!("设置必须是 JSON 对象");
    }
    serde_json::from_slice::<Settings>(bytes)?;
    Ok(value)
}

/// 保留安装和账号设置. 损坏配置只有在明确选择后才整体重建, 原始字节先备份.
pub fn reset_interface(paths: &UserPaths, reset_invalid: bool) -> Result<Option<PathBuf>> {
    let _lock = LauncherLock::recovery(paths)?;
    let path = paths.settings();
    let original = match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_file() => Some(fs::read(&path).context("读取设置失败")?),
        Ok(_) => bail!("设置路径不是普通文件, 拒绝恢复"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("检查设置路径失败"),
    };
    let mut value = if let Some(bytes) = &original {
        match settings_document(bytes) {
            Ok(value) => value,
            Err(_) if reset_invalid => serde_json::to_value(Settings::default())?,
            Err(_) => bail!("设置无法解析. 如需重建, 请明确选择重建损坏配置; 原文件将先备份"),
        }
    } else {
        serde_json::to_value(Settings::default())?
    };
    value["heroMedia"] = "photo".into();
    value["reduceMotion"] = true.into();
    value["theme"] = "light".into();
    value["minimizeToTrayOnClose"] = false.into();
    value["showLauncherAfterGameExit"] = true.into();
    fs::create_dir_all(paths.config_dir()).context("创建设置目录失败")?;
    let backup = if let Some(bytes) = original {
        let backup = paths.config_dir().join(format!(
            "settings.backup-{}-{}.json",
            stamp(),
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&backup)
            .context("备份设置失败")?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Some(backup)
    } else {
        None
    };
    let mut tmp = crate::paths::TemporaryFile::new(&path)?;
    tmp.file
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    serde_json::to_writer_pretty(&mut tmp.file, &value)?;
    tmp.commit(&path, &|| false)?;
    Ok(backup)
}

pub fn diagnose(paths: &UserPaths) -> String {
    let mut lines = vec![format!("Oddity {} 应急诊断", env!("CARGO_PKG_VERSION"))];
    if let Ok(release) = fs::read_to_string("/proc/sys/kernel/osrelease") {
        lines.push(format!("Linux: {}", release.trim()));
    }
    if let Ok(release) = fs::read_to_string("/etc/os-release") {
        for line in release
            .lines()
            .filter(|line| line.starts_with("PRETTY_NAME="))
        {
            lines.push(format!(
                "发行版: {}",
                line.trim_start_matches("PRETTY_NAME=").trim_matches('"')
            ));
        }
    }
    if let Ok(version) = fs::read_to_string("/sys/module/nvidia/version") {
        lines.push(format!("NVIDIA: {}", version.trim()));
    }
    for key in [
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
        "GDK_BACKEND",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "NTE_SOFTWARE_RENDER",
        "LIBGL_ALWAYS_SOFTWARE",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
        "WEBKIT_DISABLE_DMABUF_RENDERER",
    ] {
        lines.push(format!(
            "{key}={}",
            std::env::var(key).unwrap_or_else(|_| "未设置".into())
        ));
    }
    lines.push(format!(
        "NVIDIA 内核模块: {}",
        Path::new("/sys/module/nvidia").exists()
    ));
    for (name, path) in [("配置目录", paths.config_dir()), ("日志目录", paths.logs())] {
        lines.push(format!(
            "{name}: {} ({})",
            path.display(),
            if path.is_dir() { "存在" } else { "不存在" }
        ));
    }
    let settings = match fs::read(paths.settings()) {
        Ok(bytes) => match settings_document(&bytes) {
            Ok(_) => "可解析".into(),
            Err(error) => match error.downcast_ref::<serde_json::Error>() {
                Some(error) => format!("无法解析 (行 {}, 列 {})", error.line(), error.column()),
                None => "无法解析 (设置必须是 JSON 对象)".into(),
            },
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "尚未创建".into(),
        Err(error) => format!("读取失败 ({:?})", error.kind()),
    };
    lines.push(format!("设置: {settings}"));
    lines.push(format!(
        "启动器进程锁: {}",
        match running(paths) {
            Ok(true) => "使用中",
            Ok(false) => "空闲",
            Err(_) => "无法检查",
        }
    ));
    if running(paths).unwrap_or(false) {
        if let Ok(identity) = registered_identity(paths) {
            if process_start(identity.pid).is_ok_and(|start| start == identity.start) {
                lines.push(format!("已登记启动器: PID={}", identity.pid));
            }
        }
    }
    for executable in ["oddity", "nte-cli", "umu-run"] {
        let found = std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|p| {
                fs::metadata(p.join(executable))
                    .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            })
        });
        lines.push(format!(
            "{executable}: {}",
            if found {
                "PATH 中可用"
            } else {
                "PATH 中未找到"
            }
        ));
    }
    let report = lines.join("\n") + "\n";
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && home != "/" => report.replace(&home, "$HOME"),
        _ => report,
    }
}

pub fn export_diagnostics(paths: &UserPaths, destination: &Path) -> Result<PathBuf> {
    crate::logs::export_diagnostics(&paths.logs(), destination, &diagnose(paths))
}

pub fn open_launcher_log(paths: &UserPaths) -> Result<File> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(paths.logs())?;
    let path = paths.logs().join(format!(
        "oddity-startup-{}-{}.log",
        stamp(),
        std::process::id()
    ));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .context("创建启动器日志失败")
}

/// 同目录查找可执行文件, 保证恢复窗口操作的是配套版本.
pub fn sibling(name: &str) -> Result<PathBuf> {
    let executable = std::env::current_exe().context("无法确定应急程序路径")?;
    Ok(executable.parent().context("无法确定程序目录")?.join(name))
}

pub fn start_compatible(paths: &UserPaths) -> Result<Child> {
    let _lock = LauncherLock::recovery(paths)?;
    let mut command = Command::new(sibling("oddity")?);
    command
        .arg("--recovery-compatible")
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    // 主程序自行获取共享锁; 如有新恢复任务抢先开始, 主程序会拒绝读取配置.
    drop(_lock);
    command.spawn().context("兼容启动失败")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn paths(dir: &TempDir) -> UserPaths {
        UserPaths {
            config_base: dir.join("config"),
            data_base: dir.join("data"),
        }
    }

    #[test]
    fn recovery_lock_blocks_live_launchers_and_other_processes() {
        let _env = crate::testing::EnvGuard::new();
        let dir = TempDir::new("recovery-lock");
        let paths = paths(&dir);
        let launcher = LauncherLock::launcher(&paths).unwrap();
        let second = LauncherLock::launcher(&paths).unwrap();
        assert!(running(&paths).unwrap());
        assert!(diagnose(&paths).contains(&format!("PID={}", std::process::id())));
        assert!(reset_interface(&paths, false).is_err());
        assert!(!paths.settings().exists());
        assert!(!Command::new("flock")
            .args(["--nonblock", "--exclusive"])
            .arg(paths.instance_lock())
            .arg("true")
            .status()
            .unwrap()
            .success());
        drop((launcher, second));
        let recovery = LauncherLock::recovery(&paths).unwrap();
        assert!(LauncherLock::launcher(&paths).is_err());
        assert!(LauncherLock::recovery(&paths).is_err());
        let inherited = recovery.0.try_clone().unwrap();
        drop(recovery);
        assert!(!running(&paths).unwrap());
        assert!(LauncherLock::recovery(&paths).is_ok());
        drop(inherited);
    }

    #[test]
    fn reset_preserves_non_ui_settings_and_backs_up_exact_bytes() {
        let dir = TempDir::new("recovery-reset");
        let paths = paths(&dir);
        let original = br#"{ "distribution": "global", "installRoot": "/games/cn", "globalInstallRoot": "/games/global", "prefix": "/wine/prefix", "protonPath": "/proton", "heroMedia": "video", "futureOption": 7 }"#;
        fs::create_dir_all(paths.config_dir()).unwrap();
        fs::write(paths.settings(), original).unwrap();
        let backup = reset_interface(&paths, false).unwrap().unwrap();
        assert_eq!(fs::read(&backup).unwrap(), original);
        assert_eq!(
            fs::metadata(backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(paths.settings()).unwrap()).unwrap();
        assert_eq!(value["distribution"], "global");
        assert_eq!(value["prefix"], "/wine/prefix");
        assert_eq!(value["installRoot"], "/games/cn");
        assert_eq!(value["globalInstallRoot"], "/games/global");
        assert_eq!(value["protonPath"], "/proton");
        assert_eq!(value["futureOption"], 7);
        assert_eq!(value["heroMedia"], "photo");
        assert_eq!(value["minimizeToTrayOnClose"], false);
    }

    #[test]
    fn corrupt_settings_require_explicit_rebuild_and_keep_the_original() {
        let dir = TempDir::new("recovery-corrupt");
        let paths = paths(&dir);
        fs::create_dir_all(paths.config_dir()).unwrap();
        for bytes in [b"{broken".as_slice(), b"[]", br#"{"heroMedia":7}"#] {
            fs::write(paths.settings(), bytes).unwrap();
            assert!(diagnose(&paths).contains("无法解析"));
            assert!(reset_interface(&paths, false).is_err());
            assert_eq!(fs::read(paths.settings()).unwrap(), bytes);
            let backup = reset_interface(&paths, true).unwrap().unwrap();
            assert_eq!(fs::read(backup).unwrap(), bytes);
            assert!(
                serde_json::from_slice::<Settings>(&fs::read(paths.settings()).unwrap()).is_ok()
            );
        }
    }

    #[test]
    fn reset_rejects_symlinks_and_does_not_touch_game_data() {
        let dir = TempDir::new("recovery-links");
        let paths = paths(&dir);
        dir.write("game/keep", "game");
        fs::create_dir_all(paths.config_dir()).unwrap();
        std::os::unix::fs::symlink(dir.join("game/keep"), paths.settings()).unwrap();
        assert!(reset_interface(&paths, true).is_err());
        assert_eq!(fs::read_to_string(dir.join("game/keep")).unwrap(), "game");
        fs::remove_file(paths.settings()).unwrap();
        assert!(reset_interface(&paths, false).unwrap().is_none());
        assert_eq!(fs::read_to_string(dir.join("game/keep")).unwrap(), "game");
    }

    #[test]
    fn first_start_can_export_diagnostics_without_game_or_config() {
        let dir = TempDir::new("recovery-export");
        let paths = paths(&dir);
        let output = export_diagnostics(&paths, dir.path()).unwrap();
        assert!(!paths.config_dir().exists());
        let mut archive = zip::ZipArchive::new(File::open(output).unwrap()).unwrap();
        assert_eq!(archive.len(), 1);
        let mut report = String::new();
        archive
            .by_name("oddity-diagnostics.txt")
            .unwrap()
            .read_to_string(&mut report)
            .unwrap();
        assert!(report.contains("尚未创建"));
        assert!(report.contains("空闲"));
    }

    #[test]
    fn registered_process_fixture() {
        let Some(base) = std::env::var_os("ODDITY_RECOVERY_FIXTURE") else {
            return;
        };
        let paths = UserPaths {
            config_base: PathBuf::from(base),
            data_base: PathBuf::new(),
        };
        let _lock = LauncherLock::launcher(&paths).unwrap();
        std::thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn stop_uses_the_registered_identity_and_releases_the_lock() {
        let dir = TempDir::new("recovery-stop");
        let paths = paths(&dir);
        let executable = dir.join("oddity-backend");
        fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        let mut child = Command::new(&executable)
            .args([
                "--exact",
                "recovery::tests::registered_process_fixture",
                "--nocapture",
            ])
            .env("ODDITY_RECOVERY_FIXTURE", &paths.config_base)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !fs::read(paths.instance_lock())
            .is_ok_and(|v| serde_json::from_slice::<Identity>(&v).is_ok())
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        let result = (|| -> Result<()> {
            assert!(running(&paths)?);
            let mut identity: Identity = serde_json::from_slice(&fs::read(paths.instance_lock())?)?;
            identity.start += 1;
            fs::write(paths.instance_lock(), serde_json::to_vec(&identity)?)?;
            assert!(stop_launcher(&paths).is_err());
            assert!(child.try_wait()?.is_none());
            identity.start -= 1;
            fs::write(paths.instance_lock(), serde_json::to_vec(&identity)?)?;
            stop_launcher(&paths)?;
            assert!(!running(&paths)?);
            Ok(())
        })();
        let _ = child.kill();
        let _ = child.wait();
        result.unwrap();
    }
}
