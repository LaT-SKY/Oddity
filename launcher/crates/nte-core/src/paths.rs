use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

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
