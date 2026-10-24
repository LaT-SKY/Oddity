use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::Duration;

use md5::Digest as _;

use crate::game::check::{join_install_path, PatchRef, UpdateItem, UpdateKind, UpdatePlan};
use crate::game::config::GameUpdateConfig;
use crate::game::manifest::{Block, PakEntry, ResEntry};
use crate::http::HttpClient;
use crate::update::md5_file;
use crate::{Error, Result};

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
    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyStage {
    Download,
    Patch,
    Verify,
    Install,
}

impl ApplyStage {
    pub fn as_str(self) -> &'static str {
        match self {
            ApplyStage::Download => "download",
            ApplyStage::Patch => "patch",
            ApplyStage::Verify => "verify",
            ApplyStage::Install => "install",
        }
    }
}

const NETWORK_BUFFER: usize = 1024 * 1024;
#[cfg(not(test))]
const RANGE_THRESHOLD: u64 = 128 * 1024 * 1024;
#[cfg(test)]
const RANGE_THRESHOLD: u64 = 1;
#[cfg(not(test))]
const RANGE_CHUNK: u64 = 64 * 1024 * 1024;
#[cfg(test)]
const RANGE_CHUNK: u64 = 64 * 1024;
#[cfg(not(test))]
const RANGE_RETRIES: usize = 2;
#[cfg(test)]
const RANGE_RETRIES: usize = 1;
const SMALL_TARGET_HASH: u64 = 20 * 1024 * 1024;

enum WorkerEvent {
    Started {
        index: usize,
    },
    Bytes {
        index: usize,
        bytes: u64,
    },
    Stage {
        index: usize,
        stage: ApplyStage,
    },
    Finished {
        result: ApplyResult,
        cancelled: bool,
    },
}

struct Progress<'a> {
    tx: &'a Sender<WorkerEvent>,
    index: usize,
    cancelled: &'a (dyn Fn() -> bool + Send + Sync),
    bytes: AtomicU64,
}

impl Progress<'_> {
    fn new<'a>(
        tx: &'a Sender<WorkerEvent>,
        index: usize,
        cancelled: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> Progress<'a> {
        Progress {
            tx,
            index,
            cancelled,
            bytes: AtomicU64::new(0),
        }
    }

    fn add(&self, bytes: u64) {
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        let _ = self.tx.send(WorkerEvent::Bytes {
            index: self.index,
            bytes,
        });
    }

    fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    fn stage(&self, stage: ApplyStage) {
        let _ = self.tx.send(WorkerEvent::Stage {
            index: self.index,
            stage,
        });
    }

    fn cancelled(&self) -> bool {
        (self.cancelled)()
    }
}

enum Prepared {
    AlreadyCorrect,
    File(PathBuf),
    Patches(Vec<PathBuf>),
}

pub fn apply_plan(
    config: &GameUpdateConfig,
    plan: &UpdatePlan,
    opts: &ApplyOptions,
) -> ApplyReport {
    apply_plan_observed(config, plan, opts, &mut |_| {}, &mut |_, _| {}, &|| false)
}

