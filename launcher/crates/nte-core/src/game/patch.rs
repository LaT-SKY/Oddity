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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{write_script, EnvGuard, TempDir};

    #[test]
    fn hpatchz_path_follows_the_environment() {
        // EnvGuard 结束时会把这两个变量恢复成测试前的样子
        let mut env = EnvGuard::new();
        env.remove("NTE_HPATCHZ");
        env.remove("HPATCHZ");
        assert_eq!(hpatchz_path(), PathBuf::from("hpatchz"));

        env.set("HPATCHZ", "/opt/hpatchz");
        assert_eq!(hpatchz_path(), PathBuf::from("/opt/hpatchz"));

        // NTE_HPATCHZ 优先于 HPATCHZ
        env.set("NTE_HPATCHZ", "/custom/hpatchz");
        assert_eq!(hpatchz_path(), PathBuf::from("/custom/hpatchz"));
    }

    #[test]
    fn requires_the_input_files() {
        let dir = TempDir::new("patch-missing");
        let error = apply_hdiff(
            &dir.join("old.bin"),
            &dir.join("p.bin"),
            &dir.join("new.bin"),
        )
        .expect_err("no old file");
        assert!(error.to_string().contains("old file not found"), "{error}");

        dir.write("old.bin", b"old");
        let error = apply_hdiff(
            &dir.join("old.bin"),
            &dir.join("p.bin"),
            &dir.join("new.bin"),
        )
        .expect_err("no patch file");
        assert!(
            error.to_string().contains("patch file not found"),
            "{error}"
        );
    }

    #[test]
    fn reports_a_missing_tool() {
        let mut env = EnvGuard::new();
        let dir = TempDir::new("patch-tool");
        dir.write("old.bin", b"old");
        dir.write("p.bin", b"patch");
        env.set("NTE_HPATCHZ", dir.join("missing-hpatchz"));
        let error = apply_hdiff(
            &dir.join("old.bin"),
            &dir.join("p.bin"),
            &dir.join("new.bin"),
        )
        .expect_err("no tool");
        assert!(
            error.to_string().contains("hpatchz not available"),
            "{error}"
        );
    }

    #[test]
    fn runs_the_tool_and_replaces_a_stale_output() {
        let mut env = EnvGuard::new();
        let dir = TempDir::new("patch-run");
        dir.write("old.bin", b"old");
        dir.write("p.bin", b"patched");
        dir.write("new.bin", b"stale");
        let script = write_script(
            dir.path(),
            "hpatchz-ok",
            "#!/bin/sh\n[ \"$1\" = \"-f\" ] || exit 9\ncp \"$3\" \"$4\"\n",
        );
        env.set("NTE_HPATCHZ", &script);
        apply_hdiff(
            &dir.join("old.bin"),
            &dir.join("p.bin"),
            &dir.join("new.bin"),
        )
        .expect("apply hdiff");
        assert_eq!(dir.read("new.bin"), b"patched");
    }

    #[test]
    fn reports_tool_failures_and_missing_output() {
        let mut env = EnvGuard::new();
        let dir = TempDir::new("patch-fail");
        dir.write("old.bin", b"old");
        dir.write("p.bin", b"patched");

        let failing = write_script(
            dir.path(),
            "hpatchz-fail",
            "#!/bin/sh\necho boom >&2\nexit 3\n",
        );
        env.set("NTE_HPATCHZ", &failing);
        let error = apply_hdiff(
            &dir.join("old.bin"),
            &dir.join("p.bin"),
            &dir.join("new.bin"),
        )
        .expect_err("tool failure");
        assert!(error.to_string().contains("hpatchz failed"), "{error}");
        assert!(error.to_string().contains("boom"), "{error}");

        let quiet = write_script(dir.path(), "hpatchz-quiet", "#!/bin/sh\nexit 0\n");
        env.set("NTE_HPATCHZ", &quiet);
        let error = apply_hdiff(
            &dir.join("old.bin"),
            &dir.join("p.bin"),
            &dir.join("new.out"),
        )
        .expect_err("no output");
        assert!(error.to_string().contains("produced no output"), "{error}");
    }
}
