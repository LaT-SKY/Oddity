//! 检查并脱敏 Oddity 日志, 保留会话目录结构.

mod redact;

use anyhow::{bail, Context, Result};
use encoding_rs_io::DecodeReaderBytesBuilder;
pub use redact::Redactions;
use redact::Redactor;
use serde::Serialize;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const MAX_LINE: u64 = 4 * 1024 * 1024;
const MAX_SAMPLES: usize = 12;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPreview {
    pub file_count: usize,
    pub redactions: Redactions,
    pub samples: Vec<LogSample>,
    pub normalized_files: usize,
}

#[derive(Clone, Serialize)]
pub struct LogSample {
    pub file: String,
    pub line: usize,
    pub text: String,
}

/// 匿名临时文件只保存脱敏后的 ZIP, 关闭即释放; 保存时不再读取源日志.
pub struct PreparedLogs {
    archive: File,
    pub preview: LogPreview,
}

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

impl PreparedLogs {
    pub fn prepare(source: &Path, report: Option<&str>) -> Result<Self> {
        Self::with_home(source, report, std::env::var("HOME").ok().as_deref())
    }

    fn with_home(source: &Path, report: Option<&str>, home: Option<&str>) -> Result<Self> {
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
        let mut redactor = Redactor::new(home);
        let mut preview = LogPreview::default();
        let mut archive = ZipWriter::new(tempfile::tempfile().context("创建脱敏日志快照失败")?);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o600);
        let mut names = HashSet::new();
        if let Some(report) = report {
            archive.start_file("oddity-diagnostics.txt", options)?;
            write_redacted(
                report.as_bytes(),
                "oddity-diagnostics.txt",
                &mut archive,
                &mut redactor,
                &mut preview,
            )?;
            names.insert("oddity-diagnostics.txt".to_string());
        }
        for relative in files {
            let name = redactor.text(relative.to_str().context("日志文件名不是有效的 UTF-8")?);
            if !names.insert(name.clone()) {
                bail!("脱敏后的日志文件名重复, 无法生成压缩包");
            }
            let input = File::open(source.join(&relative)).context("打开待检查日志失败")?;
            // 日志可能持续增长, 仅检查打开时已有的字节.
            let size = input.metadata()?.len();
            archive.start_file(&name, options)?;
            write_redacted(
                input.take(size),
                &name,
                &mut archive,
                &mut redactor,
                &mut preview,
            )
            .with_context(|| format!("检查日志失败: {name}"))?;
        }
        preview.redactions = redactor.counts;
        let mut archive = archive.finish().context("生成脱敏日志快照失败")?;
        archive.rewind()?;
        Ok(Self { archive, preview })
    }

    /// 创建新文件并保存已检查的快照. 失败时保留快照以便重试.
    pub fn save(&mut self, destination: &Path) -> Result<PathBuf> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let mut index = 0u64;
        let (path, mut output) = loop {
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
            self.archive.rewind()?;
            io::copy(&mut self.archive, &mut output).context("保存脱敏日志失败")?;
            output.sync_all().context("保存日志压缩包失败")?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&path);
            return Err(error);
        }
        Ok(path)
    }
}

