use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use nte_core::config::LauncherConfig;
use nte_core::game::check::CheckOptions;
use nte_core::game::download::ApplyOptions;
use nte_core::{game, install, update};

use crate::{game_exe_for, selected_install_root, session_alive, AppState, Settings};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    pub(crate) phase: String,
    pub(crate) mode: String,
    distribution: String,
    install_root: String,
    /// 官方本地清单存在时视为已安装，用于区分安装与更新。
    installed: bool,
    full: bool,
    unverified_files: usize,
    current_file: String,
    current_file_bytes: u64,
    completed_files: usize,
    total_files: usize,
    downloaded_bytes: u64,
    total_bytes: u64,
    patch_files: usize,
    patch_bytes: u64,
    full_files: usize,
    full_bytes: u64,
    stage: String,
    remote_version: String,
    pending_files: usize,
    error: String,
}

impl Default for DownloadStatus {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            mode: "check".into(),
            distribution: "cn".into(),
            install_root: String::new(),
            installed: false,
            full: false,
            unverified_files: 0,
            current_file: String::new(),
            current_file_bytes: 0,
            completed_files: 0,
            total_files: 0,
            downloaded_bytes: 0,
            total_bytes: 0,
            patch_files: 0,
            patch_bytes: 0,
            full_files: 0,
            full_bytes: 0,
            stage: String::new(),
            remote_version: String::new(),
            pending_files: 0,
            error: String::new(),
        }
    }
}

#[derive(Default)]
pub(crate) struct DownloadInner {
    pub(crate) status: DownloadStatus,
    cancel: Option<Arc<AtomicBool>>,
    last_progress: Option<Instant>,
}

#[derive(Default)]
pub struct DownloadManager {
    pub(crate) inner: Mutex<DownloadInner>,
}

pub(crate) fn busy(phase: &str) -> bool {
    matches!(phase, "checking" | "launcher" | "game")
}

pub(crate) fn blocks_changes(status: &DownloadStatus) -> bool {
    busy(&status.phase) && status.mode == "apply"
}

fn publish<R: Runtime>(app: &AppHandle<R>, change: impl FnOnce(&mut DownloadStatus)) {
    let manager = app.state::<DownloadManager>();
    // 进度事件必须在持锁时发送, 否则多线程并行下载时事件可能乱序,
    // 界面会看到已完成的文件数倒退。
    let Ok(mut inner) = manager.inner.lock() else {
        return;
    };
    change(&mut inner.status);
    let status = inner.status.clone();
    let _ = app.emit("game-download-state", status);
}

fn publish_bytes<R: Runtime>(app: &AppHandle<R>, bytes: u64) {
    let manager = app.state::<DownloadManager>();
    let Ok(mut inner) = manager.inner.lock() else {
        return;
    };
    inner.status.downloaded_bytes += bytes;
    let now = Instant::now();
    if inner
        .last_progress
        .is_some_and(|last| now.duration_since(last) < Duration::from_millis(150))
    {
        return;
    }
    inner.last_progress = Some(now);
    let status = inner.status.clone();
    let _ = app.emit("game-download-state", status);
}

fn check_game<R: Runtime>(
    app: &AppHandle<R>,
    settings: &Settings,
    root: &Path,
    full: bool,
    cancel: &AtomicBool,
) -> Result<game::UpdatePlan, String> {
    let config =
        LauncherConfig::load(root).map_err(|error| format!("读取启动器配置失败: {error}"))?;
    let game_config = game::GameUpdateConfig::load(root, config.game_id)
        .map_err(|error| format!("读取游戏资源配置失败: {error}"))?;
    let plan = game::check::check_cancellable(
        &game_config,
        CheckOptions {
            full,
            include_optional: false,
        },
        &|| cancel.load(Ordering::Relaxed),
    )
    .map_err(|error| format!("检查游戏资源失败: {error}"))?;
    let patch_files = plan.items.iter().filter(|item| item.has_patch()).count();
    let patch_bytes: u64 = plan
        .items
        .iter()
        .filter(|item| item.has_patch())
        .map(|item| item.download_size)
        .sum();
    let total_bytes = plan.total_bytes();
    publish(app, |status| {
        status.distribution = settings.distribution.as_str().into();
        status.remote_version = plan.remote_version.clone();
        status.unverified_files = plan.unverified.len();
        status.pending_files = plan.items.len();
        status.total_bytes = total_bytes;
        status.total_files = plan.items.len();
        status.patch_files = patch_files;
        status.patch_bytes = patch_bytes;
        status.full_files = plan.items.len().saturating_sub(patch_files);
        status.full_bytes = total_bytes.saturating_sub(patch_bytes);
    });
    Ok(plan)
}

