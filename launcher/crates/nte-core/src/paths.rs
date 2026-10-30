use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Error, Result};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// 独占创建同目录临时文件, 失败或取消时自动清理。
pub(crate) struct TemporaryFile {
    pub file: fs::File,
    pub path: PathBuf,
}

impl TemporaryFile {
    pub fn new(destination: &Path) -> Result<Self> {
        Self::create(destination, None)
    }

    /// 创建临时文件时立即设置权限, 避免写入期间以默认权限暴露内容.
    pub fn with_mode(destination: &Path, mode: u32) -> Result<Self> {
        Self::create(destination, Some(mode))
    }

    fn create(destination: &Path, mode: Option<u32>) -> Result<Self> {
        for _ in 0..64 {
            let mut name = destination.as_os_str().to_os_string();
            name.push(format!(
                ".oddity-new-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            let path = PathBuf::from(name);
            let mut options = fs::OpenOptions::new();
            options.write(true).read(true).create_new(true);
            if let Some(mode) = mode {
                options.mode(mode);
            }
            match options.open(&path) {
                Ok(file) => return Ok(Self { file, path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(Error::Integrity("无法创建独立的安装临时文件".into()))
    }

    pub fn commit(self, destination: &Path, cancelled: &dyn Fn() -> bool) -> Result<()> {
        self.file.sync_all()?;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        fs::rename(&self.path, destination)?;
        if let Some(parent) = destination.parent() {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) fn relative_path(value: &str) -> Result<PathBuf> {
    let normalized = value.trim_start_matches('/').replace('\\', "/");
    let mut path = PathBuf::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return Err(Error::Parse(format!("无效的清单路径: {value}")));
        }
        path.push(part);
    }
    Ok(path)
}

pub(crate) fn ensure_no_symlink(root: &Path, relative: &Path) -> Result<()> {
    let mut current = root.to_path_buf();
    for part in relative.components() {
        if !matches!(part, std::path::Component::Normal(_)) {
            return Err(Error::Integrity("安装路径超出安装目录".into()));
        }
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::Integrity(format!(
                    "安装路径包含符号链接: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::Io(error)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn private_temporary_file_is_created_with_restricted_mode() {
        let dir = TempDir::new("temporary-file-mode");
        let target = dir.join("snapshot.json");
        let temp = TemporaryFile::with_mode(&target, 0o600).unwrap();
        let mode = fs::metadata(&temp.path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
