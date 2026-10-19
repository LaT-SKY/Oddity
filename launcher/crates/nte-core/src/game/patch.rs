use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Error, Result};

/// Path to the HDiffPatch CLI. The launcher does not bundle it yet; package
/// builders can point NTE_HPATCHZ at a Linux hpatchz binary.
pub fn hpatchz_path() -> PathBuf {
    std::env::var_os("NTE_HPATCHZ")
        .or_else(|| std::env::var_os("HPATCHZ"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("hpatchz"))
}

/// Apply a HDIFF13 patch: hpatchz -f old patch new.
///
/// The caller is responsible for verifying the output MD5/size before
/// replacing the live file.
pub fn apply_hdiff(old: &Path, patch: &Path, out: &Path) -> Result<()> {
    if !old.is_file() {
        return Err(Error::Parse(format!(
            "old file not found: {}",
            old.display()
        )));
    }
    if !patch.is_file() {
        return Err(Error::Parse(format!(
            "patch file not found: {}",
            patch.display()
        )));
    }
    if out.exists() {
        std::fs::remove_file(out)?;
    }
    let bin = hpatchz_path();
    let output = Command::new(&bin)
        .arg("-f")
        .arg(old)
        .arg(patch)
        .arg(out)
        .output()
        .map_err(|e| {
            Error::Unsupported(format!(
                "hpatchz not available ({}): {e}; install hpatchz or set NTE_HPATCHZ",
                bin.display()
            ))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(Error::Parse(format!(
            "hpatchz failed for {}: {}{}",
            patch.display(),
            stdout.trim(),
            stderr.trim()
        )));
    }
    if !out.is_file() {
        return Err(Error::Parse(format!(
            "hpatchz produced no output for {}",
            patch.display()
        )));
    }
    Ok(())
}
