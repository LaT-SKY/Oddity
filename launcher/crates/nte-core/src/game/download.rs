use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use crate::game::check::{join_install_path, PatchRef, UpdateItem, UpdateKind, UpdatePlan};
use crate::game::config::GameUpdateConfig;
use crate::update::md5_file;
use crate::{Error, Result, USER_AGENT};

#[derive(Debug, Clone)]
pub struct ApplyOptions {
    pub dry_run: bool,
    pub only: Option<String>,
    pub force: bool,
    pub update_ledger: bool,
}

impl Default for ApplyOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            only: None,
            force: false,
            update_ledger: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub path: String,
    pub bytes: u64,
    pub ok: bool,
    pub error: Option<String>,
    pub skipped: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ApplyReport {
    pub results: Vec<ApplyResult>,
    pub downloaded_bytes: u64,
    pub failed: usize,
    pub skipped: usize,
    pub ledger_error: Option<String>,
}

pub fn apply_plan(
    config: &GameUpdateConfig,
    plan: &UpdatePlan,
    opts: &ApplyOptions,
) -> ApplyReport {
    let mut report = ApplyReport::default();
    let staging = config.patcher_dir.join("tmp").join("nte-native");
    let selected: Vec<&UpdateItem> = plan
        .items
        .iter()
        .filter(|i| match &opts.only {
            Some(filter) => i.path.contains(filter),
            None => true,
        })
        .collect();

    for item in selected {
        if opts.dry_run {
            report.results.push(ApplyResult {
                path: item.path.clone(),
                bytes: 0,
                ok: true,
                error: None,
                skipped: true,
            });
            report.skipped += 1;
            continue;
        }
        match download_item(config, item, &staging, opts.force) {
            Ok(bytes) => {
                report.downloaded_bytes += bytes;
                report.results.push(ApplyResult {
                    path: item.path.clone(),
                    bytes,
                    ok: true,
                    error: None,
                    skipped: false,
                });
            }
            Err(e) => {
                report.failed += 1;
                report.results.push(ApplyResult {
                    path: item.path.clone(),
                    bytes: 0,
                    ok: false,
                    error: Some(e.to_string()),
                    skipped: false,
                });
            }
        }
    }
    if opts.update_ledger && !opts.dry_run {
        let ok_paths: Vec<String> = report
            .results
            .iter()
            .filter(|r| r.ok && !r.skipped)
            .map(|r| r.path.clone())
            .collect();
        if let Err(e) = crate::game::ledger::update_after_apply(config, plan, &ok_paths) {
            report.ledger_error = Some(e.to_string());
        }
    }
    report
}

fn download_item(
    config: &GameUpdateConfig,
    item: &UpdateItem,
    staging: &Path,
    force: bool,
) -> Result<u64> {
    let target = join_install_path(&config.install_root, &item.path);
    if !force {
        if let Ok(meta) = fs::metadata(&target) {
            if meta.len() == item.size {
                if let Ok(got) = md5_file(&target) {
                    if got.eq_ignore_ascii_case(&item.md5) {
                        return Ok(0);
                    }
                }
            }
        }
    }
    if let Some(patch) = &item.patch {
        if target.is_file() {
            match download_and_patch(config, patch, &target, staging) {
                Ok(bytes) => return Ok(bytes),
                Err(_) => {
                    // A missing/corrupt hpatchz, an old file that no longer
                    // matches the ledger, or a bad patch all fall back to the
                    // full content-addressed file.
                }
            }
        }
    }
    match &item.kind {
        UpdateKind::Res { res } => download_res(config, res, &target, staging),
        UpdateKind::PakEntry {
            pak_md5,
            pak_size,
            entry,
        } => download_pak_entry(config, pak_md5, *pak_size, entry, &target, staging),
    }
}