pub fn apply_plan_observed(
    config: &GameUpdateConfig,
    plan: &UpdatePlan,
    opts: &ApplyOptions,
    on_file: &mut dyn FnMut(&UpdateItem),
    on_bytes: &mut dyn FnMut(&str, u64),
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> ApplyReport {
    apply_plan_observed_full(
        config,
        plan,
        opts,
        on_file,
        on_bytes,
        &mut |_, _| {},
        &mut |_| {},
        cancelled,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn apply_plan_observed_with_stage(
    config: &GameUpdateConfig,
    plan: &UpdatePlan,
    opts: &ApplyOptions,
    on_file: &mut dyn FnMut(&UpdateItem),
    on_bytes: &mut dyn FnMut(&str, u64),
    on_stage: &mut dyn FnMut(&str, ApplyStage),
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> ApplyReport {
    apply_plan_observed_full(
        config,
        plan,
        opts,
        on_file,
        on_bytes,
        on_stage,
        &mut |_| {},
        cancelled,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn apply_plan_observed_full(
    config: &GameUpdateConfig,
    plan: &UpdatePlan,
    opts: &ApplyOptions,
    on_file: &mut dyn FnMut(&UpdateItem),
    on_bytes: &mut dyn FnMut(&str, u64),
    on_stage: &mut dyn FnMut(&str, ApplyStage),
    on_result: &mut dyn FnMut(&ApplyResult),
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> ApplyReport {
    let staging = config.patcher_dir.join("tmp").join("nte-native");
    let selected: Vec<&UpdateItem> = plan
        .items
        .iter()
        .filter(|item| match &opts.only {
            Some(filter) => item.path.contains(filter),
            None => true,
        })
        .collect();

    let mut report = ApplyReport::default();
    if selected.is_empty() {
        report.cancelled = cancelled();
        if opts.update_ledger && !opts.dry_run && !report.cancelled && !plan.verified.is_empty() {
            let verified = plan
                .verified
                .iter()
                .map(|item| item.path().to_string())
                .collect::<Vec<_>>();
            if let Err(error) = crate::game::ledger::update_after_apply(
                config,
                plan,
                &verified,
                plan.items.is_empty(),
            ) {
                report.ledger_error = Some(error.to_string());
            }
        }
        return report;
    }

    if opts.dry_run {
        for item in &selected {
            on_file(item);
            report.results.push(ApplyResult {
                path: item.path.clone(),
                bytes: 0,
                ok: true,
                error: None,
                skipped: true,
            });
            report.skipped += 1;
        }
        return report;
    }

    cleanup_stale_parts(&staging, plan);
    let max_threads = if plan.max_threads == 0 {
        config.max_threads
    } else {
        plan.max_threads
    }
    .clamp(1, 8);
    let workers = max_threads.min(selected.len()).max(1);
    let allow_ranges = max_threads > 1;
    let range_threads = if selected.len() == 1 { max_threads } else { 1 };
    let http = HttpClient::new();
    let mut worker_config = config.clone();
    // 多个文件已经用满连接数时，每个大文件只开一条分块连接；只有一个
    // 大文件时才把整组连接都用于它的分块下载。
    worker_config.max_threads = range_threads;
    let worker_config = &worker_config;
    let next = AtomicUsize::new(0);
    let install_lock = Mutex::new(());
    let (tx, rx) = mpsc::channel();
    let mut worker_txs: Vec<Sender<WorkerEvent>> = (0..workers).map(|_| tx.clone()).collect();
    drop(tx);
    let selected = &selected;
    let staging = &staging;
    let install_lock = &install_lock;
    let next = &next;
    let http = &http;
    let mut completed: Vec<String> = Vec::new();
    let mut report = std::thread::scope(|scope| {
        for tx in worker_txs.drain(..) {
            scope.spawn(move || {
                worker_loop(
                    worker_config,
                    plan,
                    opts,
                    selected,
                    staging,
                    http,
                    install_lock,
                    next,
                    &tx,
                    cancelled,
                    allow_ranges,
                );
            });
        }
        let mut report = ApplyReport::default();
        for event in rx {
            match event {
                WorkerEvent::Started { index } => on_file(selected[index]),
                WorkerEvent::Bytes { index, bytes } => {
                    report.downloaded_bytes += bytes;
                    on_bytes(&selected[index].path, bytes);
                }
                WorkerEvent::Stage { index, stage } => {
                    on_stage(&selected[index].path, stage);
                }
                WorkerEvent::Finished {
                    result,
                    cancelled: was_cancelled,
                } => {
                    if was_cancelled {
                        report.cancelled = true;
                    } else if result.ok && !result.skipped {
                        completed.push(result.path.clone());
                        if opts.update_ledger {
                            if let Err(error) = crate::game::ledger::update_after_apply(
                                config, plan, &completed, false,
                            ) {
                                report.ledger_error = Some(error.to_string());
                            }
                        }
                    } else if !result.ok {
                        report.failed += 1;
                    }
                    on_result(&result);
                    report.results.push(result);
                }
            }
        }
        report
    });

    report.results.sort_by(|a, b| a.path.cmp(&b.path));
    report.cancelled |= cancelled();
    completed.extend(plan.verified.iter().map(|item| item.path().to_string()));
    if opts.update_ledger && !completed.is_empty() {
        let bump = !report.cancelled && report.failed == 0;
        match crate::game::ledger::update_after_apply(config, plan, &completed, bump) {
            Ok(()) => report.ledger_error = None,
            Err(error) => report.ledger_error = Some(error.to_string()),
        }
    }
    report
}

#[allow(clippy::too_many_arguments)]
fn worker_loop(
    config: &GameUpdateConfig,
    plan: &UpdatePlan,
    opts: &ApplyOptions,
    selected: &[&UpdateItem],
    staging: &Path,
    http: &HttpClient,
    install_lock: &Mutex<()>,
    next: &AtomicUsize,
    tx: &Sender<WorkerEvent>,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
    allow_ranges: bool,
) {
    loop {
        if cancelled() {
            return;
        }
        let index = next.fetch_add(1, Ordering::Relaxed);
        if index >= selected.len() {
            return;
        }
        let progress = Progress::new(tx, index, cancelled);
        let _ = tx.send(WorkerEvent::Started { index });
        let (result, was_cancelled) = process_item(
            config,
            plan,
            opts,
            selected[index],
            staging,
            http,
            install_lock,
            &progress,
            allow_ranges,
        );
        let _ = tx.send(WorkerEvent::Finished {
            result,
            cancelled: was_cancelled,
        });
        if was_cancelled {
            return;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn process_item(
    config: &GameUpdateConfig,
    _plan: &UpdatePlan,
    opts: &ApplyOptions,
    item: &UpdateItem,
    staging: &Path,
    http: &HttpClient,
    install_lock: &Mutex<()>,
    progress: &Progress<'_>,
    allow_ranges: bool,
) -> (ApplyResult, bool) {
    let target = join_install_path(&config.install_root, &item.path);
    let outcome = run_item(
        config,
        opts,
        item,
        &target,
        staging,
        http,
        install_lock,
        progress,
        allow_ranges,
    );
    let (ok, cancelled, error) = match outcome {
        Ok(()) => (true, false, None),
        Err(Error::Cancelled) => (false, true, None),
        Err(error) => (false, false, Some(error.to_string())),
    };
    (
        ApplyResult {
            path: item.path.clone(),
            bytes: progress.bytes(),
            ok,
            error,
            skipped: false,
        },
        cancelled,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_item(
    config: &GameUpdateConfig,
    opts: &ApplyOptions,
    item: &UpdateItem,
    target: &Path,
    staging: &Path,
    http: &HttpClient,
    install_lock: &Mutex<()>,
    progress: &Progress<'_>,
    allow_ranges: bool,
) -> Result<()> {
    let prepared = prepare_item(
        config,
        opts,
        item,
        target,
        staging,
        http,
        progress,
        allow_ranges,
    )?;
    match prepared {
        Prepared::AlreadyCorrect => Ok(()),
        Prepared::File(temp) => {
            progress.stage(ApplyStage::Install);
            let installed = with_install_lock(install_lock, || {
                if progress.cancelled() {
                    return Err(Error::Cancelled);
                }
                install_temp(&temp, target)
            });
            if installed.is_ok() {
                remove_patch_temps(staging, item);
            }
            installed
        }
        Prepared::Patches(patches) => {
            let applied = with_install_lock(install_lock, || {
                if progress.cancelled() {
                    return Err(Error::Cancelled);
                }
                progress.stage(ApplyStage::Patch);
                let final_path = apply_patch_chain(item, target, staging, &patches, progress)?;
                progress.stage(ApplyStage::Verify);
                if !verify(&final_path, item.size, &item.md5)? {
                    return Err(Error::Integrity(format!(
                        "patched output mismatch for {}",
                        target.display()
                    )));
                }
                progress.stage(ApplyStage::Install);
                install_temp(&final_path, target)
            });
            match applied {
                Ok(()) => {
                    for patch in &patches {
                        fs::remove_file(patch).ok();
                    }
                    Ok(())
                }
                Err(Error::Cancelled) => Err(Error::Cancelled),
                Err(_) => {
                    // 补丁链失败时回退完整文件；已下载的补丁字节仍计入报告。
                    remove_chain_temps(staging, item);
                    let temp = download_full(config, item, staging, http, progress, allow_ranges)?;
                    progress.stage(ApplyStage::Install);
                    let installed = with_install_lock(install_lock, || {
                        if progress.cancelled() {
                            return Err(Error::Cancelled);
                        }
                        install_temp(&temp, target)
                    });
                    if installed.is_ok() {
                        remove_patch_temps(staging, item);
                        remove_chain_temps(staging, item);
                    }
                    installed
                }
            }
        }
    }
}

fn with_install_lock<T>(lock: &Mutex<()>, action: impl FnOnce() -> Result<T>) -> Result<T> {
    let guard = lock
        .lock()
        .map_err(|_| Error::Unsupported("安装锁已损坏".into()))?;
    let result = action();
    drop(guard);
    result
}

#[allow(clippy::too_many_arguments)]
fn prepare_item(
    config: &GameUpdateConfig,
    opts: &ApplyOptions,
    item: &UpdateItem,
    target: &Path,
    staging: &Path,
    http: &HttpClient,
    progress: &Progress<'_>,
    allow_ranges: bool,
) -> Result<Prepared> {
    if !opts.force && quick_target_matches(item, target) {
        return Ok(Prepared::AlreadyCorrect);
    }

    if !item.patches.is_empty() && target.is_file() {
        if let Some(final_temp) = chain_final_temp(staging, item) {
            if verify(&final_temp, item.size, &item.md5).unwrap_or(false) {
                return Ok(Prepared::File(final_temp));
            }
        }
        remove_chain_temps(staging, item);
        let mut paths = Vec::with_capacity(item.patches.len());
        let mut failed = None;
        for patch in &item.patches {
            match download_patch(config, item, patch, staging, http, progress, allow_ranges) {
                Ok(path) => paths.push(path),
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(error) => {
                    failed = Some(error);
                    break;
                }
            }
        }
        if failed.is_none() {
            return Ok(Prepared::Patches(paths));
        }
        // 补丁下载失败时还可以退回整文件；继续走完整下载。
    }

    let bytes = download_full(config, item, staging, http, progress, allow_ranges)?;
    Ok(Prepared::File(bytes))
}

fn quick_target_matches(item: &UpdateItem, target: &Path) -> bool {
    if item.size == 0 || item.size > SMALL_TARGET_HASH {
        return false;
    }
    let Ok(metadata) = fs::metadata(target) else {
        return false;
    };
    metadata.len() == item.size && verify(target, item.size, &item.md5).unwrap_or(false)
}

fn download_full(
    config: &GameUpdateConfig,
    item: &UpdateItem,
    staging: &Path,
    http: &HttpClient,
    progress: &Progress<'_>,
    allow_ranges: bool,
) -> Result<PathBuf> {
    progress.stage(ApplyStage::Download);
    let temp = match &item.kind {
        UpdateKind::Res { res } => {
            download_res(config, res, staging, http, progress, allow_ranges)?
        }
        UpdateKind::PakEntry {
            pak_md5,
            pak_size,
            entry,
        } => download_pak_entry(config, pak_md5, *pak_size, entry, staging, http, progress)?,
    };
    Ok(temp)
}

fn download_patch(
    config: &GameUpdateConfig,
    item: &UpdateItem,
    patch: &PatchRef,
    staging: &Path,
    http: &HttpClient,
    progress: &Progress<'_>,
    allow_ranges: bool,
) -> Result<PathBuf> {
    progress.stage(ApplyStage::Download);
    let url = res_url(config, &patch.md5, patch.size);
    let temp = staging.join("Patch").join(format!(
        "{}.{}.{}.part",
        patch.md5,
        patch.size,
        path_tag(&item.path)
    ));
    download_to_temp(
        config,
        http,
        &url,
        &temp,
        patch.size,
        &patch.md5,
        &[],
        progress,
        allow_ranges,
    )?;
    progress.stage(ApplyStage::Verify);
    Ok(temp)
}

fn download_res(
    config: &GameUpdateConfig,
    res: &ResEntry,
    staging: &Path,
    http: &HttpClient,
    progress: &Progress<'_>,
    allow_ranges: bool,
) -> Result<PathBuf> {
    let url = res_url(config, &res.md5, res.filesize);
    let temp = staging.join("Res").join(format!(
        "{}.{}.{}.part",
        res.md5.to_ascii_lowercase(),
        res.filesize,
        path_tag(&res.filename)
    ));
    download_to_temp(
        config,
        http,
        &url,
        &temp,
        res.filesize,
        &res.md5,
        &res.blocks,
        progress,
        allow_ranges,
    )?;
    progress.stage(ApplyStage::Verify);
    Ok(temp)
}

fn download_pak_entry(
    config: &GameUpdateConfig,
    pak_md5: &str,
    pak_size: u64,
    entry: &PakEntry,
    staging: &Path,
    http: &HttpClient,
    progress: &Progress<'_>,
) -> Result<PathBuf> {
    if entry.md5.len() != 32 || entry.offset.checked_add(entry.size).is_none() {
        return Err(Error::Integrity(format!(
            "bad PatcherPak entry for {}",
            entry.name
        )));
    }
    let url = res_url(config, pak_md5, pak_size);
    let temp = staging.join("Pak").join(format!(
        "{}.{}.{}.part",
        entry.md5,
        entry.size,
        path_tag(&entry.name)
    ));
    download_range_to_temp(
        config,
        http,
        &url,
        &temp,
        entry.offset,
        entry.size,
        &entry.md5,
        progress,
    )?;
    progress.stage(ApplyStage::Verify);
    Ok(temp)
}

fn apply_patch_chain(
    item: &UpdateItem,
    target: &Path,
    staging: &Path,
    patch_paths: &[PathBuf],
    progress: &Progress<'_>,
) -> Result<PathBuf> {
    if patch_paths.len() != item.patches.len() {
        return Err(Error::Integrity("补丁链长度与计划不一致".into()));
    }
    let mut current = target.to_path_buf();
    let mut current_is_temp = false;
    for (step, patch_path) in patch_paths.iter().enumerate() {
        if progress.cancelled() {
            if current_is_temp {
                fs::remove_file(&current).ok();
            }
            return Err(Error::Cancelled);
        }
        let patch = &item.patches[step];
        let current_size = fs::metadata(&current)?.len();
        if current_size != patch.old_size {
            return Err(Error::Integrity(format!(
                "patch step {} expects {} bytes, found {}",
                step, patch.old_size, current_size
            )));
        }
        let out = chain_temp(staging, item, step);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        if out.exists() {
            fs::remove_file(&out)?;
        }
        crate::game::patch::apply_hdiff(&current, patch_path, &out)?;
        if current_is_temp {
            fs::remove_file(&current).ok();
        }
        current = out;
        current_is_temp = true;
    }
    Ok(current)
}

fn chain_temp(staging: &Path, item: &UpdateItem, step: usize) -> PathBuf {
    let patch = &item.patches[step];
    staging.join("Patch").join(format!(
        "{}.{}.{}.{}.tmp",
        path_tag(&item.path),
        step,
        patch.new_md5,
        patch.new_size
    ))
}

fn chain_final_temp(staging: &Path, item: &UpdateItem) -> Option<PathBuf> {
    item.patches
        .len()
        .checked_sub(1)
        .map(|step| chain_temp(staging, item, step))
}

fn remove_chain_temps(staging: &Path, item: &UpdateItem) {
    for step in 0..item.patches.len() {
        fs::remove_file(chain_temp(staging, item, step)).ok();
    }
}

fn patch_temp_path(staging: &Path, item: &UpdateItem, patch: &PatchRef) -> PathBuf {
    staging.join("Patch").join(format!(
        "{}.{}.{}.part",
        patch.md5,
        patch.size,
        path_tag(&item.path)
    ))
}

fn remove_patch_temps(staging: &Path, item: &UpdateItem) {
    for patch in &item.patches {
        fs::remove_file(patch_temp_path(staging, item, patch)).ok();
    }
}

#[allow(clippy::too_many_arguments)]
fn download_to_temp(
    config: &GameUpdateConfig,
    http: &HttpClient,
    url: &str,
    temp: &Path,
    expected_size: u64,
    expected_md5: &str,
    blocks: &[Block],
    progress: &Progress<'_>,
    allow_ranges: bool,
) -> Result<u64> {
    if let Some(parent) = temp.parent() {
        fs::create_dir_all(parent)?;
    }
    if temp.is_file() {
        let length = fs::metadata(temp)?.len();
        if length == expected_size {
            if verify(temp, expected_size, expected_md5)? {
                fs::remove_file(range_index_path(temp)).ok();
                return Ok(0);
            }
            // 分块下载的稀疏文件长度已经等于目标大小，但块索引还在；
            // 保留它让下面按块续传，不能当完整文件删掉。
            if !(allow_ranges && range_index_path(temp).is_file()) {
                fs::remove_file(temp)?;
            }
        } else if length > expected_size {
            fs::remove_file(temp)?;
        }
    }

    if allow_ranges && expected_size >= RANGE_THRESHOLD {
        let index = range_index_path(temp);
        match download_ranges(
            config,
            http,
            url,
            temp,
            &index,
            expected_size,
            expected_md5,
            blocks,
            progress,
        ) {
            Ok(bytes) => return Ok(bytes),
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(Error::Unsupported(_)) | Err(Error::Integrity(_)) => {
                // Range 不可用或分块校验失败时重新走顺序下载，避免留下
                // 预分配的半成品文件。
                fs::remove_file(temp).ok();
                fs::remove_file(&index).ok();
            }
            // 网络/磁盘错误保留块索引，下一次任务可以从已完成块继续。
            Err(error) => return Err(error),
        }
    }

    let mut last_error = None;
    for (i, cdn) in config.cdns.iter().enumerate() {
        let attempt_url = request_url(config, cdn, i, url);
        for attempt in 0..=RANGE_RETRIES {
            if progress.cancelled() {
                return Err(Error::Cancelled);
            }
            let existing = temp.metadata().map(|meta| meta.len()).unwrap_or(0);
            match fetch_to_file(http, &attempt_url, temp, existing, expected_size, progress) {
                Ok(bytes) => match verify(temp, expected_size, expected_md5) {
                    Ok(true) => return Ok(bytes),
                    Ok(false) => {
                        fs::remove_file(temp).ok();
                        last_error = Some(Error::Integrity(format!(
                            "download md5 mismatch for {}",
                            temp.display()
                        )));
                        break;
                    }
                    Err(error) => {
                        last_error = Some(error);
                        break;
                    }
                },
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < RANGE_RETRIES {
                        std::thread::sleep(Duration::from_millis(250));
                    }
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| Error::Http(format!("download failed: {url}"))))
}

#[allow(clippy::too_many_arguments)]
fn download_range_to_temp(
    config: &GameUpdateConfig,
    http: &HttpClient,
    url: &str,
    temp: &Path,
    offset: u64,
    size: u64,
    expected_md5: &str,
    progress: &Progress<'_>,
) -> Result<u64> {
    if let Some(parent) = temp.parent() {
        fs::create_dir_all(parent)?;
    }
    if temp.is_file() {
        let length = fs::metadata(temp)?.len();
        if length == size {
            if verify(temp, size, expected_md5)? {
                return Ok(0);
            }
            fs::remove_file(temp)?;
        } else if length > size {
            fs::remove_file(temp)?;
        }
    }
    let end = offset
        .checked_add(size)
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| Error::Integrity("PatcherPak range overflow".into()))?;
    let mut last_error = None;
    for (i, cdn) in config.cdns.iter().enumerate() {
        let attempt_url = request_url(config, cdn, i, url);
        for attempt in 0..=RANGE_RETRIES {
            if progress.cancelled() {
                return Err(Error::Cancelled);
            }
            let existing = temp.metadata().map(|meta| meta.len()).unwrap_or(0);
            let range = format!("bytes={}-{}", offset + existing, end);
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(temp)?;
            match fetch_range_to_file(
                http,
                &attempt_url,
                &file,
                existing,
                &range,
                size.saturating_sub(existing),
                progress,
            ) {
                Ok(bytes) => match verify(temp, size, expected_md5) {
                    Ok(true) => return Ok(bytes),
                    Ok(false) => {
                        fs::remove_file(temp).ok();
                        last_error = Some(Error::Integrity(format!(
                            "PatcherPak entry md5 mismatch for {}",
                            temp.display()
                        )));
                        break;
                    }
                    Err(error) => {
                        last_error = Some(error);
                        break;
                    }
                },
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(error @ Error::Unsupported(_)) => {
                    last_error = Some(error);
                    break;
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt < RANGE_RETRIES {
                        std::thread::sleep(Duration::from_millis(250));
                    }
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| Error::Http("PatcherPak range download failed".into())))
}

#[allow(clippy::too_many_arguments)]
fn download_ranges(
    config: &GameUpdateConfig,
    http: &HttpClient,
    url: &str,
    temp: &Path,
    index_path: &Path,
    expected_size: u64,
    expected_md5: &str,
    blocks: &[Block],
    progress: &Progress<'_>,
) -> Result<u64> {
    let chunks = range_chunks(expected_size, blocks);
    if chunks.is_empty() {
        return Ok(0);
    }

    let resume = read_range_index(index_path, expected_size, chunks.len());
    let mut done = match resume {
        Some(done) => done,
        None => {
            fs::remove_file(temp).ok();
            fs::remove_file(index_path).ok();
            Vec::new()
        }
    };
    if !temp.is_file() && !done.is_empty() {
        fs::remove_file(index_path).ok();
        return Err(Error::Integrity("块索引对应的临时文件已丢失".into()));
    }

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(temp)?;
    // 临时文件被截断时，旧索引里的块不再可信。
    if file.metadata()?.len() != expected_size {
        if index_path.exists() {
            done.clear();
            fs::remove_file(index_path).ok();
        }
        file.set_len(expected_size)?;
    }
    if !index_path.exists() {
        write_range_index_header(index_path, expected_size, chunks.len())?;
    }

    for chunk in &chunks {
        if done.contains(&chunk.index) {
            continue;
        }
        if progress.cancelled() {
            return Err(Error::Cancelled);
        }
    }

    let total = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let first_error: Mutex<Option<Error>> = Mutex::new(None);
    let done_lock = Mutex::new(done);
    let threads = config.max_threads.clamp(1, 8).min(chunks.len());
    let chunk_list = &chunks;
    let file_ref = &file;
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                if failed.load(Ordering::Relaxed) || progress.cancelled() {
                    return;
                }
                let index = total.fetch_add(1, Ordering::Relaxed);
                if index >= chunk_list.len() {
                    return;
                }
                let chunk = &chunk_list[index];
                if done_lock
                    .lock()
                    .map(|done| done.contains(&chunk.index))
                    .unwrap_or(true)
                {
                    continue;
                }
                match fetch_chunk(config, http, url, file_ref, chunk, progress) {
                    Ok(_) => {
                        if let Some(expected) = &chunk.md5 {
                            if !md5_range(file_ref, chunk.start, chunk.size)
                                .is_ok_and(|got| got.eq_ignore_ascii_case(expected))
                            {
                                failed.store(true, Ordering::Relaxed);
                                set_first_error(
                                    &first_error,
                                    Error::Integrity(format!("chunk {} md5 mismatch", chunk.index)),
                                );
                                return;
                            }
                        }
                        if let Ok(mut done) = done_lock.lock() {
                            done.push(chunk.index);
                        }
                        if let Err(error) = append_range_index(index_path, chunk.index) {
                            failed.store(true, Ordering::Relaxed);
                            set_first_error(&first_error, error);
                            return;
                        }
                    }
                    Err(Error::Cancelled) => {
                        failed.store(true, Ordering::Relaxed);
                        set_first_error(&first_error, Error::Cancelled);
                        return;
                    }
                    Err(error) => {
                        failed.store(true, Ordering::Relaxed);
                        set_first_error(&first_error, error);
                        return;
                    }
                }
            });
        }
    });

    if let Some(error) = first_error.into_inner().unwrap_or(None) {
        return Err(error);
    }
    if progress.cancelled() {
        return Err(Error::Cancelled);
    }
    if !verify(temp, expected_size, expected_md5)? {
        fs::remove_file(temp).ok();
        fs::remove_file(index_path).ok();
        return Err(Error::Integrity(format!(
            "range download md5 mismatch for {}",
            temp.display()
        )));
    }
    fs::remove_file(index_path).ok();
    Ok(0)
}

fn set_first_error(slot: &Mutex<Option<Error>>, error: Error) {
    if let Ok(mut slot) = slot.lock() {
        if slot.is_none() {
            *slot = Some(error);
        }
    }
}

#[derive(Debug)]
struct RangeChunk {
    index: u64,
    start: u64,
    size: u64,
    md5: Option<String>,
}

fn range_chunks(size: u64, blocks: &[Block]) -> Vec<RangeChunk> {
    let usable_blocks = !blocks.is_empty()
        && blocks.iter().all(|block| block.size > 0)
        && blocks.iter().enumerate().all(|(index, block)| {
            index == 0
                || blocks[index - 1].start.checked_add(blocks[index - 1].size) == Some(block.start)
        })
        && blocks.first().is_some_and(|block| block.start == 0)
        && blocks
            .last()
            .is_some_and(|block| block.start.checked_add(block.size) == Some(size));
    if usable_blocks {
        return blocks
            .iter()
            .enumerate()
            .map(|(index, block)| RangeChunk {
                index: index as u64,
                start: block.start,
                size: block.size,
                md5: (!block.md5.is_empty()).then(|| block.md5.to_ascii_lowercase()),
            })
            .collect();
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while start < size {
        let length = (size - start).min(RANGE_CHUNK);
        chunks.push(RangeChunk {
            index,
            start,
            size: length,
            md5: None,
        });
        start += length;
        index += 1;
    }
    chunks
}

fn fetch_chunk(
    config: &GameUpdateConfig,
    http: &HttpClient,
    url: &str,
    file: &File,
    chunk: &RangeChunk,
    progress: &Progress<'_>,
) -> Result<u64> {
    let mut last_error = None;
    for (i, cdn) in config.cdns.iter().enumerate() {
        let attempt_url = request_url(config, cdn, i, url);
        for attempt in 0..=RANGE_RETRIES {
            if progress.cancelled() {
                return Err(Error::Cancelled);
            }
            let end = chunk.start + chunk.size - 1;
            let range = format!("bytes={}-{}", chunk.start, end);
            match fetch_range_to_file(
                http,
                &attempt_url,
                file,
                chunk.start,
                &range,
                chunk.size,
                progress,
            ) {
                Ok(bytes) => return Ok(bytes),
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(error @ Error::Unsupported(_)) => {
                    last_error = Some(error);
                    break;
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt < RANGE_RETRIES {
                        std::thread::sleep(Duration::from_millis(250));
                    }
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| Error::Http("range download failed".into())))
}

fn fetch_to_file(
    http: &HttpClient,
    url: &str,
    temp: &Path,
    existing: u64,
    expected_size: u64,
    progress: &Progress<'_>,
) -> Result<u64> {
    let mut request = http.request(
        url,
        transfer_timeout(expected_size.saturating_sub(existing)),
    );
    if existing > 0 {
        request = request.header("Range", format!("bytes={existing}-"));
    }
    let mut response = match request.call() {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(416)) if existing > 0 => return Ok(0),
        Err(error) => return Err(Error::Http(format!("GET {url}: {error}"))),
    };
    let status = response.status().as_u16();
    let append = status == 206 && existing > 0;
    let mut output = if append {
        OpenOptions::new().append(true).open(temp)?
    } else {
        File::create(temp)?
    };
    let mut reader = response.body_mut().as_reader();
    let mut buffer = vec![0u8; NETWORK_BUFFER];
    let mut total = 0u64;
    loop {
        if progress.cancelled() {
            output.flush()?;
            return Err(Error::Cancelled);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| Error::Http(format!("read {url}: {error}")))?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
        total += read as u64;
        progress.add(read as u64);
    }
    output.flush()?;
    if status != 200 && status != 206 {
        return Err(Error::Http(format!("GET {url}: HTTP {status}")));
    }
    Ok(total)
}

fn fetch_range_to_file(
    http: &HttpClient,
    url: &str,
    file: &File,
    write_offset: u64,
    range: &str,
    range_size: u64,
    progress: &Progress<'_>,
) -> Result<u64> {
    let mut response = http
        .request(url, transfer_timeout(range_size))
        .header("Range", range)
        .call()
        .map_err(|error| Error::Http(format!("GET {url}: {error}")))?;
    let status = response.status().as_u16();
    if status != 206 {
        return Err(Error::Unsupported(format!(
            "server ignored Range for {url}: HTTP {status}"
        )));
    }
    let mut reader = response.body_mut().as_reader();
    let mut buffer = vec![0u8; NETWORK_BUFFER];
    let mut written = 0u64;
    loop {
        if progress.cancelled() {
            return Err(Error::Cancelled);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| Error::Http(format!("read {url}: {error}")))?;
        if read == 0 {
            break;
        }
        write_all_at(file, &buffer[..read], write_offset + written)?;
        written += read as u64;
        progress.add(read as u64);
    }
    Ok(written)
}

fn write_all_at(file: &File, mut bytes: &[u8], mut offset: u64) -> Result<()> {
    use std::os::unix::fs::FileExt;
    while !bytes.is_empty() {
        let written = file.write_at(bytes, offset)?;
        if written == 0 {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "write_at returned 0",
            )));
        }
        bytes = &bytes[written..];
        offset += written as u64;
    }
    Ok(())
}

fn range_index_path(temp: &Path) -> PathBuf {
    temp.with_extension("cb.idx")
}

fn read_range_index(path: &Path, size: u64, chunks: usize) -> Option<Vec<u64>> {
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != "ODDITY-CB1" {
        return None;
    }
    let size_line = lines.next()?;
    if size_line != format!("size={size}") {
        return None;
    }
    let chunks_line = lines.next()?;
    if chunks_line != format!("chunks={chunks}") {
        return None;
    }
    let mut done = Vec::new();
    for line in lines {
        if let Ok(value) = line.trim().parse::<u64>() {
            if value < chunks as u64 && !done.contains(&value) {
                done.push(value);
            }
        }
    }
    Some(done)
}

fn write_range_index_header(path: &Path, size: u64, chunks: usize) -> Result<()> {
    let text = format!("ODDITY-CB1\nsize={size}\nchunks={chunks}\n");
    let temporary = path.with_extension("cb.idx.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn append_range_index(path: &Path, index: u64) -> Result<()> {
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    writeln!(file, "{index}")?;
    file.flush()?;
    Ok(())
}

fn md5_range(file: &File, start: u64, size: u64) -> Result<String> {
    use std::os::unix::fs::FileExt;
    let mut hasher = md5::Md5::new();
    let mut buffer = vec![0u8; NETWORK_BUFFER];
    let mut offset = start;
    let mut remaining = size;
    while remaining > 0 {
        let want = (remaining as usize).min(buffer.len());
        let read = file.read_at(&mut buffer[..want], offset)?;
        if read == 0 {
            return Err(Error::Integrity("块读取提前结束".into()));
        }
        hasher.update(&buffer[..read]);
        offset += read as u64;
        remaining -= read as u64;
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push_str(&format!("{byte:02x}"));
    }
    value
}

fn transfer_timeout(bytes: u64) -> Duration {
    // 以 256 KiB/s 的最慢速度估算，避免大文件在正常慢速链路上被提前中断。
    let seconds = (bytes / (256 * 1024)).clamp(120, 6 * 60 * 60);
    Duration::from_secs(seconds)
}

fn request_url(config: &GameUpdateConfig, cdn: &str, index: usize, original: &str) -> String {
    if index == 0 {
        return original.to_string();
    }
    let suffix = original
        .split_once("/Res/")
        .map(|(_, value)| value)
        .unwrap_or("");
    format!(
        "{}/{}/Res/{}",
        cdn.trim_end_matches('/'),
        config.branch,
        suffix
    )
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

fn verify(path: &Path, expected_size: u64, expected_md5: &str) -> Result<bool> {
    let metadata = fs::metadata(path)?;
    if metadata.len() != expected_size {
        return Ok(false);
    }
    let got = md5_file(path)?;
    Ok(got.eq_ignore_ascii_case(expected_md5))
}

fn install_temp(temp: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let adjacent = adjacent_temp(target);
    if adjacent.exists() {
        fs::remove_file(&adjacent)?;
    }
    match fs::rename(temp, &adjacent) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
            fs::copy(temp, &adjacent)?;
            fs::remove_file(temp)?;
        }
        Err(error) => return Err(Error::Io(error)),
    }
    File::open(&adjacent)?.sync_all()?;
    fs::rename(&adjacent, target)?;
    Ok(())
}

fn adjacent_temp(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(".oddity-new");
    PathBuf::from(name)
}

fn path_tag(path: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in path.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn cleanup_stale_parts(staging: &Path, plan: &UpdatePlan) {
    let mut keep = Vec::new();
    for item in &plan.items {
        keep.push(format!("{}.{}", item.md5.to_ascii_lowercase(), item.size));
        if let UpdateKind::PakEntry { entry, .. } = &item.kind {
            keep.push(format!("{}.{}", entry.md5.to_ascii_lowercase(), entry.size));
        }
        for patch in &item.patches {
            keep.push(format!("{}.{}", patch.md5, patch.size));
        }
    }
    for directory in [
        staging.join("Res"),
        staging.join("Patch"),
        staging.join("Pak"),
    ] {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".part") && !name.ends_with(".cb.idx") {
                continue;
            }
            if keep.iter().any(|prefix| name.starts_with(prefix)) {
                continue;
            }
            fs::remove_file(entry.path()).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::check::{UpdateKind, UpdatePlan, UpdateReason};
    use crate::game::manifest::{PakEntry, ResEntry};
    use crate::testing::{HttpServer, TempDir};

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
            patches: Vec::new(),
            download_size: size,
        }
    }

    fn patch_item(
        path: &str,
        old_md5: &str,
        old_size: u64,
        new_md5: &str,
        new_size: u64,
        patches: &[(&str, u64)],
    ) -> UpdateItem {
        let mut item = res_item(path, new_md5, new_size);
        let mut old_md5 = old_md5.to_string();
        let mut old_size = old_size;
        for (md5, size) in patches {
            item.patches.push(PatchRef {
                md5: md5.to_string(),
                size: *size,
                old_md5: old_md5.clone(),
                old_size,
                new_md5: new_md5.to_string(),
                new_size,
            });
            old_md5 = new_md5.to_string();
            old_size = new_size;
        }
        item.download_size = item.patches.iter().map(|patch| patch.size).sum();
        item
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
            patches: Vec::new(),
            download_size: size,
        }
    }

    fn plan_of(items: Vec<UpdateItem>) -> UpdatePlan {
        UpdatePlan {
            remote_version: "1.4.16".to_string(),
            remote_tag: "baseTag".to_string(),
            remote_hash: "HASH".to_string(),
            items,
            max_threads: 5,
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

    fn res_temp(config: &GameUpdateConfig, path: &str, md5: &str, size: u64) -> PathBuf {
        config.patcher_dir.join("tmp/nte-native/Res").join(format!(
            "{}.{}.{}.part",
            md5,
            size,
            path_tag(path)
        ))
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
    fn cancelling_before_workers_start_is_reported_without_writing_files() {
        let dir = TempDir::new("cancel-before-worker");
        let config = config_for(&dir);
        let plan = plan_of(vec![res_item("data/a.bin", &"ab".repeat(16), 5)]);
        let report = apply_plan_observed(
            &config,
            &plan,
            &ApplyOptions::default(),
            &mut |_| panic!("不能启动已取消的任务"),
            &mut |_, _| {},
            &|| true,
        );
        assert!(report.cancelled);
        assert!(report.results.is_empty());
        assert!(!config.install_root.join("data/a.bin").exists());
        assert!(!config.patcher_dir.join("ResList.xml").exists());
    }

    #[test]
    fn dry_run_reports_without_downloading() {
        let dir = TempDir::new("dl-dry");
        let config = config_for(&dir);
        let plan = plan_of(vec![
            res_item("data/a.bin", &"aa".repeat(16), 3),
            res_item("data/b.bin", &"bb".repeat(16), 4),
        ]);
        let stale = config.patcher_dir.join("tmp/nte-native/Res/stale.part");
        fs::create_dir_all(stale.parent().expect("parent")).expect("staging");
        fs::write(&stale, b"stale").expect("stale part");
        let report = apply_plan(
            &config,
            &plan,
            &ApplyOptions {
                dry_run: true,
                ..Default::default()
            },
        );
        assert!(stale.is_file(), "dry-run 不应清理暂存文件");
        assert_eq!(report.results.len(), 2);
        assert!(report
            .results
            .iter()
            .all(|result| result.ok && result.skipped && result.bytes == 0));
        assert_eq!(report.skipped, 2);
        assert_eq!(report.downloaded_bytes, 0);
        assert_eq!(report.failed, 0);
        assert!(!dir.is_file("install/data/a.bin"));
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

        let part = res_temp(&config, "data/a.bin", &md5, content.len() as u64);
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

        let part = res_temp(&config, "data/a.bin", &md5, content.len() as u64);
        fs::create_dir_all(part.parent().expect("parent")).expect("create staging");
        fs::write(&part, &content[..100]).expect("write partial");

        let mut plan = plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]);
        plan.max_threads = 1;
        let report = apply_plan(&config, &plan, &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
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

        let part = res_temp(&config, "data/a.bin", &md5, content.len() as u64);
        fs::create_dir_all(part.parent().expect("parent")).expect("create staging");
        fs::write(&part, b"wrong content").expect("write broken part");
        let mut plan = plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]);
        plan.max_threads = 1;
        let report = apply_plan(&config, &plan, &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(report.downloaded_bytes, content.len() as u64);
        assert_eq!(dir.read("install/data/a.bin"), content);

        let part = res_temp(&config, "data/b.bin", &md5, content.len() as u64);
        fs::create_dir_all(part.parent().expect("parent")).expect("create staging");
        fs::write(&part, vec![0u8; content.len() + 5]).expect("write oversized part");
        let mut plan = plan_of(vec![res_item("data/b.bin", &md5, content.len() as u64)]);
        plan.max_threads = 1;
        let report = apply_plan(&config, &plan, &ApplyOptions::default());
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
        server.route(res_path(&md5, 100), b"short");

        let part = res_temp(&config, "data/a.bin", &md5, 100);
        fs::create_dir_all(part.parent().expect("parent")).expect("create staging");
        fs::write(&part, vec![0u8; 50]).expect("write partial");

        let mut plan = plan_of(vec![res_item("data/a.bin", &md5, 100)]);
        plan.max_threads = 1;
        let report = apply_plan(&config, &plan, &ApplyOptions::default());
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

        let mut plan = plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]);
        plan.max_threads = 1;
        let report = apply_plan(&config, &plan, &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), content);
        assert!(server.hits(&format!("/bad/{suffix}")) >= 1);
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
        assert!(server.ranges().iter().any(|range| range == "bytes=10-39"));
    }

    #[test]
    fn pak_entries_with_bad_ids_are_rejected_before_downloading() {
        let dir = TempDir::new("dl-pak-bad");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];

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
    fn patches_are_applied_from_hdiff13() {
        let dir = TempDir::new("dl-patch");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let old_content = b"version one: reusable game bytes\n";
        let new_content = b"version two: reusable game bytes!\n";
        let old_md5 = crate::update::md5_bytes(old_content);
        let new_md5 = crate::update::md5_bytes(new_content);
        dir.write("install/data/a.bin", old_content);
        let patch_bytes = include_bytes!("../../tests/fixtures/reusable-game.hdiff");
        let patch_md5 = crate::update::md5_bytes(patch_bytes);
        server.route(res_path(&patch_md5, patch_bytes.len() as u64), patch_bytes);
        let item = patch_item(
            "data/a.bin",
            &old_md5,
            old_content.len() as u64,
            &new_md5,
            new_content.len() as u64,
            &[(&patch_md5, patch_bytes.len() as u64)],
        );

        let report = apply_plan(&config, &plan_of(vec![item]), &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), new_content);
        assert_eq!(report.downloaded_bytes, patch_bytes.len() as u64);
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
        server.route(res_path(&new_md5, new_content.len() as u64), new_content);
        let item = patch_item(
            "data/a.bin",
            &crate::update::md5_bytes(b"old"),
            3,
            &new_md5,
            new_content.len() as u64,
            &[(&"ee".repeat(16), 4)],
        );

        let report = apply_plan(&config, &plan_of(vec![item]), &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), new_content);
    }

    #[test]
    fn an_invalid_patch_is_rejected() {
        let dir = TempDir::new("dl-patch-bad");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let new_content = b"full content";
        let new_md5 = crate::update::md5_bytes(new_content);
        dir.write("install/data/a.bin", b"old");
        let patch_bytes = b"diff";
        let patch_md5 = crate::update::md5_bytes(patch_bytes);
        server.route(res_path(&patch_md5, patch_bytes.len() as u64), patch_bytes);
        server.route(res_path(&new_md5, new_content.len() as u64), new_content);
        let item = patch_item(
            "data/a.bin",
            &crate::update::md5_bytes(b"old"),
            3,
            &new_md5,
            new_content.len() as u64,
            &[(&patch_md5, patch_bytes.len() as u64)],
        );

        let report = apply_plan(&config, &plan_of(vec![item]), &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), new_content);
    }

    #[test]
    fn applies_a_two_step_patch_chain() {
        let dir = TempDir::new("dl-chain");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let old_content = b"alpha beta gamma delta\n".repeat(200);
        let mut mid_content = String::from_utf8(old_content.clone())
            .expect("utf8")
            .replace("gamma", "GAMMA")
            .into_bytes();
        for _ in 0..10 {
            mid_content.extend_from_slice(b"middle addition\n");
        }
        let mut new_content = String::from_utf8(mid_content.clone())
            .expect("utf8")
            .replace("delta", "DELTA")
            .into_bytes();
        for _ in 0..5 {
            new_content.extend_from_slice(b"final addition\n");
        }

        let old_md5 = crate::update::md5_bytes(&old_content);
        let mid_md5 = crate::update::md5_bytes(&mid_content);
        let new_md5 = crate::update::md5_bytes(&new_content);
        dir.write("install/data/a.bin", &old_content);
        let mid_patch = include_bytes!("../../tests/fixtures/chain-mid.hdiff");
        let final_patch = include_bytes!("../../tests/fixtures/chain-final.hdiff");
        let mid_patch_md5 = crate::update::md5_bytes(mid_patch);
        let final_patch_md5 = crate::update::md5_bytes(final_patch);
        server.route(res_path(&mid_patch_md5, mid_patch.len() as u64), mid_patch);
        server.route(
            res_path(&final_patch_md5, final_patch.len() as u64),
            final_patch,
        );
        let item = patch_item(
            "data/a.bin",
            &old_md5,
            old_content.len() as u64,
            &new_md5,
            new_content.len() as u64,
            &[
                (&mid_patch_md5, mid_patch.len() as u64),
                (&final_patch_md5, final_patch.len() as u64),
            ],
        );
        // 第二个补丁的输入是中间内容，不是本地旧文件。
        let mut item = item;
        item.patches[0].new_md5 = mid_md5.clone();
        item.patches[0].new_size = mid_content.len() as u64;
        item.patches[1].old_md5 = mid_md5;
        item.patches[1].old_size = mid_content.len() as u64;

        let report = apply_plan(&config, &plan_of(vec![item]), &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), new_content);
        assert_eq!(
            report.downloaded_bytes,
            (mid_patch.len() + final_patch.len()) as u64
        );
    }

    #[test]
    fn range_download_resumes_from_the_block_index() {
        let dir = TempDir::new("dl-block-index");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let content: Vec<u8> = (0..200_000u32).map(|value| value as u8).collect();
        let md5 = crate::update::md5_bytes(&content);
        server.route(res_path(&md5, content.len() as u64), &content);

        let part = res_temp(&config, "data/a.bin", &md5, content.len() as u64);
        fs::create_dir_all(part.parent().expect("parent")).expect("create staging");
        let file = File::create(&part).expect("create sparse part");
        file.set_len(content.len() as u64).expect("set length");
        use std::os::unix::fs::FileExt;
        file.write_all_at(&content[..RANGE_CHUNK as usize], 0)
            .expect("write first chunk");
        let index = range_index_path(&part);
        write_range_index_header(&index, content.len() as u64, 4).expect("header");
        append_range_index(&index, 0).expect("done chunk");

        let report = apply_plan(
            &config,
            &plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]),
            &ApplyOptions::default(),
        );
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), content);
        assert_eq!(report.downloaded_bytes, content.len() as u64 - RANGE_CHUNK);
        let ranges = server.ranges();
        assert_eq!(ranges.len(), 3, "{ranges:?}");
        assert!(!ranges.iter().any(|range| range.starts_with("bytes=0-")));
        assert!(!index.exists());
    }

    #[test]
    fn range_download_uses_manifest_blocks() {
        let dir = TempDir::new("dl-blocks");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let content: Vec<u8> = (0..200_000u32).map(|value| value as u8).collect();
        let md5 = crate::update::md5_bytes(&content);
        server.route(res_path(&md5, content.len() as u64), &content);
        let mut item = res_item("data/a.bin", &md5, content.len() as u64);
        let UpdateKind::Res { res } = &mut item.kind else {
            unreachable!()
        };
        for index in 0..4u64 {
            let start = index * 50_000;
            res.blocks.push(Block {
                index: index as u32,
                start,
                size: 50_000,
                md5: crate::update::md5_bytes(&content[start as usize..(start + 50_000) as usize]),
            });
        }

        let report = apply_plan(&config, &plan_of(vec![item]), &ApplyOptions::default());
        assert!(report.results[0].ok, "{:?}", report.results[0].error);
        assert_eq!(dir.read("install/data/a.bin"), content);
        assert_eq!(report.downloaded_bytes, content.len() as u64);
        assert_eq!(server.ranges().len(), 4, "{:?}", server.ranges());
    }

    #[test]
    fn install_temp_uses_unique_adjacent_names() {
        let target = Path::new("/game/Content/pakchunk0-Windows_0_P.pak");
        let other = Path::new("/game/Content/pakchunk0-Windows_0_P.ucas");
        assert_ne!(adjacent_temp(target), adjacent_temp(other));
        assert_eq!(
            adjacent_temp(target),
            PathBuf::from("/game/Content/pakchunk0-Windows_0_P.pak.oddity-new")
        );
    }

    #[test]
    fn cleanup_stale_parts_removes_unreferenced_downloads() {
        let dir = TempDir::new("dl-cleanup");
        let config = config_for(&dir);
        let staging = config.patcher_dir.join("tmp/nte-native");
        fs::create_dir_all(staging.join("Res")).expect("create res staging");
        let md5 = "aa".repeat(16);
        fs::write(staging.join(format!("Res/{md5}.1.part")), b"x").expect("content-addressed");
        fs::write(staging.join(format!("Res/{md5}.1.ffff.part")), b"x").expect("keep");
        fs::write(staging.join(format!("Res/{md5}.1.cb.idx")), b"x").expect("keep idx");
        fs::write(staging.join("Res/bb.2.part"), b"x").expect("stale");
        let plan = plan_of(vec![res_item("data/a.bin", &md5, 1)]);
        cleanup_stale_parts(&staging, &plan);
        assert!(staging.join(format!("Res/{md5}.1.part")).exists());
        assert!(staging.join(format!("Res/{md5}.1.ffff.part")).exists());
        assert!(staging.join(format!("Res/{md5}.1.cb.idx")).exists());
        assert!(!staging.join("Res/bb.2.part").exists());
    }

    #[test]
    fn reports_download_verify_and_install_stages() {
        let dir = TempDir::new("dl-stages");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.cdns = vec![server.url("/cdn")];
        let content = b"payload bytes";
        let md5 = crate::update::md5_bytes(content);
        server.route(res_path(&md5, content.len() as u64), content);
        let mut stages = Vec::new();
        let report = apply_plan_observed_with_stage(
            &config,
            &plan_of(vec![res_item("data/a.bin", &md5, content.len() as u64)]),
            &ApplyOptions::default(),
            &mut |_| {},
            &mut |_, _| {},
            &mut |_, stage| stages.push(stage),
            &|| false,
        );
        assert!(report.results[0].ok);
        assert!(stages.contains(&ApplyStage::Download));
        assert!(stages.contains(&ApplyStage::Verify));
        assert!(stages.contains(&ApplyStage::Install));
    }

    #[test]
    fn multiple_items_are_downloaded_and_installed() {
        let dir = TempDir::new("dl-parallel");
        let server = HttpServer::start();
        let mut config = config_for(&dir);
        config.max_threads = 4;
        config.cdns = vec![server.url("/cdn")];
        let mut items = Vec::new();
        let mut expected = Vec::new();
        for index in 0..4u8 {
            let content = format!("parallel payload {index}").into_bytes();
            let md5 = crate::update::md5_bytes(&content);
            let path = format!("data/{index}.bin");
            server.route(res_path(&md5, content.len() as u64), &content);
            expected.push((path.clone(), content));
            items.push(res_item(&path, &md5, 18));
        }
        let report = apply_plan(&config, &plan_of(items), &ApplyOptions::default());
        assert_eq!(report.failed, 0);
        assert_eq!(report.results.len(), 4);
        for (path, content) in expected {
            assert_eq!(dir.read(format!("install/{path}")), content);
        }
    }
}
