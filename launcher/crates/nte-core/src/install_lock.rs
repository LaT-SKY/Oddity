//! 安装目录级进程锁, 覆盖检查、下载、替换和账本提交。
use std::fs::{self, File, OpenOptions};
use std::path::Path;

use crate::{Error, Result};

#[derive(Debug)]
pub struct InstallLock(File);

impl InstallLock {
    pub fn acquire(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        // 目录句柄直接加锁, 路径别名指向同一 inode, 也不存在锁文件替换竞态。
        let file = OpenOptions::new().read(true).open(root)?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => {
                Error::Unsupported("该安装目录正在被其他任务使用, 请等待任务结束后重试".into())
            }
            std::fs::TryLockError::Error(error) => Error::Io(error),
        })?;
        Ok(Self(file))
    }
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn aliases_share_a_lock_and_other_installations_are_independent() {
        let dir = TempDir::new("installation-lock");
        let root = dir.join("game");
        let lock = InstallLock::acquire(&root).unwrap();
        std::os::unix::fs::symlink(&root, dir.join("alias")).unwrap();
        assert!(InstallLock::acquire(&dir.join("alias")).is_err());
        assert!(InstallLock::acquire(&root).is_err());
        let _other = InstallLock::acquire(&dir.join("other")).unwrap();
        drop(lock);
        assert!(InstallLock::acquire(&root).is_ok());
    }

    #[test]
    fn another_process_cannot_acquire_the_installation() {
        let _env = crate::testing::EnvGuard::new();
        let dir = TempDir::new("installation-process-lock");
        let _lock = InstallLock::acquire(dir.path()).unwrap();
        let status = std::process::Command::new("flock")
            .args(["--nonblock", "--exclusive"])
            .arg(dir.path())
            .arg("true")
            .status()
            .unwrap();
        assert!(!status.success());
        drop(_lock);
        assert!(std::process::Command::new("flock")
            .args(["--nonblock", "--exclusive"])
            .arg(dir.path())
            .arg("true")
            .status()
            .unwrap()
            .success());
    }
}