fn run_task<R: Runtime>(
    app: &AppHandle<R>,
    settings: &Settings,
    mode: &str,
    full: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let install_root = selected_install_root(settings);
    let launcher_plan =
        install::plan_launcher_cancellable(settings.distribution, &install_root, full, &|| {
            cancel.load(Ordering::Relaxed)
        })
        .map_err(|error| format!("检查启动器文件失败: {error}"))?;
    run_plan(app, settings, mode, full, cancel, launcher_plan)
}

/// 启动器文件大多是几十到几百 KiB 的 ZIP, 逐个下载会被握手和首字节
/// 延迟拖慢; 这里使用与游戏资源一致的固定并发。
const LAUNCHER_WORKERS: usize = 5;

fn install_launcher_files<R: Runtime>(
    app: &AppHandle<R>,
    plan: &install::LauncherPlan,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let files = &plan.files;
    let root = &plan.root;
    let zip_bases = &plan.zip_bases;
    let workers = LAUNCHER_WORKERS.min(files.len()).max(1);
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let first_error = Mutex::new(None::<String>);
    let next_ref = &next;
    let stop_ref = &stop;
    let first_error_ref = &first_error;
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let app = app.clone();
            scope.spawn(move || loop {
                if cancel.load(Ordering::Relaxed) || stop_ref.load(Ordering::Relaxed) {
                    return;
                }
                let index = next_ref.fetch_add(1, Ordering::Relaxed);
                let Some(file) = files.get(index) else {
                    return;
                };
                publish(&app, |status| {
                    status.current_file = file.path.clone();
                    status.current_file_bytes = file.zip_size.unwrap_or(file.size);
                });
                let mut progress = |bytes| publish_bytes(&app, bytes);
                let outcome =
                    update::apply_file_resumable(root, zip_bases, file, &mut progress, &|| {
                        cancel.load(Ordering::Relaxed) || stop_ref.load(Ordering::Relaxed)
                    });
                match outcome {
                    Ok(_) => publish(&app, |status| {
                        status.completed_files += 1;
                        status.pending_files = status.pending_files.saturating_sub(1);
                        if status.current_file == file.path {
                            status.current_file.clear();
                            status.current_file_bytes = 0;
                        }
                    }),
                    Err(nte_core::Error::Cancelled) => return,
                    Err(error) => {
                        if let Ok(mut slot) = first_error_ref.lock() {
                            if slot.is_none() {
                                *slot = Some(format!("安装启动器文件 {} 失败: {error}", file.path));
                            }
                        }
                        stop_ref.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            });
        }
    });
    if let Some(error) = first_error.into_inner().unwrap_or(None) {
        return Err(error);
    }
    if cancel.load(Ordering::Relaxed) {
        publish(app, |status| status.phase = "cancelled".into());
        return Ok(());
    }
    Ok(())
}