fn write_redacted(
    input: impl Read,
    name: &str,
    output: &mut impl Write,
    redactor: &mut Redactor,
    preview: &mut LogPreview,
) -> Result<()> {
    let mut reader = BufReader::new(DecodeReaderBytesBuilder::new().strip_bom(true).build(input));
    let mut bytes = Vec::new();
    let mut line = 0;
    let mut normalized = false;
    let mut pending_secret = false;
    loop {
        bytes.clear();
        let count = reader
            .by_ref()
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut bytes)
            .context("读取待检查日志失败")?;
        if count == 0 {
            break;
        }
        if count as u64 > MAX_LINE {
            bail!("日志单行超过 4 MiB, 无法完成隐私检查");
        }
        if bytes.contains(&0) {
            bail!("日志包含无法识别的二进制内容, 请转换为 UTF-8 后重试");
        }
        line += 1;
        let text = String::from_utf8_lossy(&bytes);
        normalized |= text.contains('\u{fffd}');
        let before = redactor.counts.total();
        let redacted = if pending_secret && !text.trim().is_empty() {
            pending_secret = false;
            redactor.counts.credentials += 1;
            if text.ends_with('\n') {
                "[凭据已移除]\n".into()
            } else {
                "[凭据已移除]".into()
            }
        } else {
            pending_secret |= redact::PENDING_FIELD.is_match(text.trim_end());
            redactor.text(&text)
        };
        output
            .write_all(redacted.as_bytes())
            .context("写入脱敏日志失败")?;
        if redactor.counts.total() > before && preview.samples.len() < MAX_SAMPLES {
            preview.samples.push(LogSample {
                file: name.into(),
                line,
                text: redacted.trim_end().chars().take(360).collect(),
            });
        }
    }
    preview.file_count += 1;
    preview.normalized_files += usize::from(normalized);
    Ok(())
}

/// 所有导出入口都使用相同的隐私检查规则.
pub fn export_logs(source: &Path, destination: &Path) -> Result<PathBuf> {
    PreparedLogs::prepare(source, None)?.save(destination)
}

