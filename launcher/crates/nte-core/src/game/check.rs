use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::game::config::GameUpdateConfig;
use crate::game::manifest::{
    parse_diff_list, parse_remote_config, parse_res_list, DiffManifest, GameManifest,
    LocalManifest, PakEntry, RemoteConfig, ResEntry,
};
use crate::game::pxml;
use crate::update::{md5_bytes, md5_file};
use crate::{Error, Result, USER_AGENT};

const SMALL_HASH_LIMIT: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default)]
pub struct CheckOptions {
    pub full: bool,
    pub include_optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateReason {
    Missing,
    Size,
    Md5,
    Rule,
    Unverified,
}

impl UpdateReason {
    pub fn as_str(self) -> &'static str {
        match self {
            UpdateReason::Missing => "missing",
            UpdateReason::Size => "size",
            UpdateReason::Md5 => "md5",
            UpdateReason::Rule => "rule",
            UpdateReason::Unverified => "unverified",
        }
    }
}

#[derive(Debug, Clone)]
pub enum UpdateKind {
    Res {
        res: ResEntry,
    },
    PakEntry {
        pak_md5: String,
        pak_size: u64,
        entry: PakEntry,
    },
}

impl UpdateKind {
    pub fn path(&self) -> &str {
        match self {
            UpdateKind::Res { res } => &res.filename,
            UpdateKind::PakEntry { entry, .. } => &entry.name,
        }
    }
    pub fn size(&self) -> u64 {
        match self {
            UpdateKind::Res { res } => res.filesize,
            UpdateKind::PakEntry { entry, .. } => entry.size,
        }
    }
    pub fn md5(&self) -> &str {
        match self {
            UpdateKind::Res { res } => &res.md5,
            UpdateKind::PakEntry { entry, .. } => &entry.md5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PatchRef {
    pub md5: String,
    pub size: u64,
    pub new_md5: String,
    pub new_size: u64,
}

#[derive(Debug, Clone)]
pub struct UpdateItem {
    pub path: String,
    pub size: u64,
    pub md5: String,
    pub reason: UpdateReason,
    pub kind: UpdateKind,
    pub patch: Option<PatchRef>,
    pub download_size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct UpdatePlan {
    pub local_version: String,
    pub remote_version: String,
    pub remote_hash: String,
    pub remote_tag: String,
    pub checked: usize,
    pub items: Vec<UpdateItem>,
    pub unverified: Vec<UpdateItem>,
}

impl UpdatePlan {
    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|i| i.download_size).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct RemoteBundle {
    pub config: RemoteConfig,
    pub manifest: GameManifest,
    pub diff: Option<DiffManifest>,
}

#[derive(Debug, Clone, Default)]
struct LocalInfo {
    md5: String,
    size: u64,
    timestamp: Option<u64>,
}

pub fn load_remote(config: &GameUpdateConfig) -> Result<RemoteBundle> {
    let remote = fetch_config(config)?;
    let url_path = format!(
        "/{}/Version/Windows/version/{}/ResList.bin.zip",
        config.branch, remote.res_version
    );
    let mut last_error = None;
    let mut manifests = None;
    for cdn in &config.cdns {
        let url = format!("{}{}", cdn.trim_end_matches('/'), url_path);
        match fetch_bytes(&url, 8 * 1024 * 1024)
            .and_then(|zip| pxml::read_zip_manifests(&zip, &config.app_id))
        {
            Ok(m) => {
                manifests = Some(m);
                break;
            }
            Err(e) => last_error = Some(e),
        }
    }
    let manifests = manifests.ok_or_else(|| {
        last_error.unwrap_or_else(|| Error::Http("no CDN returned ResList.bin.zip".into()))
    })?;
    if !remote.list_hash.is_empty() {
        let got = md5_bytes(manifests.res_list.as_bytes());
        if !got.eq_ignore_ascii_case(&remote.list_hash) {
            return Err(Error::Integrity(format!(
                "ResList hash mismatch: {got} != {}",
                remote.list_hash
            )));
        }
    }
    if let Some(diff) = manifests.diff_list.as_ref() {
        if !remote.diff_hash.is_empty() {
            let got = md5_bytes(diff.as_bytes());
            if !got.eq_ignore_ascii_case(&remote.diff_hash) {
                return Err(Error::Integrity(format!(
                    "lastdiff hash mismatch: {got} != {}",
                    remote.diff_hash
                )));
            }
        }
    }
    let manifest = parse_res_list(&manifests.res_list)?;
    if !manifest.version.is_empty() && manifest.version != remote.res_version {
        return Err(Error::Parse(format!(
            "ResList version {} != config ResVersion {}",
            manifest.version, remote.res_version
        )));
    }
    let diff = match manifests.diff_list.as_ref() {
        Some(text) => Some(parse_diff_list(text)?),
        None => None,
    };
    Ok(RemoteBundle {
        config: remote,
        manifest,
        diff,
    })
}

pub fn check(config: &GameUpdateConfig, opts: CheckOptions) -> Result<UpdatePlan> {
    let bundle = load_remote(config)?;
    check_bundle(config, &bundle, opts)
}

pub fn check_bundle(
    config: &GameUpdateConfig,
    bundle: &RemoteBundle,
    opts: CheckOptions,
) -> Result<UpdatePlan> {
    let local = load_local_ledger(config)?;
    let local_version = local
        .as_ref()
        .map(|m| m.version.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| config.local_version())
        .unwrap_or_default();
    let map = local_map(local.as_ref());

    let mut plan = UpdatePlan {
        local_version,
        remote_version: bundle.config.res_version.clone(),
        remote_hash: bundle.config.hash.clone(),
        remote_tag: if bundle.manifest.tag.is_empty() {
            "baseTag".to_string()
        } else {
            bundle.manifest.tag.clone()
        },
        ..Default::default()
    };

    let mut entries: Vec<(UpdateKind, u64)> = Vec::new();
    for r in &bundle.manifest.res {
        entries.push((UpdateKind::Res { res: r.clone() }, r.filesize));
    }
    for pkg in &bundle.manifest.packages {
        for pak in &pkg.paks {
            for e in &pak.entries {
                entries.push((
                    UpdateKind::PakEntry {
                        pak_md5: pak.md5.clone(),
                        pak_size: pak.filesize,
                        entry: e.clone(),
                    },
                    e.size,
                ));
            }
        }
    }
    if opts.include_optional {
        for base in &bundle.manifest.base_versions {
            for r in &base.res {
                entries.push((UpdateKind::Res { res: r.clone() }, r.filesize));
            }
            for pkg in &base.packages {
                for pak in &pkg.paks {
                    for e in &pak.entries {
                        entries.push((
                            UpdateKind::PakEntry {
                                pak_md5: pak.md5.clone(),
                                pak_size: pak.filesize,
                                entry: e.clone(),
                            },
                            e.size,
                        ));
                    }
                }
            }
        }
    }

    for (kind, _size) in entries {
        plan.checked += 1;
        let path = kind.path().to_string();
        let size = kind.size();
        let md5 = kind.md5().to_ascii_lowercase();
        if path.is_empty() {
            continue;
        }
        let local_path = join_install_path(&config.install_root, &path);
        let meta = match fs::metadata(&local_path) {
            Ok(m) => m,
            Err(_) => {
                plan.items.push(UpdateItem {
                    path,
                    size,
                    md5,
                    reason: UpdateReason::Missing,
                    kind,
                    patch: None,
                    download_size: size,
                });
                continue;
            }
        };
        if meta.len() != size {
            plan.items.push(UpdateItem {
                path,
                size,
                md5,
                reason: UpdateReason::Size,
                kind,
                patch: None,
                download_size: size,
            });
            continue;
        }

        let info = map.get(&normalize_path(&path));
        let forced = config.force_hash(&path, size);
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let metadata_changed = info
            .and_then(|i| i.timestamp)
            .map(|t| Some(t) != mtime)
            .unwrap_or(false);

        if let Some(info) = info {
            if info.size != size {
                plan.items.push(UpdateItem {
                    path,
                    size,
                    md5,
                    reason: UpdateReason::Size,
                    kind,
                    patch: None,
                    download_size: size,
                });
                continue;
            }
            if info.md5 == md5
                && !forced
                && !opts.full
                && !(metadata_changed && size <= SMALL_HASH_LIMIT)
            {
                // Fast path: the local ledger and file size agree. The official
                // updater also trusts this unless a rule says otherwise.
                continue;
            }
            if info.md5 != md5 && size > SMALL_HASH_LIMIT && !forced && !opts.full {
                plan.items.push(UpdateItem {
                    path,
                    size,
                    md5,
                    reason: UpdateReason::Md5,
                    kind,
                    patch: None,
                    download_size: size,
                });
                continue;
            }
            // Hash when the ledger is stale, a rule applies, --full is set,
            // or a small file's mtime changed.
            if forced
                || opts.full
                || info.md5 != md5
                || metadata_changed
                || size <= SMALL_HASH_LIMIT
            {
                match md5_file(&local_path) {
                    Ok(got) if got.eq_ignore_ascii_case(&md5) => continue,
                    Ok(_) => {
                        plan.items.push(UpdateItem {
                            path,
                            size,
                            md5,
                            reason: if forced {
                                UpdateReason::Rule
                            } else {
                                UpdateReason::Md5
                            },
                            kind,
                            patch: None,
                            download_size: size,
                        });
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            continue;
        }

        // No ledger entry. Small files and rule files can be checked cheaply;
        // large unknown files are reported separately instead of silently
        // trusting a size-only match.
        if forced || opts.full || size <= SMALL_HASH_LIMIT {
            match md5_file(&local_path) {
                Ok(got) if got.eq_ignore_ascii_case(&md5) => continue,
                Ok(_) => {
                    plan.items.push(UpdateItem {
                        path,
                        size,
                        md5,
                        reason: if forced {
                            UpdateReason::Rule
                        } else {
                            UpdateReason::Md5
                        },
                        kind,
                        patch: None,
                        download_size: size,
                    });
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        plan.unverified.push(UpdateItem {
            path,
            size,
            md5,
            reason: UpdateReason::Unverified,
            kind,
            patch: None,
            download_size: size,
        });
    }

    attach_patches(&mut plan, config, bundle, &map);
    plan.items.sort_by(|a, b| a.path.cmp(&b.path));
    plan.unverified.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(plan)
}

fn attach_patches(
    plan: &mut UpdatePlan,
    config: &GameUpdateConfig,
    bundle: &RemoteBundle,
    map: &BTreeMap<String, LocalInfo>,
) {
    let Some(diff) = bundle.diff.as_ref() else {
        return;
    };
    for item in &mut plan.items {
        if item.reason == UpdateReason::Missing {
            continue;
        }
        let Some(info) = map.get(&normalize_path(&item.path)) else {
            continue;
        };
        if info.md5.is_empty() || info.size == 0 {
            continue;
        }
        let target = join_install_path(&config.install_root, &item.path);
        if !target.is_file() {
            continue;
        }
        let old_id = format!("{}.{}", info.md5.to_ascii_lowercase(), info.size);
        let new_id = format!("{}.{}", item.md5.to_ascii_lowercase(), item.size);
        let Some(patch) = diff.patches.iter().find(|p| {
            p.old_file.eq_ignore_ascii_case(&old_id)
                && p.new_file.eq_ignore_ascii_case(&new_id)
                && (p.version.is_empty() || p.version.eq_ignore_ascii_case(&bundle.config.hash))
        }) else {
            continue;
        };
        let Some((patch_md5, patch_size)) = split_content_id(&patch.patch) else {
            continue;
        };
        if patch_size == 0 || patch_size >= item.size {
            continue;
        }
        item.patch = Some(PatchRef {
            md5: patch_md5,
            size: patch_size,
            new_md5: item.md5.clone(),
            new_size: item.size,
        });
        item.download_size = patch_size;
    }
}

fn split_content_id(value: &str) -> Option<(String, u64)> {
    let (md5, size) = value.rsplit_once('.')?;
    if md5.len() != 32 || !md5.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some((md5.to_ascii_lowercase(), size.parse().ok()?))
}

pub fn load_local_ledger(config: &GameUpdateConfig) -> Result<Option<LocalManifest>> {
    let path = config.patcher_dir.join("ResList.xml");
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let xml = pxml::decode_to_string(&bytes, &config.app_id)?;
    Ok(Some(parse_res_list(&xml)?))
}

fn local_map(local: Option<&LocalManifest>) -> BTreeMap<String, LocalInfo> {
    let mut map = BTreeMap::new();
    let Some(local) = local else { return map };
    for r in &local.res {
        map.insert(
            normalize_path(&r.filename),
            LocalInfo {
                md5: r.md5.to_ascii_lowercase(),
                size: r.filesize,
                timestamp: r.timestamp,
            },
        );
    }
    for base in &local.base_versions {
        for r in &base.res {
            map.insert(
                normalize_path(&r.filename),
                LocalInfo {
                    md5: r.md5.to_ascii_lowercase(),
                    size: r.filesize,
                    timestamp: r.timestamp,
                },
            );
        }
        for pkg in &base.packages {
            for pak in &pkg.paks {
                for e in &pak.entries {
                    map.insert(
                        normalize_path(&e.name),
                        LocalInfo {
                            md5: e.md5.to_ascii_lowercase(),
                            size: e.size,
                            timestamp: None,
                        },
                    );
                }
            }
        }
    }
    map
}

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_string()
}

pub fn join_install_path(root: &Path, rel: &str) -> std::path::PathBuf {
    let mut out = root.to_path_buf();
    for part in normalize_path(rel).split('/') {
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        out.push(part);
    }
    out
}

fn fetch_config(config: &GameUpdateConfig) -> Result<RemoteConfig> {
    let mut last = None;
    for cdn in &config.cdns {
        let url = format!(
            "{}/{}/Version/Windows/config.xml?tValue={}",
            cdn.trim_end_matches('/'),
            config.branch,
            now_ms()
        );
        match fetch_bytes(&url, 2 * 1024 * 1024) {
            Ok(bytes) => match String::from_utf8(bytes)
                .map_err(|e| Error::Parse(format!("config.xml utf8: {e}")))
                .and_then(|s| parse_remote_config(&s))
            {
                Ok(c) => return Ok(c),
                Err(e) => last = Some(e),
            },
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| Error::Http("no game CDN configured".into())))
}

fn fetch_bytes(url: &str, limit: u64) -> Result<Vec<u8>> {
    let mut resp = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .call()
        .map_err(|e| Error::Http(format!("GET {url}: {e}")))?;
    let mut bytes = Vec::new();
    resp.body_mut()
        .as_reader()
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Http(format!("read {url}: {e}")))?;
    Ok(bytes)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::config::CheckRule;
    use crate::game::manifest::{BaseVersion, Package, Pak, PatchEntry};
    use crate::testing::{HttpServer, TempDir};
    use std::io::Write as _;
    use std::path::PathBuf;

    const MB: u64 = 1024 * 1024;
    const RES_LIST: &str = "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/a.bin\" md5=\"00\" filesize=\"5\"/></ResList>";

    fn config_for(dir: &TempDir) -> GameUpdateConfig {
        GameUpdateConfig {
            launcher_root: dir.join("install/NTELauncher"),
            install_root: dir.join("install"),
            config_dir: dir.join("install/NTELauncher/ResFilesM/1289/PatcherConfig"),
            patcher_dir: dir.join("install/NTELauncher/UserData/Patcher/PatcherSDK"),
            branch: "publish_ob".to_string(),
            update_branch: "publish_Updata".to_string(),
            app_id: "1289".to_string(),
            cdns: vec!["https://cdn.test/clientRes".to_string()],
            rules: Vec::new(),
            check_dirs: Vec::new(),
            max_threads: 5,
        }
    }

    fn res(path: &str, md5: &str, size: u64) -> ResEntry {
        ResEntry {
            filename: path.to_string(),
            filesize: size,
            md5: md5.to_string(),
            timestamp: None,
            check: false,
            blocks: Vec::new(),
        }
    }

    fn manifest_res(entries: Vec<ResEntry>) -> GameManifest {
        GameManifest {
            version: "1.4.16".to_string(),
            tag: "baseTag".to_string(),
            res: entries,
            ..Default::default()
        }
    }

    fn bundle(manifest: GameManifest, diff: Option<DiffManifest>) -> RemoteBundle {
        RemoteBundle {
            config: RemoteConfig {
                res_version: "1.4.16".to_string(),
                hash: "HASH".to_string(),
                max_threads: 5,
                ..Default::default()
            },
            manifest,
            diff,
        }
    }

    fn write_ledger(config: &GameUpdateConfig, xml: &str) {
        let encoded = pxml::encode(xml.as_bytes(), &config.app_id).expect("encode ledger");
        fs::create_dir_all(&config.patcher_dir).expect("create patcher dir");
        fs::write(config.patcher_dir.join("ResList.xml"), encoded).expect("write ledger");
    }

    /// 稀疏文件：伪造大文件但不占磁盘。
    fn sparse(path: &Path, size: u64) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let file = fs::File::create(path).expect("create sparse file");
        file.set_len(size).expect("set sparse length");
    }

    fn zip_of(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, contents) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .expect("start zip entry");
            writer.write_all(contents).expect("write zip entry");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    fn config_xml(res_list: &str, diff: Option<&str>) -> String {
        let mut xml = format!(
            "<Config><ResVersion>1.4.16</ResVersion><Hash>HASH</Hash><Tag>baseTag</Tag><listHash>{}</listHash>",
            crate::update::md5_bytes(res_list.as_bytes())
        );
        if let Some(diff) = diff {
            xml.push_str(&format!(
                "<diffHash>{}</diffHash>",
                crate::update::md5_bytes(diff.as_bytes())
            ));
        }
        xml.push_str("</Config>");
        xml
    }

    fn publish(server: &HttpServer, prefix: &str, res_list: &str, diff: Option<&str>) {
        server.route(
            format!("{prefix}/publish_ob/Version/Windows/config.xml"),
            config_xml(res_list, diff),
        );
        let mut entries = vec![(
            "ResList.bin",
            pxml::encode(res_list.as_bytes(), "1289").expect("encode res list"),
        )];
        if let Some(diff) = diff {
            entries.push((
                "LastDiff.bin",
                pxml::encode(diff.as_bytes(), "1289").expect("encode diff"),
            ));
        }
        server.route(
            format!("{prefix}/publish_ob/Version/Windows/version/1.4.16/ResList.bin.zip"),
            zip_of(&entries),
        );
    }

    #[test]
    fn reports_missing_and_resized_files() {
        let dir = TempDir::new("check-missing");
        let config = config_for(&dir);
        dir.write("install/data/present.bin", b"hello");
        dir.write("install/data/short.bin", b"hi");
        let plan = check_bundle(
            &config,
            &bundle(
                manifest_res(vec![
                    res("data/present.bin", &crate::update::md5_bytes(b"hello"), 5),
                    res("data/short.bin", "aa", 99),
                    res("data/gone.bin", "bb", 7),
                ]),
                None,
            ),
            CheckOptions::default(),
        )
        .expect("check");

        assert_eq!(plan.checked, 3);
        assert_eq!(plan.local_version, "");
        assert_eq!(plan.remote_version, "1.4.16");
        assert_eq!(plan.remote_hash, "HASH");
        assert_eq!(plan.remote_tag, "baseTag");
        assert!(!plan.is_empty());
        let reported: Vec<(&str, UpdateReason)> = plan
            .items
            .iter()
            .map(|item| (item.path.as_str(), item.reason))
            .collect();
        assert_eq!(
            reported,
            vec![
                ("data/gone.bin", UpdateReason::Missing),
                ("data/short.bin", UpdateReason::Size),
            ]
        );
        assert_eq!(plan.total_bytes(), 106);
        assert_eq!(plan.items[0].kind.path(), "data/gone.bin");
        assert_eq!(plan.items[0].kind.size(), 7);
        assert_eq!(plan.items[0].kind.md5(), "bb");
        assert!(plan.items[0].patch.is_none());
    }

    #[test]
    fn trusts_the_ledger_for_large_unchanged_files() {
        let dir = TempDir::new("check-ledger");
        let config = config_for(&dir);
        let recorded = "cc".repeat(16);
        sparse(&dir.join("install/data/big.pak"), 21 * MB);
        write_ledger(
            &config,
            &format!(
                "<ResList version=\"1.4.0\" tag=\"old\"><Res filename=\"data/big.pak\" md5=\"{recorded}\" filesize=\"{}\"/></ResList>",
                21 * MB
            ),
        );

        let plan = check_bundle(
            &config,
            &bundle(
                manifest_res(vec![res("data/big.pak", &recorded, 21 * MB)]),
                None,
            ),
            CheckOptions::default(),
        )
        .expect("check");
        assert!(plan.items.is_empty());
        assert!(plan.unverified.is_empty());
        assert!(plan.is_empty());
        assert_eq!(plan.local_version, "1.4.0");

        // 远端 md5 变了：大文件不做哈希，直接按 md5 上报
        let changed = "dd".repeat(16);
        let plan = check_bundle(
            &config,
            &bundle(
                manifest_res(vec![res("data/big.pak", &changed, 21 * MB)]),
                None,
            ),
            CheckOptions::default(),
        )
        .expect("check");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].reason, UpdateReason::Md5);
        assert_eq!(plan.items[0].reason.as_str(), "md5");

        // --full 会真正哈希，稀疏文件的内容同样对不上
        let plan = check_bundle(
            &config,
            &bundle(
                manifest_res(vec![res("data/big.pak", &changed, 21 * MB)]),
                None,
            ),
            CheckOptions {
                full: true,
                include_optional: false,
            },
        )
        .expect("check");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].reason, UpdateReason::Md5);
    }

    #[test]
    fn a_changed_mtime_rehashes_small_files() {
        let dir = TempDir::new("check-mtime");
        let config = config_for(&dir);
        let path = dir.write("install/data/a.bin", b"hello");
        let recorded = crate::update::md5_bytes(b"world");
        write_ledger(
            &config,
            &format!(
                "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/a.bin\" md5=\"{recorded}\" filesize=\"5\" timestamp=\"1\"/></ResList>"
            ),
        );

        // 时间戳与文件不符：小文件重新哈希，内容不符即上报
        let plan = check_bundle(
            &config,
            &bundle(manifest_res(vec![res("data/a.bin", &recorded, 5)]), None),
            CheckOptions::default(),
        )
        .expect("check");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].reason, UpdateReason::Md5);

        // 时间戳一致且 md5 一致：走快速路径，完全不读文件
        let mtime = fs::metadata(&path)
            .expect("stat")
            .modified()
            .expect("mtime")
            .duration_since(UNIX_EPOCH)
            .expect("after epoch")
            .as_secs();
        write_ledger(
            &config,
            &format!(
                "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/a.bin\" md5=\"{recorded}\" filesize=\"5\" timestamp=\"{mtime}\"/></ResList>"
            ),
        );
        let plan = check_bundle(
            &config,
            &bundle(manifest_res(vec![res("data/a.bin", &recorded, 5)]), None),
            CheckOptions::default(),
        )
        .expect("check");
        assert!(plan.items.is_empty());
    }

    #[test]
    fn rule_files_are_hashed_even_when_the_ledger_agrees() {
        let dir = TempDir::new("check-rule");
        let mut config = config_for(&dir);
        config.rules = vec![CheckRule {
            extension: ".sig".into(),
            max_size: 5 * MB,
        }];
        dir.write("install/data/a.sig", b"actual");
        let good = crate::update::md5_bytes(b"actual");
        write_ledger(
            &config,
            &format!(
                "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/a.sig\" md5=\"{good}\" filesize=\"6\"/></ResList>"
            ),
        );

        let plan = check_bundle(
            &config,
            &bundle(manifest_res(vec![res("data/a.sig", &good, 6)]), None),
            CheckOptions::default(),
        )
        .expect("check");
        assert!(plan.items.is_empty(), "{:?}", plan.items);

        let plan = check_bundle(
            &config,
            &bundle(
                manifest_res(vec![res(
                    "data/a.sig",
                    &crate::update::md5_bytes(b"other!"),
                    6,
                )]),
                None,
            ),
            CheckOptions::default(),
        )
        .expect("check");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].reason, UpdateReason::Rule);
        assert_eq!(plan.items[0].reason.as_str(), "rule");
    }

    #[test]
    fn large_unknown_files_are_reported_as_unverified() {
        let dir = TempDir::new("check-unverified");
        let config = config_for(&dir);
        sparse(&dir.join("install/data/big.pak"), 21 * MB);
        let wrong = "ee".repeat(16);

        let plan = check_bundle(
            &config,
            &bundle(
                manifest_res(vec![res("data/big.pak", &wrong, 21 * MB)]),
                None,
            ),
            CheckOptions::default(),
        )
        .expect("check");
        assert!(plan.items.is_empty());
        assert_eq!(plan.unverified.len(), 1);
        assert_eq!(plan.unverified[0].reason, UpdateReason::Unverified);
        assert_eq!(plan.unverified[0].reason.as_str(), "unverified");

        let plan = check_bundle(
            &config,
            &bundle(
                manifest_res(vec![res("data/big.pak", &wrong, 21 * MB)]),
                None,
            ),
            CheckOptions {
                full: true,
                include_optional: false,
            },
        )
        .expect("check");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].reason, UpdateReason::Md5);
        assert!(plan.unverified.is_empty());
    }

    #[test]
    fn walks_packages_and_optional_base_versions() {
        let dir = TempDir::new("check-packages");
        let config = config_for(&dir);
        dir.write("install/Client/Content/Paks/pakchunk0.pak", b"xx");
        dir.write("install/data/optional.bin", b"y");

        let mut manifest = manifest_res(vec![res("", "aa", 0)]);
        manifest.packages.push(Package {
            paks: vec![Pak {
                md5: "aa".repeat(16),
                filesize: 8,
                entries: vec![PakEntry {
                    name: "Client/Content/Paks/pakchunk0.pak".into(),
                    start: 0,
                    offset: 0,
                    size: 2,
                    md5: crate::update::md5_bytes(b"xx"),
                    check: true,
                }],
            }],
        });
        manifest.base_versions.push(BaseVersion {
            version: "1.0".into(),
            tag: "old".into(),
            res: vec![res("data/optional.bin", &crate::update::md5_bytes(b"y"), 1)],
            packages: Vec::new(),
        });

        // 空路径计入 checked 但不产生条目；可选包默认不检查
        let quick = check_bundle(
            &config,
            &bundle(manifest.clone(), None),
            CheckOptions::default(),
        )
        .expect("check");
        assert_eq!(quick.checked, 2);
        assert!(quick.items.is_empty(), "{:?}", quick.items);

        let with_optional = check_bundle(
            &config,
            &bundle(manifest, None),
            CheckOptions {
                full: false,
                include_optional: true,
            },
        )
        .expect("check");
        assert_eq!(with_optional.checked, 3);

        // 分包条目内容不符时带上 pak 自身的信息
        let mut broken = manifest_res(Vec::new());
        broken.packages.push(Package {
            paks: vec![Pak {
                md5: "aa".repeat(16),
                filesize: 8,
                entries: vec![PakEntry {
                    name: "Client/Content/Paks/pakchunk0.pak".into(),
                    start: 0,
                    offset: 0,
                    size: 2,
                    md5: "bb".repeat(16),
                    check: false,
                }],
            }],
        });
        let plan =
            check_bundle(&config, &bundle(broken, None), CheckOptions::default()).expect("check");
        assert_eq!(plan.items.len(), 1);
        match &plan.items[0].kind {
            UpdateKind::PakEntry {
                pak_md5,
                pak_size,
                entry,
            } => {
                assert_eq!(pak_md5, &"aa".repeat(16));
                assert_eq!(*pak_size, 8);
                assert_eq!(entry.name, "Client/Content/Paks/pakchunk0.pak");
            }
            other => panic!("unexpected kind: {other:?}"),
        }
        assert_eq!(
            plan.items[0].kind.path(),
            "Client/Content/Paks/pakchunk0.pak"
        );
        assert_eq!(plan.items[0].kind.size(), 2);
        assert_eq!(plan.items[0].kind.md5(), "bb".repeat(16));
    }

    #[test]
    fn attaches_hdiff_patches_only_when_content_ids_line_up() {
        let dir = TempDir::new("check-patch");
        let config = config_for(&dir);
        let old_content = vec![b'o'; 100];
        let new_content = vec![b'n'; 200];
        dir.write("install/data/a.bin", &old_content);
        let old_md5 = crate::update::md5_bytes(&old_content);
        let new_md5 = crate::update::md5_bytes(&new_content);
        write_ledger(
            &config,
            &format!(
                "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/a.bin\" md5=\"{old_md5}\" filesize=\"100\"/></ResList>"
            ),
        );
        let remote = manifest_res(vec![res("data/a.bin", &new_md5, 200)]);
        let patch_md5 = "ab".repeat(16);

        let diff_with = |patch: String, version: &str| {
            let mut diff = DiffManifest {
                sections: vec!["1.4.16".into()],
                patches: Vec::new(),
            };
            diff.patches.push(PatchEntry {
                old_file: format!("{old_md5}.100"),
                new_file: format!("{new_md5}.200"),
                patch,
                version: version.to_string(),
                blocks: Vec::new(),
            });
            diff
        };

        let plan = check_bundle(
            &config,
            &bundle(
                remote.clone(),
                Some(diff_with(format!("{patch_md5}.50"), "HASH")),
            ),
            CheckOptions::default(),
        )
        .expect("check");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].reason, UpdateReason::Size);
        let patch = plan.items[0].patch.as_ref().expect("patch attached");
        assert_eq!(patch.md5, patch_md5);
        assert_eq!(patch.size, 50);
        assert_eq!(patch.new_md5, new_md5);
        assert_eq!(patch.new_size, 200);
        assert_eq!(plan.items[0].download_size, 50);
        assert_eq!(plan.total_bytes(), 50);

        // 补丁版本号与远端 hash 不符则不使用
        let plan = check_bundle(
            &config,
            &bundle(
                remote.clone(),
                Some(diff_with(format!("{patch_md5}.50"), "OTHER")),
            ),
            CheckOptions::default(),
        )
        .expect("check");
        assert!(plan.items[0].patch.is_none());
        assert_eq!(plan.items[0].download_size, 200);

        // 补丁体积不小于整包时直接下载整包
        let plan = check_bundle(
            &config,
            &bundle(
                remote.clone(),
                Some(diff_with(format!("{patch_md5}.200"), "HASH")),
            ),
            CheckOptions::default(),
        )
        .expect("check");
        assert!(plan.items[0].patch.is_none());

        // 内容标识非法时忽略补丁
        let plan = check_bundle(
            &config,
            &bundle(
                remote.clone(),
                Some(diff_with("not-a-content-id".into(), "HASH")),
            ),
            CheckOptions::default(),
        )
        .expect("check");
        assert!(plan.items[0].patch.is_none());

        // 账本里没有可用的旧 md5 时不上补丁
        write_ledger(
            &config,
            "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/a.bin\" md5=\"\" filesize=\"0\"/></ResList>",
        );
        let plan = check_bundle(
            &config,
            &bundle(remote, Some(diff_with(format!("{patch_md5}.50"), "HASH"))),
            CheckOptions::default(),
        )
        .expect("check");
        assert!(plan.items[0].patch.is_none());

        // 缺失的文件永远不用补丁
        let missing_dir = TempDir::new("check-patch-missing");
        let missing_config = config_for(&missing_dir);
        write_ledger(
            &missing_config,
            &format!(
                "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/a.bin\" md5=\"{old_md5}\" filesize=\"100\"/></ResList>"
            ),
        );
        let plan = check_bundle(
            &missing_config,
            &bundle(
                manifest_res(vec![res("data/a.bin", &new_md5, 200)]),
                Some(diff_with(format!("{patch_md5}.50"), "HASH")),
            ),
            CheckOptions::default(),
        )
        .expect("check");
        assert_eq!(plan.items[0].reason, UpdateReason::Missing);
        assert!(plan.items[0].patch.is_none());
    }

    #[test]
    fn loads_the_local_ledger_and_reports_broken_ones() {
        let dir = TempDir::new("check-ledger-load");
        let config = config_for(&dir);
        assert!(load_local_ledger(&config).expect("ledger").is_none());

        write_ledger(
            &config,
            "<ResList version=\"9.9.9\" tag=\"ledger\"><Res filename=\"data/a.bin\" md5=\"00\" filesize=\"5\"/></ResList>",
        );
        let local = load_local_ledger(&config)
            .expect("ledger")
            .expect("some ledger");
        assert_eq!(local.version, "9.9.9");
        assert_eq!(local.tag, "ledger");

        let broken = TempDir::new("check-ledger-broken");
        let broken_config = config_for(&broken);
        fs::create_dir_all(&broken_config.patcher_dir).expect("create patcher dir");
        fs::write(broken_config.patcher_dir.join("ResList.xml"), b"nope").expect("write");
        assert!(load_local_ledger(&broken_config).is_err());
    }

    #[test]
    fn local_ledger_entries_cover_base_versions_and_pak_entries() {
        let dir = TempDir::new("check-ledger-map");
        let config = config_for(&dir);
        let md5 = "cc".repeat(16);
        sparse(&dir.join("install/data/old.pak"), 21 * MB);
        sparse(&dir.join("install/data/paked.bin"), 21 * MB);
        write_ledger(
            &config,
            &format!(
                "<ResList version=\"1.4.0\" tag=\"old\"><BaseVersion version=\"1.0\" tag=\"t\"><Res filename=\"data/old.pak\" md5=\"{md5}\" filesize=\"{size}\"/><Package><Pak md5=\"{pak}\" filesize=\"9\"><Entry name=\"data/paked.bin\" start=\"0\" offset=\"0\" size=\"{size}\" md5=\"{md5}\"/></Pak></Package></BaseVersion></ResList>",
                size = 21 * MB,
                pak = "dd".repeat(16)
            ),
        );

        let mut manifest = manifest_res(vec![res("data/old.pak", &md5, 21 * MB)]);
        manifest.packages.push(Package {
            paks: vec![Pak {
                md5: "dd".repeat(16),
                filesize: 9,
                entries: vec![PakEntry {
                    name: "data/paked.bin".into(),
                    start: 0,
                    offset: 0,
                    size: 21 * MB,
                    md5: md5.clone(),
                    check: false,
                }],
            }],
        });
        let plan =
            check_bundle(&config, &bundle(manifest, None), CheckOptions::default()).expect("check");
        assert!(plan.items.is_empty(), "{:?}", plan.items);
        assert!(plan.unverified.is_empty());
        assert_eq!(plan.local_version, "1.4.0");
    }

    #[test]
    fn normalizes_and_joins_install_paths() {
        assert_eq!(normalize_path("\\Client\\a.pak"), "Client/a.pak");
        assert_eq!(normalize_path("/Client/a.pak"), "Client/a.pak");
        let root = Path::new("/install");
        assert_eq!(
            join_install_path(root, "\\data\\a.bin"),
            PathBuf::from("/install/data/a.bin")
        );
        // 相对路径里的 .. 和 . 会被丢掉，不会跳出安装目录
        assert_eq!(
            join_install_path(root, "../escape/./a"),
            PathBuf::from("/install/escape/a")
        );
        assert_eq!(join_install_path(root, ""), PathBuf::from("/install"));
    }

    #[test]
    fn splits_content_ids() {
        let md5 = "ab".repeat(16);
        assert_eq!(
            split_content_id(&format!("{md5}.42")),
            Some((md5.clone(), 42))
        );
        assert_eq!(
            split_content_id(&format!("{}.42", md5.to_uppercase())),
            Some((md5, 42))
        );
        assert_eq!(split_content_id("short.42"), None);
        assert_eq!(split_content_id(&format!("{}.x", "zz".repeat(16))), None);
        assert_eq!(split_content_id("nonsense"), None);
    }

    #[test]
    fn check_fetches_config_and_manifests_from_the_cdn() {
        let dir = TempDir::new("check-cdn");
        let mut config = config_for(&dir);
        let server = HttpServer::start();
        config.cdns = vec![server.url("/cdn")];
        dir.write("install/data/a.bin", b"hello");
        let diff =
            "<LastDiff><Patch oldfile=\"aa.10\" newfile=\"bb.20\" patch=\"cc.5\"/></LastDiff>";
        publish(&server, "/cdn", RES_LIST, Some(diff));

        let plan = check(&config, CheckOptions::default()).expect("check");
        assert_eq!(plan.checked, 1);
        assert_eq!(plan.remote_version, "1.4.16");
        assert_eq!(plan.remote_hash, "HASH");
        assert_eq!(plan.remote_tag, "baseTag");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].path, "data/a.bin");
        assert!(
            server
                .targets()
                .iter()
                .any(|target| target
                    .starts_with("/cdn/publish_ob/Version/Windows/config.xml?tValue=")),
            "{:?}",
            server.targets()
        );
    }

    #[test]
    fn check_rejects_hash_mismatches_and_version_skew() {
        let dir = TempDir::new("check-hashes");
        let mut config = config_for(&dir);
        let server = HttpServer::start();
        let res_hash = crate::update::md5_bytes(RES_LIST.as_bytes());

        let bad_list = config_xml(RES_LIST, None).replace(&res_hash, &"11".repeat(16));
        server.route("/cdn1/publish_ob/Version/Windows/config.xml", bad_list);
        server.route(
            "/cdn1/publish_ob/Version/Windows/version/1.4.16/ResList.bin.zip",
            zip_of(&[(
                "ResList.bin",
                pxml::encode(RES_LIST.as_bytes(), "1289").expect("encode"),
            )]),
        );
        config.cdns = vec![server.url("/cdn1")];
        let error = check(&config, CheckOptions::default()).expect_err("list hash");
        assert!(
            error.to_string().contains("ResList hash mismatch"),
            "{error}"
        );

        let diff = "<LastDiff/>";
        let bad_diff = config_xml(RES_LIST, Some(diff))
            .replace(&crate::update::md5_bytes(diff.as_bytes()), &"22".repeat(16));
        server.route("/cdn2/publish_ob/Version/Windows/config.xml", bad_diff);
        server.route(
            "/cdn2/publish_ob/Version/Windows/version/1.4.16/ResList.bin.zip",
            zip_of(&[
                (
                    "ResList.bin",
                    pxml::encode(RES_LIST.as_bytes(), "1289").expect("encode"),
                ),
                ("LastDiff.bin", diff.as_bytes().to_vec()),
            ]),
        );
        config.cdns = vec![server.url("/cdn2")];
        let error = check(&config, CheckOptions::default()).expect_err("diff hash");
        assert!(
            error.to_string().contains("lastdiff hash mismatch"),
            "{error}"
        );

        let older = "<ResList version=\"1.4.15\" tag=\"baseTag\"><Res filename=\"data/a.bin\" md5=\"00\" filesize=\"5\"/></ResList>";
        publish(&server, "/cdn3", older, None);
        config.cdns = vec![server.url("/cdn3")];
        let error = check(&config, CheckOptions::default()).expect_err("version skew");
        assert!(
            error.to_string().contains("!= config ResVersion"),
            "{error}"
        );
    }

    #[test]
    fn check_falls_back_to_the_next_cdn_and_reports_total_failure() {
        let dir = TempDir::new("check-cdn-fallback");
        let mut config = config_for(&dir);
        let server = HttpServer::start();
        server.route(
            "/bad/publish_ob/Version/Windows/config.xml",
            b"\xff\xfe not utf8",
        );
        publish(&server, "/good", RES_LIST, None);

        config.cdns = vec![server.url("/bad"), server.url("/good")];
        let plan = check(&config, CheckOptions::default()).expect("check");
        assert_eq!(plan.remote_version, "1.4.16");
        assert_eq!(server.hits("/bad/publish_ob/Version/Windows/config.xml"), 1);

        // 所有 CDN 都取不到 config.xml
        let mut unreachable = config_for(&dir);
        unreachable.cdns = vec![server.url("/nope")];
        let error = check(&unreachable, CheckOptions::default()).expect_err("404");
        assert!(error.to_string().contains("config.xml"), "{error}");

        // 完全没有配置 CDN
        let mut empty = config_for(&dir);
        empty.cdns = Vec::new();
        let error = check(&empty, CheckOptions::default()).expect_err("no cdn");
        assert!(
            error.to_string().contains("no game CDN configured"),
            "{error}"
        );

        // config.xml 可用但 ResList.bin.zip 在所有 CDN 上都缺失
        let mut no_zip = config_for(&dir);
        no_zip.cdns = vec![server.url("/cfgonly")];
        server.route(
            "/cfgonly/publish_ob/Version/Windows/config.xml",
            config_xml(RES_LIST, None),
        );
        let error = check(&no_zip, CheckOptions::default()).expect_err("no zip");
        assert!(error.to_string().contains("ResList.bin.zip"), "{error}");
    }

    #[test]
    fn check_uses_base_tag_when_the_manifest_has_none() {
        let dir = TempDir::new("check-basetag");
        let config = config_for(&dir);
        let mut manifest = manifest_res(Vec::new());
        manifest.tag = String::new();
        let plan =
            check_bundle(&config, &bundle(manifest, None), CheckOptions::default()).expect("check");
        assert_eq!(plan.remote_tag, "baseTag");
        assert_eq!(plan.checked, 0);
        assert!(plan.is_empty());
    }
}