fn run_plan<R: Runtime>(
    app: &AppHandle<R>,
    settings: &Settings,
    mode: &str,
    full: bool,
    cancel: &AtomicBool,
    launcher_plan: install::LauncherPlan,
) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        publish(app, |status| status.phase = "cancelled".into());
        return Ok(());
    }
    publish(app, |status| {
        status.pending_files = launcher_plan.file_count();
        status.total_files = launcher_plan.file_count();
        status.total_bytes = launcher_plan.download_bytes;
        status.completed_files = 0;
        status.downloaded_bytes = 0;
        status.current_file.clear();
        status.current_file_bytes = 0;
        status.patch_files = 0;
        status.patch_bytes = 0;
        status.full_files = launcher_plan.file_count();
        status.full_bytes = launcher_plan.download_bytes;
        status.stage = "download".into();
        status.error.clear();
    });
    if mode == "check" && !launcher_plan.files.is_empty() {
        publish(app, |status| status.phase = "needs-install".into());
        return Ok(());
    }
    if mode == "apply" && !launcher_plan.files.is_empty() {
        publish(app, |status| status.phase = "launcher".into());
        install_launcher_files(app, &launcher_plan, cancel)?;
    }
    if cancel.load(Ordering::Relaxed) {
        publish(app, |status| status.phase = "cancelled".into());
        return Ok(());
    }
    publish(app, |status| {
        status.phase = "checking".into();
        status.current_file.clear();
        status.current_file_bytes = 0;
        status.completed_files = 0;
        status.total_files = 0;
        status.pending_files = 0;
        status.downloaded_bytes = 0;
        status.total_bytes = 0;
        status.patch_files = 0;
        status.patch_bytes = 0;
        status.full_files = 0;
        status.full_bytes = 0;
        status.stage.clear();
    });
    let mut plan = check_game(app, settings, &launcher_plan.root, full, cancel)?;
    // 应用任务必须先确认未知资源，不能把仅大小相符的大文件视为已校验。
    if mode == "apply" && !plan.unverified.is_empty() && !cancel.load(Ordering::Relaxed) {
        publish(app, |status| status.full = true);
        plan = check_game(app, settings, &launcher_plan.root, true, cancel)?;
    }
    if cancel.load(Ordering::Relaxed) {
        publish(app, |status| status.phase = "cancelled".into());
        return Ok(());
    }
    if plan.items.is_empty() && game_exe_for(&launcher_plan.root).is_none() {
        return Err("资源清单未发现待下载文件，但本地缺少 HTGame.exe".into());
    }
    if mode == "check" {
        publish(app, |status| {
            status.phase = if !plan.unverified.is_empty() {
                "needs-verify"
            } else if plan.items.is_empty() {
                "ready"
            } else {
                "needs-update"
            }
            .into();
        });
        return Ok(());
    }
    let launcher_config = LauncherConfig::load(&launcher_plan.root)
        .map_err(|error| format!("读取启动器配置失败: {error}"))?;
    let game_config = game::GameUpdateConfig::load(&launcher_plan.root, launcher_config.game_id)
        .map_err(|error| format!("读取游戏资源配置失败: {error}"))?;
    publish(app, |status| {
        status.phase = "game".into();
        status.completed_files = 0;
        status.downloaded_bytes = 0;
        status.pending_files = plan.items.len();
        status.total_files = plan.items.len();
        status.stage = "download".into();
    });
    let mut on_file = |item: &game::UpdateItem| {
        publish(app, |status| {
            status.current_file = item.path.clone();
            status.current_file_bytes = item.size;
            status.stage = "download".into();
        });
    };
    let mut on_bytes = |_: &str, bytes: u64| {
        publish_bytes(app, bytes);
    };
    let mut on_stage = |path: &str, stage: game::download::ApplyStage| {
        publish(app, |status| {
            status.current_file = path.into();
            status.current_file_bytes = plan
                .items
                .iter()
                .find(|item| item.path == path)
                .map_or(0, |item| item.size);
            status.stage = stage.as_str().into();
        });
    };
    let mut on_result = |result: &game::download::ApplyResult| {
        publish(app, |status| {
            if result.ok {
                status.completed_files += 1;
            }
            status.pending_files = status.pending_files.saturating_sub(1);
            if status.current_file == result.path {
                status.current_file.clear();
                status.current_file_bytes = 0;
                status.stage.clear();
            }
            if status.pending_files == 0 {
                status.stage.clear();
            }
        });
    };
    let report = game::download::apply_plan_observed_full(
        &game_config,
        &plan,
        &ApplyOptions::default(),
        &mut on_file,
        &mut on_bytes,
        &mut on_stage,
        &mut on_result,
        &|| cancel.load(Ordering::Relaxed),
    );
    if report.cancelled {
        publish(app, |status| status.phase = "cancelled".into());
        return Ok(());
    }
    if report.failed > 0 {
        let first = report
            .results
            .iter()
            .find_map(|item| item.error.as_ref())
            .cloned()
            .unwrap_or_default();
        return Err(format!("{} 个游戏文件下载失败: {first}", report.failed));
    }
    if let Some(error) = report.ledger_error {
        return Err(format!("写入游戏资源记录失败: {error}"));
    }
    if game_exe_for(&launcher_plan.root).is_none() {
        return Err("资源下载已结束，但未找到游戏可执行文件".into());
    }
    publish(app, |status| {
        status.phase = "ready".into();
        status.completed_files = status.total_files;
        status.pending_files = 0;
        status.current_file.clear();
        status.current_file_bytes = 0;
        status.stage.clear();
    });
    Ok(())
}

