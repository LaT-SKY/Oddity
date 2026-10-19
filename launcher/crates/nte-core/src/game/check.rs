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

#[derive(Debug, Clone, Copy)]
pub struct CheckOptions {
    pub full: bool,
    pub include_optional: bool,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            full: false,
            include_optional: false,
        }
    }
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
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| Error::Http(format!("GET {url}: {e}")))?;
    let mut bytes = Vec::new();
    resp.into_reader()
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