fn download_and_patch(
    config: &GameUpdateConfig,
    patch: &PatchRef,
    target: &Path,
    staging: &Path,
) -> Result<u64> {
    let url = res_url(config, &patch.md5, patch.size);
    let patch_temp = staging
        .join("Patch")
        .join(format!("{}.{}.part", patch.md5, patch.size));
    if let Some(parent) = patch_temp.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = download_to_temp(config, &url, &patch_temp, patch.size, &patch.md5)?;
    let new_temp = staging
        .join("Patch")
        .join(format!("{}.{}.new", patch.new_md5, patch.new_size));
    crate::game::patch::apply_hdiff(target, &patch_temp, &new_temp)?;
    if !verify(&new_temp, patch.new_size, &patch.new_md5)? {
        fs::remove_file(&new_temp).ok();
        fs::remove_file(&patch_temp).ok();
        return Err(Error::Integrity(format!(
            "patched output mismatch for {}",
            target.display()
        )));
    }
    install_temp(&new_temp, target)?;
    fs::remove_file(&patch_temp).ok();
    Ok(bytes)
}

fn download_res(
    config: &GameUpdateConfig,
    res: &crate::game::manifest::ResEntry,
    target: &Path,
    staging: &Path,
) -> Result<u64> {
    let url = res_url(config, &res.md5, res.filesize);
    let temp = staging.join("Res").join(format!(
        "{}.{}.part",
        res.md5.to_ascii_lowercase(),
        res.filesize
    ));
    if let Some(parent) = temp.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = download_to_temp(config, &url, &temp, res.filesize, &res.md5)?;
    install_temp(&temp, target)?;
    Ok(bytes)
}

fn download_pak_entry(
    config: &GameUpdateConfig,
    pak_md5: &str,
    pak_size: u64,
    entry: &crate::game::manifest::PakEntry,
    target: &Path,
    staging: &Path,
) -> Result<u64> {
    if entry.md5.len() != 32 || entry.offset.checked_add(entry.size).is_none() {
        return Err(Error::Integrity(format!(
            "bad PatcherPak entry for {}",
            entry.name
        )));
    }
    let url = res_url(config, pak_md5, pak_size);
    let temp = staging
        .join("Pak")
        .join(format!("{}_{}.part", entry.md5, entry.size));
    if let Some(parent) = temp.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = download_range_to_temp(config, &url, &temp, entry.offset, entry.size, &entry.md5)?;
    install_temp(&temp, target)?;
    Ok(bytes)
}

pub fn res_url(config: &GameUpdateConfig, md5: &str, size: u64) -> String {
    let md5 = md5.to_ascii_lowercase();
    let first = md5.chars().next().unwrap_or('0');
    let cdn = config
        .cdns
        .first()
        .map(String::as_str)
        .unwrap_or("https://yhcdn1.wmupd.com/clientRes");
    format!(
        "{}/{}/Res/{}/{}.{}",
        cdn.trim_end_matches('/'),
        config.branch,
        first,
        md5,
        size
    )
}

fn download_to_temp(
    config: &GameUpdateConfig,
    url: &str,
    temp: &Path,
    expected_size: u64,
    expected_md5: &str,
) -> Result<u64> {
    if temp.is_file() {
        let len = fs::metadata(temp)?.len();
        if len == expected_size {
            if verify(temp, expected_size, expected_md5)? {
                return Ok(0);
            }
            fs::remove_file(temp)?;
        } else if len > expected_size {
            fs::remove_file(temp)?;
        }
    }
    let mut last_error = None;
    for (i, cdn) in config.cdns.iter().enumerate() {
        let url = if i == 0 {
            url.to_string()
        } else {
            let suffix = url.splitn(2, "/Res/").nth(1).unwrap_or("");
            format!(
                "{}/{}/Res/{}",
                cdn.trim_end_matches('/'),
                config.branch,
                suffix
            )
        };
        let existing = if temp.is_file() {
            fs::metadata(temp)?.len()
        } else {
            0
        };
        match fetch_to_file(&url, temp, existing, None) {
            Ok(new_bytes) => {
                if verify(temp, expected_size, expected_md5)? {
                    return Ok(new_bytes);
                }
                fs::remove_file(temp).ok();
                last_error = Some(Error::Integrity(format!(
                    "download md5 mismatch for {}",
                    temp.display()
                )));
            }
            Err(e) => {
                last_error = Some(e);
                // Keep the partial file for a resume on the next mirror.
            }
        }
    }
    Err(last_error.unwrap_or_else(|| Error::Http(format!("download failed: {url}"))))
}

