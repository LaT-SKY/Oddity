//! 文件系统空间查询.

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::{Error, Result};

/// 返回路径所在文件系统的可用字节数.
///
/// 目标目录尚未创建时继续查找最近的已存在祖先, 使首次安装前的预算
/// 反映最终位置.
pub fn available_bytes(path: &Path) -> Result<u64> {
    let mut current = path;
    loop {
        let target = if current.as_os_str().is_empty() {
            Path::new(".")
        } else {
            current
        };
        let target = CString::new(target.as_os_str().as_bytes()).map_err(|_| {
            Error::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "路径包含空字节",
            ))
        })?;
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::statvfs(target.as_ptr(), stat.as_mut_ptr()) } == 0 {
            let stat = unsafe { stat.assume_init() };
            return Ok(stat.f_bavail.saturating_mul(stat.f_frsize));
        }
        let error = io::Error::last_os_error();
        let missing = matches!(
            error.raw_os_error(),
            Some(libc::ENOENT) | Some(libc::ENOTDIR)
        );
        if !missing || current.as_os_str().is_empty() {
            return Err(error.into());
        }
        current = current.parent().unwrap_or(Path::new(""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::ffi::OsStr;

    #[test]
    fn missing_directories_use_the_nearest_existing_ancestor() {
        let dir = TempDir::new("disk-space");
        let nested = dir.path().join("missing/child");
        assert_eq!(
            available_bytes(&nested).unwrap(),
            available_bytes(dir.path()).unwrap()
        );
    }

    #[test]
    fn empty_paths_fall_back_to_the_current_directory() {
        assert_eq!(
            available_bytes(Path::new("")).unwrap(),
            available_bytes(Path::new(".")).unwrap()
        );
    }

    #[test]
    fn rejects_paths_with_nul_bytes() {
        assert!(available_bytes(Path::new(OsStr::from_bytes(b"bad\0path"))).is_err());
    }
}
