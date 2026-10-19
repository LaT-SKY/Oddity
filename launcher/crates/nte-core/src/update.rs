use std::fs;
use std::io::Read;
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
        if n == 0 { break; }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

pub fn check(root: &Path, manifest: &Manifest, quick: bool) -> Result<CheckReport> {
    let mut report = CheckReport::default();
    for f in &manifest.files {
        report.checked += 1;
        let rel = f.path.trim_start_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR);
        let local = root.join(rel);
        let meta = match fs::metadata(&local) {
            Ok(m) => m,
            Err(_) => { report.missing.push(f.clone()); continue; }
        };
        if meta.len() != f.size {
            report.changed.push(f.clone());
            continue;
        }
        if quick {
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
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| Error::Http(format!("GET {url}: {e}")))?;
    let mut buf = Vec::new();
    resp.into_reader()
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
            return Err(Error::Integrity(format!("zip md5 mismatch for {}: {got} != {expect}", f.path)));
        }
    }
    if let Some(expect) = f.zip_size {
        if zip_bytes.len() as u64 != expect {
            return Err(Error::Integrity(format!("zip size mismatch for {}: {} != {expect}", f.path, zip_bytes.len())));
        }
    }
    let cursor = std::io::Cursor::new(&zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| Error::Parse(format!("zip {}: {e}", f.path)))?;
    if archive.len() != 1 {
        return Err(Error::Parse(format!("expected one entry in {}, got {}", f.path, archive.len())));
    }
    let mut entry = archive.by_index(0).map_err(|e| Error::Parse(format!("zip entry {}: {e}", f.path)))?;
    let mut content = Vec::with_capacity(f.size as usize);
    entry.read_to_end(&mut content).map_err(|e| Error::Parse(format!("inflate {}: {e}", f.path)))?;
    drop(entry);
    if content.len() as u64 != f.size {
        return Err(Error::Integrity(format!("content size mismatch for {}: {} != {}", f.path, content.len(), f.size)));
    }
    let got = md5_bytes(&content);
    if !got.eq_ignore_ascii_case(&f.checksum) {
        return Err(Error::Integrity(format!("content md5 mismatch for {}: {got} != {}", f.path, f.checksum)));
    }
    let rel = f.path.trim_start_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR);
    let dest: PathBuf = root.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("nte-new");
    fs::write(&tmp, &content)?;
    fs::rename(&tmp, &dest)?;
    Ok(zip_bytes.len())
}