fn download_range_to_temp(
    config: &GameUpdateConfig,
    url: &str,
    temp: &Path,
    offset: u64,
    size: u64,
    expected_md5: &str,
) -> Result<u64> {
    if temp.is_file() {
        let len = fs::metadata(temp)?.len();
        if len == size {
            if verify(temp, size, expected_md5)? {
                return Ok(0);
            }
            fs::remove_file(temp)?;
        } else if len > size {
            fs::remove_file(temp)?;
        }
    }
    let end = offset
        .checked_add(size)
        .and_then(|v| v.checked_sub(1))
        .ok_or_else(|| Error::Integrity("PatcherPak range overflow".into()))?;
    let mut last_error = None;
    for (i, cdn) in config.cdns.iter().enumerate() {
        let attempt_url = if i == 0 {
            url.to_string()
        } else {
            let suffix = url.splitn(2, "/Res/").nth(1).unwrap_or("");
            format!(
                "{}/{}/Res/{}",
                cdn.trim_end_matches('/'),
                config.branch,
                suffix
            )
        };
        let existing = if temp.is_file() {
            fs::metadata(temp)?.len()
        } else {
            0
        };
        let range = if existing > 0 {
            format!("bytes={}-{}", offset + existing, end)
        } else {
            format!("bytes={offset}-{end}")
        };
        match fetch_to_file(&attempt_url, temp, existing, Some(&range)) {
            Ok(new_bytes) => {
                if verify(temp, size, expected_md5)? {
                    return Ok(new_bytes);
                }
                fs::remove_file(temp).ok();
                last_error = Some(Error::Integrity(format!(
                    "PatcherPak entry md5 mismatch for {}",
                    temp.display()
                )));
            }
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error.unwrap_or_else(|| Error::Http("PatcherPak range download failed".into())))
}

fn fetch_to_file(url: &str, temp: &Path, existing: u64, range: Option<&str>) -> Result<u64> {
    let mut req = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .timeout(std::time::Duration::from_secs(120));
    if let Some(range) = range {
        req = req.set("Range", range);
    } else if existing > 0 {
        req = req.set("Range", &format!("bytes={existing}-"));
    }
    let resp = match req.call() {
        Ok(r) => r,
        Err(ureq::Error::Status(416, _)) if existing > 0 => return Ok(0),
        Err(e) => return Err(Error::Http(format!("GET {url}: {e}"))),
    };
    let status = resp.status();
    let append = status == 206 && existing > 0;
    let mut out = if append {
        OpenOptions::new().append(true).open(temp)?
    } else {
        File::create(temp)?
    };
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut total = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| Error::Http(format!("read {url}: {e}")))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        total += n as u64;
    }
    out.flush()?;
    if status != 200 && status != 206 {
        return Err(Error::Http(format!("GET {url}: HTTP {status}")));
    }
    Ok(total)
}

fn verify(path: &Path, expected_size: u64, expected_md5: &str) -> Result<bool> {
    let meta = fs::metadata(path)?;
    if meta.len() != expected_size {
        return Ok(false);
    }
    let got = md5_file(path)?;
    Ok(got.eq_ignore_ascii_case(expected_md5))
}

fn install_temp(temp: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(temp, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(temp, target)?;
            fs::remove_file(temp)?;
            Ok(())
        }
    }
}
