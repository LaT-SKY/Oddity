use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use md5::{Digest, Md5};

use crate::manifest::{Manifest, ManifestFile};
use crate::{Error, Result, USER_AGENT};

#[derive(Debug, Default)]
pub struct CheckReport {
    pub checked: usize,
    pub ok: usize,
    pub missing: Vec<ManifestFile>,
    pub changed: Vec<ManifestFile>,
}

impl CheckReport {
    pub fn total_missing_bytes(&self) -> u64 {
        self.missing
            .iter()
            .chain(self.changed.iter())
            .map(|f| f.size)
            .sum()
    }
}

pub fn md5_bytes(bytes: &[u8]) -> String {
    let mut h = Md5::new();
    h.update(bytes);
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub fn md5_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut h = Md5::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

pub fn check(root: &Path, manifest: &Manifest, quick: bool) -> Result<CheckReport> {
    let mut report = CheckReport::default();
    for f in &manifest.files {
        report.checked += 1;
        let rel = f
            .path
            .trim_start_matches('/')
            .replace('/', std::path::MAIN_SEPARATOR_STR);
        let local = root.join(rel);
        let meta = match fs::metadata(&local) {
            Ok(m) => m,
            Err(_) => {
                report.missing.push(f.clone());
                continue;
            }
        };
        if meta.len() != f.size {
            report.changed.push(f.clone());
            continue;
        }
        if quick || f.size == 0 {
            report.ok += 1;
            continue;
        }
        let sum = md5_file(&local)?;
        if !sum.eq_ignore_ascii_case(&f.checksum) {
            report.changed.push(f.clone());
        } else {
            report.ok += 1;
        }
    }
    Ok(report)
}

fn download(url: &str) -> Result<Vec<u8>> {
    let mut resp = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .call()
        .map_err(|e| Error::Http(format!("GET {url}: {e}")))?;
    let mut buf = Vec::new();
    resp.body_mut()
        .as_reader()
        .take(512 * 1024 * 1024)
        .read_to_end(&mut buf)
        .map_err(|e| Error::Http(format!("read {url}: {e}")))?;
    Ok(buf)
}

pub fn apply_file(root: &Path, zip_base: &str, f: &ManifestFile) -> Result<usize> {
    let url = format!("{}{}.zip", zip_base.trim_end_matches('/'), f.path);
    let zip_bytes = download(&url)?;
    if let Some(expect) = &f.zip_checksum {
        let got = md5_bytes(&zip_bytes);
        if !got.eq_ignore_ascii_case(expect) {
            return Err(Error::Integrity(format!(
                "zip md5 mismatch for {}: {got} != {expect}",
                f.path
            )));
        }
    }
    if let Some(expect) = f.zip_size {
        if zip_bytes.len() as u64 != expect {
            return Err(Error::Integrity(format!(
                "zip size mismatch for {}: {} != {expect}",
                f.path,
                zip_bytes.len()
            )));
        }
    }
    let cursor = std::io::Cursor::new(&zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| Error::Parse(format!("zip {}: {e}", f.path)))?;
    if archive.len() != 1 {
        return Err(Error::Parse(format!(
            "expected one entry in {}, got {}",
            f.path,
            archive.len()
        )));
    }
    let mut entry = archive
        .by_index(0)
        .map_err(|e| Error::Parse(format!("zip entry {}: {e}", f.path)))?;
    let mut content = Vec::with_capacity(f.size as usize);
    entry
        .read_to_end(&mut content)
        .map_err(|e| Error::Parse(format!("inflate {}: {e}", f.path)))?;
    drop(entry);
    if content.len() as u64 != f.size {
        return Err(Error::Integrity(format!(
            "content size mismatch for {}: {} != {}",
            f.path,
            content.len(),
            f.size
        )));
    }
    let got = md5_bytes(&content);
    if !got.eq_ignore_ascii_case(&f.checksum) {
        return Err(Error::Integrity(format!(
            "content md5 mismatch for {}: {got} != {}",
            f.path, f.checksum
        )));
    }
    let rel = f
        .path
        .trim_start_matches('/')
        .replace('/', std::path::MAIN_SEPARATOR_STR);
    let dest: PathBuf = root.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("nte-new");
    fs::write(&tmp, &content)?;
    fs::rename(&tmp, &dest)?;
    Ok(zip_bytes.len())
}

fn safe_relative_path(value: &str) -> Result<PathBuf> {
    let normalized = value.trim_start_matches('/').replace('\\', "/");
    let mut path = PathBuf::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return Err(Error::Parse(format!("无效的清单路径: {value}")));
        }
        path.push(part);
    }
    if path.as_os_str().is_empty() {
        return Err(Error::Parse("清单路径为空".into()));
    }
    Ok(path)
}

