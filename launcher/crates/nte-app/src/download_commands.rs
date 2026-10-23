use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use nte_core::config::LauncherConfig;
use nte_core::game::check::CheckOptions;
use nte_core::game::download::ApplyOptions;
use nte_core::{game, install, update};

use crate::{game_exe_for, process_group_alive, selected_install_root, AppState, Settings};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    pub(crate) phase: String,
    distribution: String,
    install_root: String,
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
            distribution: "cn".into(),
            install_root: String::new(),
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
}

#[derive(Default)]
pub struct DownloadManager {
    pub(crate) inner: Mutex<DownloadInner>,
}

pub(crate) fn busy(phase: &str) -> bool {
    matches!(phase, "checking" | "launcher" | "game")
}

fn publish(app: &AppHandle, change: impl FnOnce(&mut DownloadStatus)) {
    let manager = app.state::<DownloadManager>();
    let status = match manager.inner.lock() {
        Ok(mut inner) => {
            change(&mut inner.status);
            inner.status.clone()
        }
        Err(_) => return,
    };
    let _ = app.emit("game-download-state", status);
}

fn check_game(
    app: &AppHandle,
    settings: &Settings,
    root: &Path,
    full: bool,
) -> Result<game::UpdatePlan, String> {
    let config =
        LauncherConfig::load(root).map_err(|error| format!("读取启动器配置失败: {error}"))?;
    let game_config = game::GameUpdateConfig::load(root, config.game_id)
        .map_err(|error| format!("读取游戏资源配置失败: {error}"))?;
    let plan = game::check::check(
        &game_config,
        CheckOptions {
            full,
            include_optional: false,
        },
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

fn run_task(
    app: &AppHandle,
    settings: &Settings,
    mode: &str,
    full: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let install_root = selected_install_root(settings);
    let launcher_plan = install::plan_launcher(settings.distribution, &install_root, full)
        .map_err(|error| format!("检查启动器文件失败: {error}"))?;
    if cancel.load(Ordering::Relaxed) {
        publish(app, |status| status.phase = "cancelled".into());
        return Ok(());
    }
    publish(app, |status| {
        status.pending_files = launcher_plan.file_count();
        status.total_files = launcher_plan.file_count();
        status.total_bytes = launcher_plan.download_bytes;
        status.patch_files = 0;
        status.patch_bytes = 0;
        status.full_files = launcher_plan.file_count();
        status.full_bytes = launcher_plan.download_bytes;
        status.stage = "download".into();
    });
    if mode == "check" && !launcher_plan.files.is_empty() {
        publish(app, |status| status.phase = "needs-install".into());
        return Ok(());
    }
    if mode == "apply" && !launcher_plan.files.is_empty() {
        publish(app, |status| status.phase = "launcher".into());
        for (index, file) in launcher_plan.files.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                publish(app, |status| status.phase = "cancelled".into());
                return Ok(());
            }
            publish(app, |status| {
                status.current_file = file.path.clone();
                status.completed_files = index;
            });
            let mut progress = |bytes| {
                publish(app, |status| status.downloaded_bytes += bytes);
            };
            match update::apply_file_resumable(
                &launcher_plan.root,
                &launcher_plan.zip_bases,
                file,
                &mut progress,
                &|| cancel.load(Ordering::Relaxed),
            ) {
                Ok(_) => publish(app, |status| status.completed_files = index + 1),
                Err(nte_core::Error::Cancelled) => {
                    publish(app, |status| status.phase = "cancelled".into());
                    return Ok(());
                }
                Err(error) => return Err(format!("安装启动器文件 {} 失败: {error}", file.path)),
            }
        }
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
    let plan = check_game(app, settings, &launcher_plan.root, full)?;
    if cancel.load(Ordering::Relaxed) {
        publish(app, |status| status.phase = "cancelled".into());
        return Ok(());
    }
    if plan.items.is_empty() && game_exe_for(&launcher_plan.root).is_none() {
        return Err("资源清单未发现待下载文件，但本地缺少 HTGame.exe".into());
    }
    if mode == "check" {
        publish(app, |status| {
            status.phase = if plan.items.is_empty() {
                "ready"
            } else {
                "needs-update"
            }
            .into();
        });
        return Ok(());
    }
    if plan.items.is_empty() {
        publish(app, |status| status.phase = "ready".into());
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
        });
    };
    let mut on_bytes = |_: &str, bytes: u64| {
        publish(app, |status| status.downloaded_bytes += bytes);
    };
    let mut on_stage = |_: &str, stage: game::download::ApplyStage| {
        publish(app, |status| status.stage = stage.as_str().into());
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

#[tauri::command]
pub fn game_download_start(
    app: AppHandle,
    state: State<'_, AppState>,
    manager: State<'_, DownloadManager>,
    mode: String,
    full: bool,
) -> Result<DownloadStatus, String> {
    if mode != "check" && mode != "apply" {
        return Err("无效的游戏资源操作".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let (status, settings) = {
        let mut inner = manager.inner.lock().map_err(|error| error.to_string())?;
        if busy(&inner.status.phase) {
            return Err("游戏资源任务正在进行".into());
        }
        let settings = state
            .settings
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        if mode == "apply" {
            let game = state.game.lock().map_err(|error| error.to_string())?;
            if game.starting
                || game
                    .session
                    .as_ref()
                    .is_some_and(|session| process_group_alive(session.pgid))
            {
                return Err("游戏正在运行，无法修改游戏文件".into());
            }
        }
        inner.cancel = Some(cancel.clone());
        inner.status = DownloadStatus {
            phase: "checking".into(),
            distribution: settings.distribution.as_str().into(),
            install_root: selected_install_root(&settings).display().to_string(),
            ..DownloadStatus::default()
        };
        (inner.status.clone(), settings)
    };
    let _ = app.emit("game-download-state", status.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let result = run_task(&app, &settings, &mode, full, &cancel);
        if let Err(error) = result {
            publish(&app, |status| {
                status.phase = "failed".into();
                status.error = error;
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
