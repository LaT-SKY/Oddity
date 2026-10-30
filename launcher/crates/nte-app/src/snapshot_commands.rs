//! 启动环境快照的命令与状态.
use std::path::Path;
use std::sync::{Arc, Mutex};

use nte_core::config::LauncherConfig;
use nte_core::distribution::Distribution;
use nte_core::settings::Settings;
use nte_core::snapshots::{
    self, Attempt, Diff, DiffRow, Failure, Reference, Snapshot, SnapshotStore, Status,
};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{environment_commands, AppState};

/// 托管在 Tauri 状态里的快照记录. 启动线程, 监控线程和命令共用同一把锁.
#[derive(Clone)]
pub struct Snapshots {
    inner: Arc<Mutex<SnapshotStore>>,
}

impl Snapshots {
    pub fn load(data_dir: &Path) -> Self {
        let store = SnapshotStore::load(data_dir.join("environment-snapshots.json"));
        Self {
            inner: Arc::new(Mutex::new(store)),
        }
    }

    pub fn begin(&self, snapshot: Snapshot) -> Result<String, String> {
        self.lock()?
            .begin(snapshot, crate::now_millis())
            .map_err(|error| format!("{error:#}"))
    }

    pub fn mark_running(&self, id: &str, now: u64) {
        if let Ok(mut store) = self.inner.lock() {
            if let Err(error) = store.mark_running(id, now) {
                eprintln!("记录游戏运行状态失败: {error:#}");
            }
        }
    }