fn ensure_no_symlink(root: &Path, relative: &Path) -> Result<()> {
    let mut current = root.to_path_buf();
    for part in relative.components() {
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

/// Download a launcher ZIP into a persistent staging file, then verify and install it.
pub fn apply_file_resumable(
    root: &Path,
    zip_bases: &[String],
    file: &ManifestFile,
    on_bytes: &mut dyn FnMut(u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<u64> {
    let relative = safe_relative_path(&file.path)?;
    ensure_no_symlink(root, &relative)?;
    let key = file
        .zip_checksum
        .clone()
        .filter(|value| value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or_else(|| md5_bytes(file.path.as_bytes()));
    let staging = root.join(".oddity-downloads").join("launcher");
    fs::create_dir_all(&staging)?;
    let part = staging.join(format!("{key}.part"));
    if let Some(size) = file.zip_size {
        if part.is_file() && fs::metadata(&part)?.len() > size {
            fs::remove_file(&part)?;
        }
    }
    let mut downloaded = 0;
    let mut verified = false;
    for base in zip_bases {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        if part.is_file() {
            let length = fs::metadata(&part)?.len();
            if file.zip_size == Some(length) {
                if file.zip_checksum.as_ref().is_some_and(|checksum| {
                    md5_file(&part).is_ok_and(|hash| hash.eq_ignore_ascii_case(checksum))
                }) {
                    verified = true;
                    break;
                }
                fs::remove_file(&part)?;
            }
        }
        let existing = part.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        let url = format!(
            "{}/{}.zip",
            base.trim_end_matches('/'),
            file.path.trim_start_matches('/')
        );
        let mut request = ureq::get(&url)
            .header("User-Agent", USER_AGENT)
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(120)))
            .build();
        if existing > 0 {
            request = request.header("Range", format!("bytes={existing}-"));
        }
        let mut response = match request.call() {
            Ok(response) => response,
            Err(_) => continue,
        };
        let append = existing > 0 && response.status().as_u16() == 206;
        let mut output = if append {
            fs::OpenOptions::new().append(true).open(&part)?
        } else {
            fs::File::create(&part)?
        };
        let mut buffer = [0u8; 1024 * 1024];
        let mut reader = response.body_mut().as_reader();
        loop {
            if cancelled() {
                output.flush()?;
                return Err(Error::Cancelled);
            }
            let read = match reader.read(&mut buffer) {
                Ok(read) => read,
                Err(_) => break,
            };
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read])?;
            downloaded += read as u64;
            on_bytes(read as u64);
        }
        output.flush()?;
        let size_ok = file
            .zip_size
            .is_none_or(|size| fs::metadata(&part).is_ok_and(|metadata| metadata.len() == size));
        let hash_ok = file.zip_checksum.as_ref().is_none_or(|checksum| {
            md5_file(&part).is_ok_and(|hash| hash.eq_ignore_ascii_case(checksum))
        });
        if size_ok && hash_ok {
            verified = true;
            break;
        }
        if file
            .zip_size
            .is_some_and(|size| fs::metadata(&part).is_ok_and(|metadata| metadata.len() >= size))
        {
            fs::remove_file(&part)?;
        }
    }
    if !verified {
        return Err(Error::Integrity(format!(
            "启动器文件下载或校验失败: {}",
            file.path
        )));
    }
    let archive_file = fs::File::open(&part)?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .map_err(|error| Error::Parse(format!("ZIP {}: {error}", file.path)))?;
    if archive.len() != 1 {
        return Err(Error::Parse(format!("ZIP 条目数量错误: {}", file.path)));
    }
    let entry = archive
        .by_index(0)
        .map_err(|error| Error::Parse(format!("ZIP {}: {error}", file.path)))?;
    let destination = root.join(&relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = destination.with_extension("oddity-new");
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let written = std::io::copy(&mut entry.take(file.size.saturating_add(1)), &mut output)?;
    output.sync_all()?;
    if written != file.size {
        fs::remove_file(&temporary).ok();
        return Err(Error::Integrity(format!(
            "启动器文件大小不符: {}",
            file.path
        )));
    }
    // 官方清单中可能有零字节占位文件，ZIP 校验仍需通过。
    if file.size != 0 && !md5_file(&temporary)?.eq_ignore_ascii_case(&file.checksum) {
        fs::remove_file(&temporary).ok();
        return Err(Error::Integrity(format!(
            "启动器文件哈希不符: {}",
            file.path
        )));
    }
    fs::rename(&temporary, &destination)?;
    fs::remove_file(&part).ok();
    Ok(downloaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{HttpServer, TempDir};

    fn manifest_file(path: &str, checksum: &str, size: u64) -> ManifestFile {
        ManifestFile {
            path: path.to_string(),
            checksum: checksum.to_string(),
            size,
            zip_checksum: None,
            zip_size: None,
        }
    }

    fn manifest(files: Vec<ManifestFile>) -> Manifest {
        Manifest {
            files,
            ..Default::default()
        }
    }

    /// 打包成单条目 zip，模拟 CDN 上的 {path}.zip。
    fn zip_one(name: &str, contents: &[u8]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .expect("start zip entry");
        writer.write_all(contents).expect("write zip entry");
        writer.finish().expect("finish zip").into_inner()
    }

    fn zip_many(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, contents) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .expect("start zip entry");
            writer.write_all(contents).expect("write zip entry");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    #[test]
    fn md5_matches_known_vectors() {
        assert_eq!(md5_bytes(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_bytes(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn md5_file_streams_in_chunks() {
        let dir = TempDir::new("md5");
        // 超过 1 MiB 读缓冲，确认分块读取和一次性计算一致
        let big = vec![b'x'; 1024 * 1024 + 7];
        let path = dir.write("big.bin", &big);
        assert_eq!(md5_file(&path).expect("hash file"), md5_bytes(&big));
        assert!(md5_file(&dir.join("missing.bin")).is_err());
    }

    #[test]
    fn check_reports_missing_changed_and_ok() {
        let dir = TempDir::new("check");
        dir.write("present.txt", b"hello");
        dir.write("samesize.txt", b"world");
        dir.write("short.txt", b"hi");
        let manifest = manifest(vec![
            manifest_file("/present.txt", &md5_bytes(b"hello"), 5),
            manifest_file("/samesize.txt", &md5_bytes(b"other"), 5),
            manifest_file("/short.txt", &md5_bytes(b"hi"), 9),
            manifest_file("/gone.txt", &md5_bytes(b"x"), 1),
        ]);
        let report = check(dir.path(), &manifest, false).expect("check");
        assert_eq!(report.checked, 4);
        assert_eq!(report.ok, 1);
        let missing: Vec<&str> = report.missing.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(missing, vec!["/gone.txt"]);
        let changed: Vec<&str> = report.changed.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(changed, vec!["/samesize.txt", "/short.txt"]);
        assert_eq!(report.total_missing_bytes(), 15);
    }

    #[test]
    fn quick_check_trusts_a_matching_size() {
        let dir = TempDir::new("check-quick");
        dir.write("a.txt", b"hello");
        let manifest = manifest(vec![manifest_file("/a.txt", "deadbeef", 5)]);
        let report = check(dir.path(), &manifest, true).expect("quick check");
        assert_eq!(report.checked, 1);
        assert_eq!(report.ok, 1);
        assert!(report.changed.is_empty());
        assert_eq!(report.total_missing_bytes(), 0);
    }

    #[test]
    fn apply_file_downloads_and_installs_content() {
        let dir = TempDir::new("apply");
        let content = b"new content";
        let zip = zip_one("nested/file.txt", content);
        let server = HttpServer::start();
        server.route("/files/nested/file.txt.zip", &zip);
        let file = ManifestFile {
            path: "/nested/file.txt".into(),
            checksum: md5_bytes(content),
            size: content.len() as u64,
            zip_checksum: Some(md5_bytes(&zip)),
            zip_size: Some(zip.len() as u64),
        };
        let written =
            apply_file(dir.path(), &format!("{}/files/", server.base()), &file).expect("apply");
        assert_eq!(written, zip.len());
        assert_eq!(dir.read("nested/file.txt"), content);
    }

    #[test]
    fn resumable_file_uses_verified_part_and_installs_atomically() {
        let dir = TempDir::new("apply-resume");
        let content = b"verified launcher file";
        let zip = zip_one("file.bin", content);
        let zip_md5 = md5_bytes(&zip);
        let file = ManifestFile {
            path: "/file.bin".into(),
            checksum: md5_bytes(content),
            size: content.len() as u64,
            zip_checksum: Some(zip_md5.clone()),
            zip_size: Some(zip.len() as u64),
        };
        let part = format!(".oddity-downloads/launcher/{zip_md5}.part");
        dir.write(&part, &zip[..zip.len() / 2]);
        let server = HttpServer::start();
        server.route("/files/file.bin.zip", &zip);
        let mut observed = 0;
        let bytes = apply_file_resumable(
            dir.path(),
            &[format!("{}/files", server.base())],
            &file,
            &mut |amount| observed += amount,
            &|| false,
        )
        .expect("resume launcher file");
        assert_eq!(bytes, (zip.len() - zip.len() / 2) as u64);
        assert_eq!(observed, bytes);
        assert_eq!(dir.read("file.bin"), content);
        assert!(!dir.is_file(&part));
        assert!(!dir.is_file("file.oddity-new"));
    }

    #[test]
    fn resumable_file_replaces_a_corrupt_complete_part() {
        let dir = TempDir::new("apply-corrupt-part");
        let content = b"fresh content";
        let zip = zip_one("file.bin", content);
        let zip_md5 = md5_bytes(&zip);
        let file = ManifestFile {
            path: "/file.bin".into(),
            checksum: md5_bytes(content),
            size: content.len() as u64,
            zip_checksum: Some(zip_md5.clone()),
            zip_size: Some(zip.len() as u64),
        };
        let part = format!(".oddity-downloads/launcher/{zip_md5}.part");
        dir.write(&part, vec![0; zip.len()]);
        let server = HttpServer::start();
        server.route("/files/file.bin.zip", &zip);
        apply_file_resumable(
            dir.path(),
            &[format!("{}/files", server.base())],
            &file,
            &mut |_| {},
            &|| false,
        )
        .expect("redownload corrupt part");
        assert_eq!(dir.read("file.bin"), content);
    }

    #[test]
    fn cancelled_launcher_transfer_keeps_a_reusable_part() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let dir = TempDir::new("apply-cancel-part");
        let content = b"keep this content";
        let zip = zip_one("file.bin", content);
        let zip_md5 = md5_bytes(&zip);
        let file = ManifestFile {
            path: "/file.bin".into(),
            checksum: md5_bytes(content),
            size: content.len() as u64,
            zip_checksum: Some(zip_md5.clone()),
            zip_size: Some(zip.len() as u64),
        };
        let server = HttpServer::start();
        server.route("/files/file.bin.zip", &zip);
        let stopped = AtomicBool::new(false);
        let bases = [format!("{}/files", server.base())];
        let result = apply_file_resumable(
            dir.path(),
            &bases,
            &file,
            &mut |_| stopped.store(true, Ordering::Relaxed),
            &|| stopped.load(Ordering::Relaxed),
        );
        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(!dir.is_file("file.bin"));
        assert!(dir.is_file(format!(".oddity-downloads/launcher/{zip_md5}.part")));
        apply_file_resumable(dir.path(), &bases, &file, &mut |_| {}, &|| false)
            .expect("resume cancelled transfer");
        assert_eq!(dir.read("file.bin"), content);
        assert_eq!(server.hits("/files/file.bin.zip"), 1);
    }

    #[test]
    fn apply_file_rejects_bad_zip_metadata() {
        let dir = TempDir::new("apply-zip-meta");
        let content = b"payload";
        let zip = zip_one("p.bin", content);
        let server = HttpServer::start();
        server.route("/p.bin.zip", &zip);

        let mut bad_zip_md5 = manifest_file("/p.bin", &md5_bytes(content), content.len() as u64);
        bad_zip_md5.zip_checksum = Some(md5_bytes(b"other"));
        let error = apply_file(dir.path(), &server.base(), &bad_zip_md5).expect_err("zip md5");
        assert!(error.to_string().contains("zip md5 mismatch"), "{error}");

        let mut bad_zip_size = manifest_file("/p.bin", &md5_bytes(content), content.len() as u64);
        bad_zip_size.zip_size = Some(zip.len() as u64 + 1);
        let error = apply_file(dir.path(), &server.base(), &bad_zip_size).expect_err("zip size");
        assert!(error.to_string().contains("zip size mismatch"), "{error}");
    }

    #[test]
    fn apply_file_rejects_bad_content() {
        let dir = TempDir::new("apply-content");
        let content = b"payload";
        let zip = zip_one("p.bin", content);
        let server = HttpServer::start();
        server.route("/p.bin.zip", &zip);

        let wrong_size = manifest_file("/p.bin", &md5_bytes(content), content.len() as u64 + 1);
        let error = apply_file(dir.path(), &server.base(), &wrong_size).expect_err("size mismatch");
        assert!(
            error.to_string().contains("content size mismatch"),
            "{error}"
        );

        let wrong_md5 = manifest_file("/p.bin", "0000", content.len() as u64);
        let error = apply_file(dir.path(), &server.base(), &wrong_md5).expect_err("md5 mismatch");
        assert!(
            error.to_string().contains("content md5 mismatch"),
            "{error}"
        );
        assert!(!dir.is_file("p.bin"));
    }

    #[test]
    fn apply_file_requires_exactly_one_zip_entry() {
        let dir = TempDir::new("apply-multi");
        let content = b"payload";
        let zip = zip_many(&[("a.bin", content), ("b.bin", content)]);
        let server = HttpServer::start();
        server.route("/p.bin.zip", &zip);
        let file = manifest_file("/p.bin", &md5_bytes(content), content.len() as u64);
        let error = apply_file(dir.path(), &server.base(), &file).expect_err("two entries");
        assert!(error.to_string().contains("expected one entry"), "{error}");
    }

    #[test]
    fn apply_file_reports_a_broken_zip() {
        let dir = TempDir::new("apply-broken");
        let server = HttpServer::start();
        server.route("/p.bin.zip", b"not a zip at all");
        let file = manifest_file("/p.bin", "0000", 4);
        let error = apply_file(dir.path(), &server.base(), &file).expect_err("broken zip");
        assert!(error.to_string().contains("zip /p.bin"), "{error}");
    }

    #[test]
    fn apply_file_reports_download_failures() {
        let dir = TempDir::new("apply-404");
        let server = HttpServer::start();
        let file = manifest_file("/p.bin", "0000", 4);
        let error = apply_file(dir.path(), &server.base(), &file).expect_err("404");
        assert!(error.to_string().contains("p.bin.zip"), "{error}");
    }
}