pub fn export_diagnostics(source: &Path, destination: &Path, report: &str) -> Result<PathBuf> {
    PreparedLogs::prepare(source, Some(report))?.save(destination)
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

    #[test]
    fn redacts_credentials_phones_and_wine_paths_without_hiding_diagnostics() {
        let mut redactor = Redactor::new(Some("/home/alice"));
        let text = concat!(
            "/home/alice/Games Z:\\home\\alice\\Games Z:\\\\home\\\\alice\\\\Games\n",
            "file:///home/alice/Games\n",
            "手机号13800138000 +86 13900139000 113800138000 abc13800138000\n",
            "Authorization: Bearer header-secret\nCookie: sid=cookie-secret; other=secret\n",
            "{\"refreshToken\":\"json-secret\", \"password\":\"space secret\"}\n",
            "url=https://example.test/?token=query-secret&file=keep\n",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.signature\n",
            "/home/alice2/keep /mnt/home/alice/keep /usr/lib/libc.so\n",
            "md5=ae695bb78c939cde41807473c6e92460 error=503 version=0.1.0\n",
        );
        let output = redactor.text(text);
        for secret in [
            "header-secret",
            "cookie-secret",
            "json-secret",
            "space secret",
            "query-secret",
            "signature",
            "13900139000",
        ] {
            assert!(!output.contains(secret), "leaked fixture: {secret}");
        }
        assert!(output.contains("手机号[手机号已移除]"));
        for kept in [
            "113800138000",
            "abc13800138000",
            "/home/alice2/keep",
            "/mnt/home/alice/keep",
            "/usr/lib/libc.so",
            "ae695bb78c939cde41807473c6e92460",
            "error=503",
            "version=0.1.0",
            "&file=keep",
        ] {
            assert!(output.contains(kept), "lost fixture: {kept}");
        }
        assert_eq!(redactor.counts.home, 4);
        assert_eq!(redactor.counts.phone, 2);
        assert_eq!(redactor.counts.credentials, 5);
        assert_eq!(redactor.counts.jwt, 1);
    }

    #[test]
    fn saves_the_reviewed_snapshot_and_keeps_sources_unchanged() {
        let dir = TempDir::new("log-snapshot");
        let original = "/home/alice/Games\nphone=13800138000\ntoken=fixture-secret\n";
        dir.write("logs/test.log", original);
        let mut prepared =
            PreparedLogs::with_home(&dir.join("logs"), None, Some("/home/alice")).unwrap();
        assert_eq!(dir.read_text("logs/test.log"), original);
        assert_eq!(prepared.preview.file_count, 1);
        assert_eq!(prepared.preview.samples.len(), 3);
        let summary = serde_json::to_string(&prepared.preview).unwrap();
        assert!(!summary.contains("fixture-secret"));
        assert!(!summary.contains("13800138000"));
        assert!(!summary.contains("/home/alice"));
        // 修改和新增日志都不能改变已预览的内容.
        dir.write("logs/test.log", "new content token=unreviewed-secret\n");
        dir.write("logs/new.log", "unreviewed file\n");
        let first = prepared.save(dir.path()).unwrap();
        let second = prepared.save(dir.path()).unwrap();
        assert_eq!(fs::read(&first).unwrap(), fs::read(second).unwrap());
        let mut zip = zip::ZipArchive::new(File::open(first).unwrap()).unwrap();
        assert_eq!(zip.len(), 1);
        let mut content = String::new();
        zip.by_name("test.log")
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert!(content.contains("$HOME/Games"));
        assert!(!content.contains("unreviewed"));
        assert!(!content.contains("fixture-secret"));
        assert!(!content.contains("13800138000"));
    }

    #[test]
    fn normalizes_encodings_and_checks_multiline_credentials() {
        let dir = TempDir::new("log-encodings");
        let text = "手机号13800138000\n\"token\":\n  \"multiline-secret\"\n";
        let mut utf16 = vec![0xff, 0xfe];
        utf16.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        dir.write("logs/utf16.log", utf16);
        dir.write("logs/invalid.log", b"bad\xff token=invalid-secret\n");
        let mut prepared = PreparedLogs::with_home(&dir.join("logs"), None, None).unwrap();
        assert_eq!(prepared.preview.normalized_files, 1);
        let archive = prepared.save(dir.path()).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(archive).unwrap()).unwrap();
        for index in 0..zip.len() {
            let mut content = String::new();
            zip.by_index(index)
                .unwrap()
                .read_to_string(&mut content)
                .unwrap();
            assert!(!content.contains("13800138000"));
            assert!(!content.contains("multiline-secret"));
            assert!(!content.contains("invalid-secret"));
        }
    }

    #[test]
    fn diagnostics_and_filenames_are_checked_and_failed_save_can_retry() {
        let dir = TempDir::new("log-diagnostics-redaction");
        dir.write("logs/13800138000.log", "token=log-secret");
        let mut prepared =
            PreparedLogs::with_home(&dir.join("logs"), Some("token=report-secret\n"), None)
                .unwrap();
        assert!(prepared.save(&dir.join("missing")).is_err());
        let output = prepared.save(dir.path()).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(output).unwrap()).unwrap();
        assert!(zip.by_name("[手机号已移除].log").is_ok());
        let mut text = String::new();
        zip.by_name("oddity-diagnostics.txt")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert!(!text.contains("report-secret"));
    }

    #[test]
    fn bounds_preview_and_rejects_uninspectable_logs() {
        let dir = TempDir::new("log-preview-limits");
        dir.write("logs/test.log", "token=fixture-secret\n".repeat(40));
        let prepared = PreparedLogs::with_home(&dir.join("logs"), None, None).unwrap();
        assert_eq!(prepared.preview.redactions.credentials, 40);
        assert_eq!(prepared.preview.samples.len(), MAX_SAMPLES);
        dir.write("logs/test.log", "x".repeat(MAX_LINE as usize + 1));
        assert!(PreparedLogs::with_home(&dir.join("logs"), None, None).is_err());
        dir.write("logs/test.log", b"token=secret\0binary");
        assert!(PreparedLogs::with_home(&dir.join("logs"), None, None).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn removes_escaped_and_unfinished_quoted_credentials() {
        let mut redactor = Redactor::new(None);
        for text in [
            r#"{"token":"partial-secret"#,
            "password='partial-secret\\",
            r#"{\"access_token\":\"escaped-secret\"}"#,
            r#"{"password":"escaped \"quoted\" secret", "error":503}"#,
            "auth_token=auth-secret sessionToken=session-secret user_token=user-secret",
        ] {
            let output = redactor.text(text);
            assert!(!output.contains("secret"), "leaked fixture: {output}");
            assert!(output.contains("[凭据已移除]"));
        }
    }
}
