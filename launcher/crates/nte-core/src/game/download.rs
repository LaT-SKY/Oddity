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
            let suffix = url.split_once("/Res/").map(|x| x.1).unwrap_or("");
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
            let suffix = url.split_once("/Res/").map(|x| x.1).unwrap_or("");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::check::{UpdatePlan, UpdateReason};
    use crate::game::manifest::{PakEntry, ResEntry};
    use crate::testing::{write_script, EnvGuard, HttpServer, TempDir};

    const BRANCH: &str = "publish_ob";

    fn config_for(dir: &TempDir) -> GameUpdateConfig {
        GameUpdateConfig {
            launcher_root: dir.join("install/NTELauncher"),
            install_root: dir.join("install"),
            config_dir: dir.join("install/NTELauncher/ResFilesM/1289/PatcherConfig"),
            patcher_dir: dir.join("install/NTELauncher/UserData/Patcher/PatcherSDK"),
            branch: BRANCH.to_string(),
            update_branch: "publish_Updata".to_string(),
            app_id: "1289".to_string(),
            cdns: vec!["https://cdn.test/clientRes".to_string()],
            rules: Vec::new(),
            check_dirs: Vec::new(),
            max_threads: 5,
        }
    }

    fn res_item(path: &str, md5: &str, size: u64) -> UpdateItem {
        UpdateItem {
            path: path.to_string(),
            size,
            md5: md5.to_string(),
            reason: UpdateReason::Missing,
            kind: UpdateKind::Res {
                res: ResEntry {
                    filename: path.to_string(),
                    filesize: size,
                    md5: md5.to_string(),
                    timestamp: None,
                    check: false,
                    blocks: Vec::new(),
                },
            },
            patch: None,
            download_size: size,
        }
    }

    fn pak_item(path: &str, entry_md5: &str, offset: u64, size: u64, pak_size: u64) -> UpdateItem {
        UpdateItem {
            path: path.to_string(),
            size,
            md5: entry_md5.to_string(),
            reason: UpdateReason::Missing,
            kind: UpdateKind::PakEntry {
                pak_md5: "aa".repeat(16),
                pak_size,
                entry: PakEntry {
                    name: path.to_string(),
                    start: 0,
                    offset,
                    size,
                    md5: entry_md5.to_string(),
                    check: false,
                },
            },
            patch: None,
            download_size: size,
        }
    }

    fn plan_of(items: Vec<UpdateItem>) -> UpdatePlan {
        UpdatePlan {
            remote_version: "1.4.16".to_string(),
            remote_tag: "baseTag".to_string(),
            remote_hash: "HASH".to_string(),
            items,
            ..Default::default()
        }
    }

    fn res_path(md5: &str, size: u64) -> String {
        let md5 = md5.to_ascii_lowercase();
        format!(
            "/cdn/{BRANCH}/Res/{}/{md5}.{size}",
            md5.chars().next().unwrap_or('0')
        )
    }

    #[test]
    fn res_url_uses_the_first_cdn_and_a_lowercase_md5() {
        let dir = TempDir::new("dl-url");
        let mut config = config_for(&dir);
        config.cdns = vec![
            "https://cdn.test/clientRes/".to_string(),
            "https://backup.test/res".to_string(),
        ];
        assert_eq!(
            res_url(&config, &"AB".repeat(16), 12),
            format!(
                "https://cdn.test/clientRes/{BRANCH}/Res/a/{}.12",
                "ab".repeat(16)
            )
        );

        config.cdns.clear();
        assert_eq!(
            res_url(&config, "", 0),
            format!("https://yhcdn1.wmupd.com/clientRes/{BRANCH}/Res/0/.0")
        );
    }

    #[test]
    fn dry_run_reports_without_downloading() {
        let dir = TempDir::new("dl-dry");
        let config = config_for(&dir);
        let plan = plan_of(vec![
            res_item("data/a.bin", &"aa".repeat(16), 3),
            res_item("data/b.bin", &"bb".repeat(16), 4),
        ]);
        let report = apply_plan(
            &config,
            &plan,
            &ApplyOptions {
                dry_run: true,
                ..Default::default()
            },
        );
        assert_eq!(report.results.len(), 2);
        assert!(report
            .results
            .iter()
            .all(|r| r.ok && r.skipped && r.bytes == 0));
        assert_eq!(report.skipped, 2);
        assert_eq!(report.downloaded_bytes, 0);
        assert_eq!(report.failed, 0);
        assert!(report.ledger_error.is_none());
        assert!(!dir.is_file("install/data/a.bin"));

        let report = apply_plan(
            &config,
            &plan,
            &ApplyOptions {
                dry_run: true,
                only: Some("b.bin".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].path, "data/b.bin");
    }

    #[test]
    fn apply_plan_downloads_res_files_and_updates_the_ledger() {
        let dir = TempDir::new("dl-apply");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let content = b"payload bytes";
        let md5 = crate::update::md5_bytes(content);
        server.route(res_path(&md5, content.len() as u64), content);

        let report = apply_plan(
            &config,
            &plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]),
            &ApplyOptions::default(),
        );
        assert_eq!(report.failed, 0);
        assert!(report.results[0].ok);
        assert!(!report.results[0].skipped);
        assert_eq!(report.downloaded_bytes, content.len() as u64);
        assert_eq!(dir.read("install/data/a.bin"), content);

        // 账本写回后，下一次检查可以直接信任记录
        let ledger = crate::game::check::load_local_ledger(&config)
            .expect("load ledger")
            .expect("ledger written");
        assert_eq!(ledger.version, "1.4.16");
        assert_eq!(ledger.tag, "baseTag");
        assert_eq!(ledger.res[0].filename, "data/a.bin");
        assert_eq!(ledger.res[0].md5, md5);
    }

    #[test]
    fn apply_plan_skips_files_that_already_match() {
        let dir = TempDir::new("dl-skip");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let content = b"payload bytes";
        let md5 = crate::update::md5_bytes(content);
        let path = res_path(&md5, content.len() as u64);
        server.route(&path, content);
        dir.write("install/data/a.bin", content);

        let plan = plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]);
        let report = apply_plan(&config, &plan, &ApplyOptions::default());
        assert_eq!(report.downloaded_bytes, 0);
        assert_eq!(report.results[0].bytes, 0);
        assert_eq!(server.hits(&path), 0);

        // force 会绕过这个快速路径
        let report = apply_plan(
            &config,
            &plan,
            &ApplyOptions {
                force: true,
                ..Default::default()
            },
        );
        assert_eq!(report.downloaded_bytes, content.len() as u64);
        assert_eq!(server.hits(&path), 1);
    }

    #[test]
    fn apply_plan_records_failures_without_touching_the_ledger() {
        let dir = TempDir::new("dl-fail");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];

        let report = apply_plan(
            &config,
            &plan_of(vec![res_item("data/a.bin", &"aa".repeat(16), 5)]),
            &ApplyOptions {
                update_ledger: false,
                ..Default::default()
            },
        );
        assert_eq!(report.failed, 1);
        assert!(!report.results[0].ok);
        assert!(report.results[0]
            .error
            .as_deref()
            .expect("error text")
            .contains("404"));
        assert!(report.ledger_error.is_none());
        assert!(!config.patcher_dir.join("ResList.xml").exists());
    }

    #[test]
    fn apply_plan_reuses_a_verified_partial_download() {
        let dir = TempDir::new("dl-reuse");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let content = b"payload bytes";
        let md5 = crate::update::md5_bytes(content);
        let path = res_path(&md5, content.len() as u64);
        server.route(&path, content);

        let part = config
            .patcher_dir
            .join("tmp/nte-native/Res")
            .join(format!("{md5}.{}.part", content.len()));
        fs::create_dir_all(part.parent().expect("parent")).expect("create staging");
        fs::write(&part, content).expect("write part");

        let report = apply_plan(
            &config,
            &plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]),
            &ApplyOptions::default(),
        );
        assert_eq!(report.downloaded_bytes, 0);
        assert_eq!(server.hits(&path), 0);
        assert_eq!(dir.read("install/data/a.bin"), content);
    }

    #[test]
    fn apply_plan_resumes_an_incomplete_download() {
        let dir = TempDir::new("dl-resume");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let content: Vec<u8> = (0..=255u8).collect();
        let md5 = crate::update::md5_bytes(&content);
        let path = res_path(&md5, content.len() as u64);
        server.route(&path, &content);

        let part = config
            .patcher_dir
            .join("tmp/nte-native/Res")
            .join(format!("{md5}.{}.part", content.len()));
        fs::create_dir_all(part.parent().expect("parent")).expect("create staging");
        fs::write(&part, &content[..100]).expect("write partial");

        let report = apply_plan(
            &config,
            &plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]),
            &ApplyOptions::default(),
        );
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        // 只补齐了缺少的部分
        assert_eq!(report.downloaded_bytes, content.len() as u64 - 100);
        assert_eq!(dir.read("install/data/a.bin"), content);
        assert!(!part.exists());
    }

    #[test]
    fn apply_plan_discards_unusable_partial_downloads() {
        let dir = TempDir::new("dl-discard");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let content = b"payload bytes";
        let md5 = crate::update::md5_bytes(content);
        let path = res_path(&md5, content.len() as u64);
        server.route(&path, content);

        let part = config
            .patcher_dir
            .join("tmp/nte-native/Res")
            .join(format!("{md5}.{}.part", content.len()));
        fs::create_dir_all(part.parent().expect("parent")).expect("create staging");

        // 同尺寸但内容损坏：必须先校验再丢弃
        fs::write(&part, b"wrong content").expect("write broken part");
        let report = apply_plan(
            &config,
            &plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]),
            &ApplyOptions::default(),
        );
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(report.downloaded_bytes, content.len() as u64);
        assert_eq!(dir.read("install/data/a.bin"), content);

        // 超出预期大小的残留文件直接删除重下
        fs::create_dir_all(part.parent().expect("parent")).expect("create staging");
        fs::write(&part, vec![0u8; content.len() + 5]).expect("write oversized part");
        let report = apply_plan(
            &config,
            &plan_of(vec![res_item("data/b.bin", &md5, content.len() as u64)]),
            &ApplyOptions::default(),
        );
        assert_eq!(report.downloaded_bytes, content.len() as u64);
        assert_eq!(dir.read("install/data/b.bin"), content);
    }

    #[test]
    fn apply_plan_reports_a_partial_download_that_cannot_be_resumed() {
        let dir = TempDir::new("dl-416");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let md5 = "ab".repeat(16);
        // 服务端内容比已存在的分片还短：Range 请求会得到 416
        server.route(res_path(&md5, 100), b"short");

        let part = config
            .patcher_dir
            .join("tmp/nte-native/Res")
            .join(format!("{md5}.100.part"));
        fs::create_dir_all(part.parent().expect("parent")).expect("create staging");
        fs::write(&part, vec![0u8; 50]).expect("write partial");

        let report = apply_plan(
            &config,
            &plan_of(vec![res_item("data/a.bin", &md5, 100)]),
            &ApplyOptions::default(),
        );
        assert_eq!(report.failed, 1);
        assert!(report.results[0]
            .error
            .as_deref()
            .expect("error text")
            .contains("download md5 mismatch"));
    }

    #[test]
    fn download_falls_back_to_the_next_cdn() {
        let dir = TempDir::new("dl-cdn");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/bad"), server.url("/good")];
        let content = b"payload bytes";
        let md5 = crate::update::md5_bytes(content);
        let suffix = format!(
            "{BRANCH}/Res/{}/{md5}.{}",
            md5.chars().next().unwrap_or('0'),
            content.len()
        );
        server.route(format!("/good/{suffix}"), content);

        let report = apply_plan(
            &config,
            &plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]),
            &ApplyOptions::default(),
        );
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), content);
        assert_eq!(server.hits(&format!("/bad/{suffix}")), 1);
        assert_eq!(server.hits(&format!("/good/{suffix}")), 1);
    }

    #[test]
    fn download_verifies_the_content_md5() {
        let dir = TempDir::new("dl-md5");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let content = b"payload bytes";
        let md5 = "ff".repeat(16);
        let path = res_path(&md5, content.len() as u64);
        server.route(&path, content);

        let report = apply_plan(
            &config,
            &plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]),
            &ApplyOptions::default(),
        );
        assert_eq!(report.failed, 1);
        assert!(report.results[0]
            .error
            .as_deref()
            .expect("error text")
            .contains("download md5 mismatch"));
        assert!(!dir.is_file("install/data/a.bin"));
    }

    #[test]
    fn pak_entries_are_downloaded_with_a_range_request() {
        let dir = TempDir::new("dl-pak");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let pak: Vec<u8> = (0..=255u8).collect();
        let entry_bytes = pak[10..40].to_vec();
        let entry_md5 = crate::update::md5_bytes(&entry_bytes);
        let pak_md5 = "aa".repeat(16);
        server.route(res_path(&pak_md5, pak.len() as u64), &pak);

        let report = apply_plan(
            &config,
            &plan_of(vec![pak_item(
                "Client/Content/Paks/pakchunk0.pak",
                &entry_md5,
                10,
                entry_bytes.len() as u64,
                pak.len() as u64,
            )]),
            &ApplyOptions::default(),
        );
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(report.downloaded_bytes, entry_bytes.len() as u64);
        assert_eq!(
            dir.read("install/Client/Content/Paks/pakchunk0.pak"),
            entry_bytes
        );
    }

    #[test]
    fn pak_entries_with_bad_ids_are_rejected_before_downloading() {
        let dir = TempDir::new("dl-pak-bad");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];

        // md5 长度不足
        let report = apply_plan(
            &config,
            &plan_of(vec![pak_item("data/a.pak", "short", 0, 4, 100)]),
            &ApplyOptions::default(),
        );
        assert!(report.results[0]
            .error
            .as_deref()
            .expect("error text")
            .contains("bad PatcherPak entry"));

        // offset + size 溢出
        let report = apply_plan(
            &config,
            &plan_of(vec![pak_item(
                "data/b.pak",
                &"bb".repeat(16),
                u64::MAX,
                1,
                100,
            )]),
            &ApplyOptions::default(),
        );
        assert!(report.results[0]
            .error
            .as_deref()
            .expect("error text")
            .contains("bad PatcherPak entry"));
        assert!(server.targets().is_empty(), "{:?}", server.targets());
    }

    #[test]
    fn patches_are_applied_when_hpatchz_is_available() {
        let mut env = EnvGuard::new();
        let dir = TempDir::new("dl-patch");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let old_content = vec![b'o'; 100];
        let new_content = vec![b'n'; 200];
        let new_md5 = crate::update::md5_bytes(&new_content);
        dir.write("install/data/a.bin", &old_content);

        // 假 hpatchz：把补丁文件当成新内容复制到输出路径
        let script = write_script(dir.path(), "hpatchz", "#!/bin/sh\ncp \"$3\" \"$4\"\n");
        env.set("NTE_HPATCHZ", &script);

        let patch_md5 = new_md5.clone();
        server.route(res_path(&patch_md5, new_content.len() as u64), &new_content);
        let mut item = res_item("data/a.bin", &new_md5, new_content.len() as u64);
        item.reason = UpdateReason::Md5;
        item.patch = Some(PatchRef {
            md5: patch_md5,
            size: new_content.len() as u64,
            new_md5: new_md5.clone(),
            new_size: new_content.len() as u64,
        });
        item.download_size = new_content.len() as u64;

        let report = apply_plan(&config, &plan_of(vec![item]), &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), new_content);
        assert_eq!(report.downloaded_bytes, new_content.len() as u64);
    }

    #[test]
    fn patch_failures_fall_back_to_the_full_file() {
        let dir = TempDir::new("dl-patch-fallback");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let new_content = b"full content";
        let new_md5 = crate::update::md5_bytes(new_content);
        dir.write("install/data/a.bin", b"old");
        // 补丁本身取不到：下载失败后必须回到整包下载
        server.route(res_path(&new_md5, new_content.len() as u64), new_content);

        let mut item = res_item("data/a.bin", &new_md5, new_content.len() as u64);
        item.reason = UpdateReason::Md5;
        item.patch = Some(PatchRef {
            md5: "ee".repeat(16),
            size: 4,
            new_md5: new_md5.clone(),
            new_size: new_content.len() as u64,
        });

        let report = apply_plan(&config, &plan_of(vec![item]), &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), new_content);
    }

    #[test]
    fn a_bad_patch_output_is_rejected() {
        let mut env = EnvGuard::new();
        let dir = TempDir::new("dl-patch-bad");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let new_content = b"full content";
        let new_md5 = crate::update::md5_bytes(new_content);
        dir.write("install/data/a.bin", b"old");

        // 假 hpatchz 输出错误内容：校验失败后仍然回退到整包
        let script = write_script(dir.path(), "hpatchz", "#!/bin/sh\nprintf broken > \"$4\"\n");
        env.set("NTE_HPATCHZ", &script);

        let patch_bytes = b"diff";
        let patch_md5 = crate::update::md5_bytes(patch_bytes);
        server.route(res_path(&patch_md5, patch_bytes.len() as u64), patch_bytes);
        server.route(res_path(&new_md5, new_content.len() as u64), new_content);
        let mut item = res_item("data/a.bin", &new_md5, new_content.len() as u64);
        item.reason = UpdateReason::Md5;
        item.patch = Some(PatchRef {
            md5: patch_md5,
            size: patch_bytes.len() as u64,
            new_md5: new_md5.clone(),
            new_size: new_content.len() as u64,
        });

        let report = apply_plan(&config, &plan_of(vec![item]), &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), new_content);
    }
}