#[tauri::command]
pub fn game_download_status(manager: State<'_, DownloadManager>) -> DownloadStatus {
    manager
        .inner
        .lock()
        .map(|inner| inner.status.clone())
        .unwrap_or_default()
}

fn prepare_task(
    state: &AppState,
    manager: &DownloadManager,
    mode: &str,
    full: bool,
    cancel: &Arc<AtomicBool>,
) -> Result<(DownloadStatus, Settings, bool), String> {
    if mode != "check" && mode != "apply" {
        return Err("无效的游戏资源操作".into());
    }
    let (status, settings, full) = {
        let mut inner = manager.inner.lock().map_err(|error| error.to_string())?;
        if busy(&inner.status.phase) {
            return Err("游戏资源任务正在进行".into());
        }
        let mut settings = state
            .settings
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        // 未配置目录时沿用环境探测结果, 只用于本轮任务。
        if crate::configured_install_root(&settings).is_none() {
            if let Some(root) = crate::detect_launcher_root(&settings)
                .and_then(|root| root.parent().map(Path::to_path_buf))
            {
                let root = Some(root.display().to_string());
                match settings.distribution {
                    nte_core::distribution::Distribution::Cn => settings.install_root = root,
                    nte_core::distribution::Distribution::Global => {
                        settings.global_install_root = root
                    }
                }
            }
        }
        if mode == "apply" {
            let game = state.game.lock().map_err(|error| error.to_string())?;
            if game.starting || game.session.as_ref().is_some_and(session_alive) {
                return Err("游戏正在运行，无法修改游戏文件".into());
            }
        }
        let install_root = selected_install_root(&settings);
        let installed = install_root
            .join(settings.distribution.launcher_dir())
            .join("UserData/Patcher/PatcherSDK/ResList.xml")
            .is_file();
        let install_root = install_root.display().to_string();
        let full = full
            || (inner.status.full
                && inner.status.phase != "ready"
                && inner.status.distribution == settings.distribution.as_str()
                && inner.status.install_root == install_root);
        inner.cancel = Some(cancel.clone());
        inner.last_progress = None;
        inner.status = DownloadStatus {
            phase: "checking".into(),
            mode: mode.into(),
            distribution: settings.distribution.as_str().into(),
            install_root,
            installed,
            full,
            ..DownloadStatus::default()
        };
        (inner.status.clone(), settings, full)
    };
    Ok((status, settings, full))
}

#[tauri::command]
pub fn game_download_start<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    manager: State<'_, DownloadManager>,
    mode: String,
    full: bool,
) -> Result<DownloadStatus, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    let (status, settings, full) = prepare_task(&state, &manager, &mode, full, &cancel)?;
    let _ = app.emit("game-download-state", status.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let result = run_task(&app, &settings, &mode, full, &cancel);
        if let Err(error) = result {
            publish(&app, |status| {
                status.phase = if cancel.load(Ordering::Relaxed) {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                status.error = if cancel.load(Ordering::Relaxed) {
                    String::new()
                } else {
                    error
                };
                status.stage.clear();
            });
        }
        if let Ok(mut inner) = app.state::<DownloadManager>().inner.lock() {
            if inner
                .cancel
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &cancel))
            {
                inner.cancel = None;
            }
        }
    });
    Ok(status)
}

#[tauri::command]
pub fn game_download_cancel(manager: State<'_, DownloadManager>) -> DownloadStatus {
    if let Ok(inner) = manager.inner.lock() {
        if let Some(cancel) = &inner.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        inner.status.clone()
    } else {
        DownloadStatus::default()
    }
}

#[cfg(test)]
#[path = "tests/download.rs"]
mod tests;
