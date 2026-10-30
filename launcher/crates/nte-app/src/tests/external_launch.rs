//! 外部启动入口与启动前状态检查.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use crate::download_commands::DownloadStatus;
use crate::{cli, prepare_launch, AppState, GameSession, GameTracker};
use nte_core::distribution::Distribution;
use nte_core::environment::{ProtonPin, RuntimeEnvironment};
use nte_core::settings::Settings;

fn managed_state(settings: Settings, game: GameTracker) -> AppState {
    AppState {
        settings: Mutex::new(settings),
        game: Mutex::new(game),
    }
}

fn session(closing: bool, alive: bool) -> GameSession {
    GameSession {
        id: 1,
        pid: 0,
        pgid: 0,
        exe: String::new(),
        started_at: 0,
        log: String::new(),
        mode: "background".into(),
        distribution: Distribution::Cn,
        closing,
        exit_notified: false,
        game_alive: Arc::new(AtomicBool::new(alive)),
        monitor_done: Arc::new(AtomicBool::new(false)),
        finalized: Arc::new(AtomicBool::new(false)),
        _install_lock: None,
        _prefix_lock: None,
        background: None,
        game_mode: None,
        snapshot: None,
        snapshot_success_after_ms: 0,
        launcher_root: None,
    }
}

fn global_environment() -> RuntimeEnvironment {
    RuntimeEnvironment {
        id: "0".repeat(32),
        name: "Global Env".into(),
        proton: ProtonPin {
            path: PathBuf::from("/tmp/proton"),
            identity: "identity".into(),
        },
        install_root: PathBuf::from("/tmp/install"),
        distribution: Distribution::Global,
        created_at: 0,
    }
}

#[test]
fn prepare_launch_rejects_busy_downloads() {
    let mut tracker = GameTracker::default();
    let mut status = DownloadStatus::default();
    status.phase = "game".into();
    status.mode = "apply".into();
    assert_eq!(
        prepare_launch(&mut tracker, &status).unwrap_err(),
        "游戏资源任务正在进行，暂时无法启动游戏"
    );
}

#[test]
fn prepare_launch_rejects_environment_tasks_and_starting_sessions() {
    let status = DownloadStatus::default();
    let mut environment = GameTracker {
        environment_busy: true,
        ..Default::default()
    };
    assert_eq!(
        prepare_launch(&mut environment, &status).unwrap_err(),
        "运行环境任务正在进行, 请完成后再启动游戏"
    );

    let mut starting = GameTracker {
        starting: true,
        ..Default::default()
    };
    assert_eq!(
        prepare_launch(&mut starting, &status).unwrap_err(),
        "游戏正在启动，请稍候。"
    );
}

#[test]
fn prepare_launch_rejects_closing_and_running_sessions() {
    let status = DownloadStatus::default();
    let mut closing = GameTracker {
        session: Some(session(true, false)),
        ..Default::default()
    };
    assert_eq!(
        prepare_launch(&mut closing, &status).unwrap_err(),
        "游戏正在关闭，请稍候。"
    );

    let mut running = GameTracker {
        session: Some(session(false, true)),
        ..Default::default()
    };
    assert_eq!(
        prepare_launch(&mut running, &status).unwrap_err(),
        "游戏已在运行。"
    );
}

#[test]
fn prepare_launch_clears_finished_sessions() {
    let status = DownloadStatus::default();
    let mut tracker = GameTracker {
        session: Some(session(false, false)),
        last_error: Some("旧错误".into()),
        ..Default::default()
    };
    prepare_launch(&mut tracker, &status).unwrap();
    assert!(tracker.starting);
    assert!(tracker.session.is_none());
    assert!(tracker.last_error.is_none());
}

#[test]
fn launch_summary_uses_region_environment_or_current_distribution() {
    let app = tauri::test::mock_app();
    let settings = Settings {
        distribution: Distribution::Cn,
        runtime_environments: vec![global_environment()],
        ..Default::default()
    };
    app.manage(managed_state(settings, GameTracker::default()));

    let region = cli::LaunchRequest {
        region: Some(Distribution::Global),
        mode: cli::LaunchMode::Background,
        environment: None,
    };
    assert_eq!(
        crate::launch_failure_summary(app.handle(), &region),
        "NTE启动失败"
    );

    let environment = cli::LaunchRequest {
        region: None,
        mode: cli::LaunchMode::Background,
        environment: Some("Global Env".into()),
    };
    assert_eq!(
        crate::launch_failure_summary(app.handle(), &environment),
        "NTE启动失败"
    );

    let current = cli::LaunchRequest {
        region: None,
        mode: cli::LaunchMode::Background,
        environment: None,
    };
    assert_eq!(
        crate::launch_failure_summary(app.handle(), &current),
        "异环启动失败"
    );

    let unknown = cli::LaunchRequest {
        region: None,
        mode: cli::LaunchMode::Background,
        environment: Some("missing".into()),
    };
    assert_eq!(
        crate::launch_failure_summary(app.handle(), &unknown),
        "异环启动失败"
    );
}

#[test]
fn launch_summary_falls_back_without_app_state() {
    let app = tauri::test::mock_app();
    let request = cli::LaunchRequest {
        region: None,
        mode: cli::LaunchMode::Background,
        environment: None,
    };
    assert_eq!(
        crate::launch_failure_summary(app.handle(), &request),
        "Oddity 启动失败"
    );
}

#[test]
fn take_launch_error_returns_and_clears_the_pending_error() {
    let app = tauri::test::mock_app();
    app.manage(managed_state(
        Settings::default(),
        GameTracker {
            last_error: Some("启动失败".into()),
            ..Default::default()
        },
    ));
    assert_eq!(
        crate::take_launch_error(app.state::<AppState>()),
        Some("启动失败".into())
    );
    assert_eq!(crate::take_launch_error(app.state::<AppState>()), None);
}

#[test]
fn launch_settings_clones_the_current_settings() {
    let app = tauri::test::mock_app();
    let settings = Settings {
        distribution: Distribution::Global,
        launch_mode: "direct".into(),
        ..Default::default()
    };
    app.manage(managed_state(settings, GameTracker::default()));

    let settings = crate::launch_settings(&app.state::<AppState>()).unwrap();
    assert_eq!(settings.distribution, Distribution::Global);
    assert_eq!(settings.launch_mode, "direct");
}

#[test]
fn launch_mode_names_are_stable() {
    assert_eq!(cli::LaunchMode::Official.as_str(), "official");
    assert_eq!(cli::LaunchMode::Background.as_str(), "background");
    assert_eq!(cli::LaunchMode::Direct.as_str(), "direct");
}