    pub fn mark_success(
        &self,
        id: &str,
        now: u64,
        game_version: Option<String>,
        success_after_ms: u64,
    ) -> bool {
        let Ok(mut store) = self.inner.lock() else {
            return false;
        };
        match store.mark_success(id, now, game_version, success_after_ms) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("记录启动成功失败: {error:#}");
                false
            }
        }
    }

    pub fn finish(&self, id: &str, failure: Option<Failure>) {
        if let Ok(mut store) = self.inner.lock() {
            if let Err(error) = store.finish(id, crate::now_millis(), failure) {
                eprintln!("记录启动结束失败: {error:#}");
            }
        }
    }

    pub fn fail(&self, id: &str, failure: Failure) {
        if let Ok(mut store) = self.inner.lock() {
            if let Err(error) = store.finish(id, crate::now_millis(), Some(failure)) {
                eprintln!("记录启动失败: {error:#}");
            }
        }
    }

    pub fn overview(&self) -> Result<SnapshotOverview, String> {
        let store = self.lock()?;
        Ok(SnapshotOverview {
            baseline: store.baseline().map(summary),
            reference: store.reference().map(reference_view),
            latest: store.latest().map(summary),
            attempts: store.attempts().len(),
        })
    }

    pub fn compare(&self, target: &str, current: Snapshot) -> Result<SnapshotComparison, String> {
        let store = self.lock()?;
        let (target_label, target_time, target_environment, target_snapshot) = match target {
            "reference" => {
                let reference = store.reference().ok_or("尚未保存参考快照")?;
                (
                    "参考快照",
                    reference.captured_at,
                    reference.snapshot.runtime.environment_name.clone(),
                    &reference.snapshot,
                )
            }
            "baseline" => {
                let baseline = store.baseline().ok_or("还没有成功启动记录")?;
                (
                    "上次成功启动",
                    baseline.captured_at,
                    baseline.snapshot.runtime.environment_name.clone(),
                    &baseline.snapshot,
                )
            }
            _ => return Err("未知的对比目标".into()),
        };
        let diff = Diff::between(target_snapshot, &current);
        Ok(SnapshotComparison {
            target: target.to_string(),
            target_label: target_label.to_string(),
            target_time,
            target_environment,
            current_environment: current.runtime.environment_name.clone(),
            rows: diff.rows,
            changed: diff.changed,
            unknown: diff.unknown,
            unchanged: diff.unchanged,
        })
    }

    pub fn save_reference(
        &self,
        current: &Snapshot,
        label: Option<String>,
        attempt_id: Option<String>,
    ) -> Result<ReferenceView, String> {
        let mut store = self.lock()?;
        let now = crate::now_millis();
        let (snapshot, captured_at) = match attempt_id {
            Some(id) => {
                let attempt = store
                    .find(&id)
                    .ok_or_else(|| "所选启动记录不存在".to_string())?;
                (attempt.snapshot.clone(), attempt.captured_at)
            }
            None => (current.clone(), now),
        };
        let reference = store
            .save_reference(snapshot, captured_at, now, label.as_deref().unwrap_or(""))
            .map_err(|error| format!("{error:#}"))?;
        Ok(reference_view(&reference))
    }

    pub fn delete_reference(&self) -> Result<(), String> {
        self.lock()?
            .delete_reference()
            .map_err(|error| format!("{error:#}"))
    }

    pub fn clear_history(&self) -> Result<(), String> {
        self.lock()?
            .clear_history()
            .map_err(|error| format!("{error:#}"))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, SnapshotStore>, String> {
        self.inner
            .lock()
            .map_err(|_| "环境快照状态不可用".to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    pub id: String,
    pub captured_at: u64,
    pub status: Status,
    pub duration_ms: Option<u64>,
    pub failure: Option<Failure>,
    pub environment: Option<String>,
    pub proton: Option<String>,
    pub distribution: Distribution,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceView {
    pub label: String,
    pub saved_at: u64,
    pub captured_at: u64,
    pub environment: Option<String>,
    pub proton: Option<String>,
    pub distribution: Distribution,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotOverview {
    pub baseline: Option<SnapshotSummary>,
    pub reference: Option<ReferenceView>,
    pub latest: Option<SnapshotSummary>,
    pub attempts: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotComparison {
    pub target: String,
    pub target_label: String,
    pub target_time: u64,
    pub target_environment: Option<String>,
    pub current_environment: Option<String>,
    pub rows: Vec<DiffRow>,
    pub changed: usize,
    pub unknown: usize,
    pub unchanged: usize,
}

fn summary(attempt: &Attempt) -> SnapshotSummary {
    SnapshotSummary {
        id: attempt.id.clone(),
        captured_at: attempt.captured_at,
        status: attempt.status,
        duration_ms: attempt.duration_ms,
        failure: attempt.failure.clone(),
        environment: attempt.snapshot.runtime.environment_name.clone(),
        proton: attempt.snapshot.runtime.proton_name.clone(),
        distribution: attempt.snapshot.game.distribution,
        mode: attempt.snapshot.runtime.launch_mode.clone(),
    }
}

fn reference_view(reference: &Reference) -> ReferenceView {
    ReferenceView {
        label: reference.label.clone(),
        saved_at: reference.saved_at,
        captured_at: reference.captured_at,
        environment: reference.snapshot.runtime.environment_name.clone(),
        proton: reference.snapshot.runtime.proton_name.clone(),
        distribution: reference.snapshot.game.distribution,
        mode: reference.snapshot.runtime.launch_mode.clone(),
    }
}

/// 从当前设置构造快照输入; 缺失的字段留空, 不阻断对比.
pub(crate) fn current_inputs(settings: &Settings) -> snapshots::Inputs {
    let launcher_root = crate::detect_launcher_root(settings);
    let install_root = crate::effective_install_root(settings, launcher_root.as_deref());
    let environment = settings.active_environment.as_ref().and_then(|id| {
        settings
            .runtime_environments
            .iter()
            .find(|environment| &environment.id == id)
    });
    let proton_name = environment
        .and_then(|environment| environment.proton.path.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    let proton_identity = environment.map(|environment| environment.proton.identity.clone());
    let proton_ok = environment.is_some_and(|environment| environment.proton.verify().is_ok());
    let (prefix, prefix_ok) = match environment_commands::prefix(settings) {
        Ok(prefix) => {
            let ok = prefix.join("drive_c").is_dir();
            (Some(prefix), ok)
        }
        Err(_) => (None, false),
    };
    let game_version = launcher_root
        .as_ref()
        .map(|root| crate::game_version_for(root))
        .filter(|version| !version.is_empty());
    let launcher_version = launcher_root
        .as_ref()
        .and_then(|root| LauncherConfig::load(root).ok())
        .map(|config| config.version);
    let umu = crate::which("umu-run");
    let gamescope = &settings.game_mode.gamescope;
    snapshots::Inputs {
        distribution: settings.distribution,
        install_root: Some(install_root),
        launcher_root,
        game_version,
        launcher_version,
        environment_id: environment.map(|environment| environment.id.clone()),
        environment_name: environment.map(|environment| environment.name.clone()),
        proton_name,
        proton_identity,
        proton_ok,
        prefix,
        prefix_ok,
        launch_mode: settings.launch_mode.clone(),
        gamescope: snapshots::Gamescope {
            enabled: gamescope.enabled,
            width: gamescope.game_width,
            height: gamescope.game_height,
            fps: gamescope.fps,
        },
        umu,
    }
}

#[tauri::command]
pub fn environment_snapshot_overview(
    snapshots: State<'_, Snapshots>,
) -> Result<SnapshotOverview, String> {
    snapshots.overview()
}

#[tauri::command]
pub async fn environment_snapshot_compare(
    app: AppHandle,
    target: String,
) -> Result<SnapshotComparison, String> {
    let settings = app
        .state::<AppState>()
        .settings
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let snapshots = app.state::<Snapshots>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let current = snapshots::capture(current_inputs(&settings));
        snapshots.compare(&target, current)
    })
    .await
    .map_err(|error| format!("环境对比任务失败: {error}"))?
}

#[tauri::command]
pub async fn environment_snapshot_save_reference(
    app: AppHandle,
    label: Option<String>,
    attempt_id: Option<String>,
) -> Result<ReferenceView, String> {
    let settings = app
        .state::<AppState>()
        .settings
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let snapshots = app.state::<Snapshots>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let current = snapshots::capture(current_inputs(&settings));
        snapshots.save_reference(&current, label, attempt_id)
    })
    .await
    .map_err(|error| format!("保存参考快照任务失败: {error}"))?
}

#[tauri::command]
pub fn environment_snapshot_delete_reference(
    snapshots: State<'_, Snapshots>,
) -> Result<(), String> {
    snapshots.delete_reference()
}

#[tauri::command]
pub fn environment_snapshot_clear_history(snapshots: State<'_, Snapshots>) -> Result<(), String> {
    snapshots.clear_history()
}
