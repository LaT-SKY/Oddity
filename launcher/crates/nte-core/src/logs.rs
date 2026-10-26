//! 导出 Oddity 日志, 保留会话目录结构.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

/// 只收集普通日志文件, 不跟随链接或导出会话控制文件.
fn collect_logs(root: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root.join(relative)).context("读取日志目录失败")? {
        let entry = entry.context("读取日志条目失败")?;
        let kind = entry.file_type().context("读取日志文件类型失败")?;
        let path = relative.join(entry.file_name());
        if kind.is_dir() {
            collect_logs(root, &path, files)?;
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "log") {
            files.push(path);
        }
    }
    Ok(())
}

/// 在所选目录创建新的 ZIP, 不覆盖已有文件. 返回实际保存路径.
pub fn export_logs(source: &Path, destination: &Path) -> Result<PathBuf> {
    export(source, destination, None)
}

/// 没有游戏日志时也可导出独立诊断报告.
pub fn export_diagnostics(source: &Path, destination: &Path, report: &str) -> Result<PathBuf> {
    export(source, destination, Some(report))
}

fn export(source: &Path, destination: &Path, report: Option<&str>) -> Result<PathBuf> {
    let mut files = Vec::new();
    match fs::symlink_metadata(source) {
        Ok(metadata) if metadata.is_dir() => collect_logs(source, Path::new(""), &mut files)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("读取日志目录失败"),
        _ => bail!("日志路径不是普通目录"),
    }
    if files.is_empty() && report.is_none() {
        bail!("暂无可导出的日志, 请先运行游戏后重试");
    }
    files.sort();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let mut index = 0u64;
    let (path, output) = loop {
        let path = destination.join(format!(
            "oddity-logs-{stamp}-{}-{index}.zip",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => break (path, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => index += 1,
            Err(error) => return Err(error).context("创建日志压缩包失败"),
        }
    };
    let result = (|| -> Result<()> {
        let mut archive = ZipWriter::new(output);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o600);
        if let Some(report) = report {
            archive.start_file("oddity-diagnostics.txt", options)?;
            archive.write_all(report.as_bytes())?;
        }
        for relative in files {
            let name = relative.to_str().context("日志文件名不是有效的 UTF-8")?;
            let input = File::open(source.join(&relative))
                .with_context(|| format!("打开日志失败: {name}"))?;
            // 日志可能持续增长, 仅复制打开时已有的字节.
            let size = input.metadata()?.len();
            archive.start_file(name, options)?;
            io::copy(&mut input.take(size), &mut archive)
                .with_context(|| format!("复制日志失败: {name}"))?;
        }
        archive.finish()?.sync_all().context("保存日志压缩包失败")?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn exports_nested_logs_without_links_or_control_files() {
        let dir = TempDir::new("export-logs");
        dir.write("logs/nte-app-launch.log", "启动日志\n");
        dir.write("logs/native-session-1/status.log", "game_running\n");
        dir.write("logs/native-session-1/go.signal", "private");
        dir.write("outside/secret.log", "secret");
        symlink(dir.join("outside"), dir.join("logs/link")).unwrap();
        symlink(dir.join("outside/secret.log"), dir.join("logs/link.log")).unwrap();
        let destination = dir.join("导出 目录");
        fs::create_dir(&destination).unwrap();
        let first = export_logs(&dir.join("logs"), &destination).unwrap();
        let second = export_logs(&dir.join("logs"), &destination).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            fs::metadata(&first).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let mut archive = zip::ZipArchive::new(File::open(first).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        let mut contents = String::new();
        archive
            .by_name("nte-app-launch.log")
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents, "启动日志\n");
        assert!(archive.by_name("native-session-1/status.log").is_ok());
    }

    #[test]
    fn missing_and_empty_logs_do_not_create_an_archive() {
        let dir = TempDir::new("empty-logs");
        assert!(export_logs(&dir.join("logs"), dir.path())
            .unwrap_err()
            .to_string()
            .contains("暂无"));
        fs::create_dir(dir.join("logs")).unwrap();
        assert!(export_logs(&dir.join("logs"), dir.path()).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn invalid_destination_reports_failure() {
        let dir = TempDir::new("export-destination");
        dir.write("logs/test.log", "test");
        dir.write("file", "existing");
        assert!(export_logs(&dir.join("logs"), &dir.join("file")).is_err());
        assert_eq!(dir.read_text("file"), "existing");
    }

    #[test]
    fn failed_export_removes_partial_archive() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let dir = TempDir::new("export-cleanup");
        dir.write("logs/a.log", "first log");
        dir.write(
            PathBuf::from("logs").join(OsString::from_vec(b"\xff.log".to_vec())),
            "bad name",
        );
        let destination = dir.join("output");
        fs::create_dir(&destination).unwrap();
        assert!(export_logs(&dir.join("logs"), &destination).is_err());
        assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
    }
}
