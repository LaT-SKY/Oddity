use super::*;
use crate::tests::{HttpServer, TempDir};
use crate::GameTracker;
use nte_core::distribution::Distribution;
use nte_core::manifest::ManifestFile;
use nte_core::update::{md5_bytes, md5_file};
use std::io::{Cursor, Write};
use tauri::test::{mock_app, MockRuntime};
use tauri::Listener;

const EXE: &str = "Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe";
const DATA: &str = "Client/WindowsNoEditor/HT/Content/data.pak";

fn zip(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(bytes).unwrap();
    writer.finish().unwrap().into_inner()
}

struct Fixture {
    dir: TempDir,
    server: HttpServer,
    settings: Settings,
    files: Vec<ManifestFile>,
    data: Vec<u8>,
    app: tauri::App<MockRuntime>,
}

impl Fixture {
    fn new(distribution: Distribution, installed: bool) -> Self {
        let dir = TempDir::new("download-task");
        let server = HttpServer::start();
        let root = distribution.launcher_dir();
        let id = if distribution == Distribution::Cn {
            "1289"
        } else {
            "3000001"
        };
        let ini = format!(
            "[General]\nGameID={id}\nClient={}\n",
            distribution.client_exe()
        );
        let patcher = serde_json::json!({
            "gameResUrl": [server.base()], "branchName": "test", "appId": id
        })
        .to_string();
        let config_path = format!("ResFilesM/{id}/PatcherConfig/PatcherConfig.json");
        let mut files = Vec::new();
        for (path, content) in [
            ("Config/Config.ini", ini.as_bytes()),
            (&config_path, patcher.as_bytes()),
        ] {
            let bytes = zip(path, content);
            server.route(format!("/launcher/{path}.zip"), &bytes);
            files.push(ManifestFile {
                path: path.into(),
                checksum: md5_bytes(content),
                size: content.len() as u64,
                zip_size: Some(bytes.len() as u64),
                zip_checksum: Some(md5_bytes(&bytes)),
            });
            if installed {
                dir.write(format!("{root}/{path}"), content);
            }
        }
        let data = vec![23; 21 * 1024 * 1024];
        let exe = b"fixture game executable";
        let list = format!("<ResList version=\"1.0\" tag=\"baseTag\"><Res filename=\"{EXE}\" md5=\"{}\" filesize=\"{}\"/><Res filename=\"{DATA}\" md5=\"{}\" filesize=\"{}\"/></ResList>", md5_bytes(exe), exe.len(), md5_bytes(&data), data.len());
        server.route("/test/Version/Windows/config.xml", format!("<Config><ResVersion>1.0</ResVersion><Hash>hash</Hash><listHash>{}</listHash></Config>", md5_bytes(list.as_bytes())));
        let encoded = game::pxml::encode(list.as_bytes(), id).unwrap();
        server.route(
            "/test/Version/Windows/version/1.0/ResList.bin.zip",
            zip("ResList.bin", &encoded),
        );
        for bytes in [exe.as_slice(), data.as_slice()] {
            let hash = md5_bytes(bytes);
            server.route(
                format!("/test/Res/{}/{}.{}", &hash[..1], hash, bytes.len()),
                bytes,
            );
        }
        if installed {
            dir.write(EXE, exe);
            dir.write(DATA, &data);
        }
        let settings = Settings {
            distribution,
            install_root: Some(dir.path().display().to_string()),
            global_install_root: Some(dir.path().display().to_string()),
            ..Settings::default()
        };
        let app = mock_app();
        app.manage(DownloadManager::default());
        app.manage(AppState {
            settings: Mutex::new(settings.clone()),
            game: Mutex::new(GameTracker::default()),
        });
        Self {
            dir,
            server,
            settings,
            files,
            data,
            app,
        }
    }

    fn plan(&self, install: bool) -> install::LauncherPlan {
        install::LauncherPlan {
            root: self.dir.join(self.settings.distribution.launcher_dir()),
            version: "1.0".into(),
            files: if install { self.files.clone() } else { vec![] },
            download_bytes: self.files.iter().map(|file| file.zip_size.unwrap()).sum(),
            zip_bases: vec![self.server.url("/launcher")],
        }
    }

    fn run(&self, mode: &str, full: bool, install: bool) -> Result<(), String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let (_, _, full) = prepare_task(
            &self.app.state::<AppState>(),
            &self.app.state::<DownloadManager>(),
            mode,
            full,
            &cancel,
        )?;
        run_plan(
            self.app.handle(),
            &self.settings,
            mode,
            full,
            &cancel,
            self.plan(install),
        )
    }

    fn status(&self) -> DownloadStatus {
        game_download_status(self.app.state::<DownloadManager>())
    }

    fn ledger(&self) {
        let id = if self.settings.distribution == Distribution::Cn {
            "1289"
        } else {
            "3000001"
        };
        let xml = format!("<ResList version=\"1.0\" tag=\"baseTag\"><Res filename=\"{DATA}\" md5=\"{}\" filesize=\"{}\"/></ResList>", md5_bytes(&self.data), self.data.len());
        self.dir.write(
            format!(
                "{}/UserData/Patcher/PatcherSDK/ResList.xml",
                self.settings.distribution.launcher_dir()
            ),
            game::pxml::encode(xml.as_bytes(), id).unwrap(),
        );
    }
}

#[test]
fn installs_from_an_empty_directory_for_both_regions() {
    for distribution in [Distribution::Cn, Distribution::Global] {
        let fixture = Fixture::new(distribution, false);
        fixture.run("check", false, true).unwrap();
        assert_eq!(fixture.status().phase, "needs-install");
        assert!(!fixture.status().installed);
        fixture.run("apply", false, true).unwrap();
        let status = fixture.status();
        assert_eq!(status.phase, "ready");
        assert_eq!(status.pending_files, 0);
        assert_eq!(status.completed_files, status.total_files);
        assert!(status.current_file.is_empty());
        assert!(status.stage.is_empty());
        assert_eq!(
            md5_file(&fixture.dir.join(DATA)).unwrap(),
            md5_bytes(&fixture.data)
        );
        fixture.run("check", true, false).unwrap();
        assert_eq!(fixture.status().phase, "ready");
        assert!(fixture.status().installed);
    }
}

#[test]
fn a_full_check_is_preserved_until_corruption_is_repaired() {
    let fixture = Fixture::new(Distribution::Cn, true);
    fixture.ledger();
    fixture.dir.write(DATA, vec![9; fixture.data.len()]);
    fixture.run("check", true, false).unwrap();
    assert_eq!(fixture.status().phase, "needs-update");
    assert!(fixture.status().installed);
    assert_eq!(fixture.status().pending_files, 1);
    // 按钮传 false 时，后端仍须保留先前的完整校验要求。
    fixture.run("apply", false, false).unwrap();
    assert!(fixture.status().full);
    assert_eq!(fixture.status().phase, "ready");
    assert_eq!(
        md5_file(&fixture.dir.join(DATA)).unwrap(),
        md5_bytes(&fixture.data)
    );
}

#[test]
fn unknown_large_files_require_verification_and_apply_repairs_them() {
    let fixture = Fixture::new(Distribution::Global, true);
    fixture.dir.write(DATA, vec![7; fixture.data.len()]);
    fixture.run("check", false, false).unwrap();
    assert_eq!(fixture.status().phase, "needs-verify");
    assert_eq!(fixture.status().unverified_files, 1);
    fixture.run("apply", false, false).unwrap();
    assert_eq!(fixture.status().phase, "ready");
    assert_eq!(fixture.status().unverified_files, 0);
    assert!(fixture.status().full);
    assert_eq!(
        md5_file(&fixture.dir.join(DATA)).unwrap(),
        md5_bytes(&fixture.data)
    );
}

#[test]
fn verified_existing_files_are_recorded_by_apply_without_redownloading() {
    let fixture = Fixture::new(Distribution::Cn, true);
    fixture.run("apply", false, false).unwrap();
    assert_eq!(fixture.status().phase, "ready");
    assert_eq!(fixture.status().downloaded_bytes, 0);
    fixture.run("check", false, false).unwrap();
    assert_eq!(fixture.status().phase, "ready");
    assert_eq!(fixture.status().unverified_files, 0);
    assert!(!fixture
        .server
        .targets()
        .iter()
        .any(|path| path.contains("/Res/")));
}

#[test]
fn cancellation_preserves_the_task_and_allows_a_fresh_attempt() {
    let fixture = Fixture::new(Distribution::Cn, false);
    let cancel = Arc::new(AtomicBool::new(false));
    prepare_task(
        &fixture.app.state::<AppState>(),
        &fixture.app.state::<DownloadManager>(),
        "apply",
        true,
        &cancel,
    )
    .unwrap();
    game_download_cancel(fixture.app.state::<DownloadManager>());
    assert!(cancel.load(Ordering::Relaxed));
    run_plan(
        fixture.app.handle(),
        &fixture.settings,
        "apply",
        true,
        &cancel,
        fixture.plan(true),
    )
    .unwrap();
    assert_eq!(fixture.status().phase, "cancelled");
    assert!(!fixture.dir.join(EXE).exists());
    fixture.run("apply", false, true).unwrap();
    assert_eq!(fixture.status().phase, "ready");
}

#[test]
fn commands_reject_overlapping_tasks_and_running_games() {
    let fixture = Fixture::new(Distribution::Cn, true);
    assert!(game_download_start(
        fixture.app.handle().clone(),
        fixture.app.state(),
        fixture.app.state(),
        "invalid".into(),
        false
    )
    .is_err());
    fixture
        .app
        .state::<AppState>()
        .game
        .lock()
        .unwrap()
        .starting = true;
    assert!(game_download_start(
        fixture.app.handle().clone(),
        fixture.app.state(),
        fixture.app.state(),
        "apply".into(),
        false
    )
    .unwrap_err()
    .contains("游戏正在运行"));
    fixture
        .app
        .state::<AppState>()
        .game
        .lock()
        .unwrap()
        .starting = false;
    // UMU 进程组已消失时, 后台会话仍阻止资源写入.
    let background = crate::native_session::NativeSession::prepare(
        &fixture.dir.join("logs"),
        &fixture.dir.join("native.log"),
        nte_core::launch::background::ClickProfile {
            width: 1280,
            height: 720,
            x: 1120,
            y: 636,
        },
    )
    .unwrap();
    fixture.app.state::<AppState>().game.lock().unwrap().session = Some(crate::GameSession {
        id: 1,
        pid: 0,
        pgid: 0,
        exe: EXE.into(),
        started_at: 0,
        log: String::new(),
        closing: false,
        background: Some(background.clone()),
    });
    assert!(game_download_start(
        fixture.app.handle().clone(),
        fixture.app.state(),
        fixture.app.state(),
        "apply".into(),
        false,
    )
    .unwrap_err()
    .contains("游戏正在运行"));
    background.close().unwrap();
    fixture.app.state::<AppState>().game.lock().unwrap().session = None;
    publish(fixture.app.handle(), |status| status.phase = "game".into());
    assert!(game_download_start(
        fixture.app.handle().clone(),
        fixture.app.state(),
        fixture.app.state(),
        "check".into(),
        false
    )
    .unwrap_err()
    .contains("任务正在进行"));
}

#[test]
fn full_check_requirement_is_scoped_to_the_installation() {
    let fixture = Fixture::new(Distribution::Cn, true);
    publish(fixture.app.handle(), |status| {
        status.phase = "needs-update".into();
        status.full = true;
        status.install_root = "/another-installation".into();
    });
    let (_, _, full) = prepare_task(
        &fixture.app.state::<AppState>(),
        &fixture.app.state::<DownloadManager>(),
        "check",
        false,
        &Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert!(!full);
}

#[test]
fn emitted_stage_and_current_file_always_refer_to_the_same_item() {
    let fixture = Fixture::new(Distribution::Cn, false);
    let seen = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let observed = seen.clone();
    fixture.app.listen_any("game-download-state", move |event| {
        observed
            .lock()
            .unwrap()
            .push(serde_json::from_str(event.payload()).unwrap());
    });
    fixture.run("apply", false, true).unwrap();
    let events = seen.lock().unwrap();
    assert!(events.iter().any(|event| event["stage"] == "verify"));
    for event in events.iter().filter(|event| {
        event["phase"] == "game" && event["stage"] != "" && event["currentFile"] != ""
    }) {
        let path = event["currentFile"].as_str().unwrap();
        let size = event["currentFileBytes"].as_u64().unwrap();
        assert_eq!(
            size,
            if path == DATA {
                fixture.data.len() as u64
            } else {
                b"fixture game executable".len() as u64
            }
        );
    }
}

#[test]
fn read_only_checks_do_not_block_launch_or_settings() {
    let fixture = Fixture::new(Distribution::Cn, true);
    let cancel = Arc::new(AtomicBool::new(false));
    let (status, _, _) = prepare_task(
        &fixture.app.state::<AppState>(),
        &fixture.app.state::<DownloadManager>(),
        "check",
        true,
        &cancel,
    )
    .unwrap();
    assert!(busy(&status.phase));
    assert!(!blocks_changes(&status));
    let mut applying = status;
    applying.mode = "apply".into();
    assert!(blocks_changes(&applying));
    game_download_cancel(fixture.app.state());
    assert!(cancel.load(Ordering::Relaxed));
}

#[test]
fn byte_events_are_throttled_without_losing_progress() {
    let fixture = Fixture::new(Distribution::Cn, true);
    let seen = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let observed = seen.clone();
    fixture.app.listen_any("game-download-state", move |event| {
        observed
            .lock()
            .unwrap()
            .push(serde_json::from_str(event.payload()).unwrap());
    });
    for _ in 0..100 {
        publish_bytes(fixture.app.handle(), 1024);
    }
    assert_eq!(fixture.status().downloaded_bytes, 100 * 1024);
    assert_eq!(seen.lock().unwrap().len(), 1);
    fixture
        .app
        .state::<DownloadManager>()
        .inner
        .lock()
        .unwrap()
        .last_progress = Some(Instant::now() - Duration::from_millis(150));
    publish_bytes(fixture.app.handle(), 1024);
    assert_eq!(seen.lock().unwrap().len(), 2);
    publish(fixture.app.handle(), |status| status.phase = "ready".into());
    let events = seen.lock().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events.last().unwrap()["downloadedBytes"], 101 * 1024);
    assert_eq!(events.last().unwrap()["phase"], "ready");
}

#[test]
fn start_command_publishes_a_fast_worker_failure() {
    let fixture = Fixture::new(Distribution::Cn, true);
    fixture.dir.write(
        "NTEGlobal/Config/Config.ini",
        b"[General]\nGameID=3000001\n",
    );
    let (send, receive) = std::sync::mpsc::channel();
    fixture.app.listen_any("game-download-state", move |event| {
        let status: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
        if status["phase"] == "failed" {
            send.send(status).unwrap();
        }
    });
    let initial = game_download_start(
        fixture.app.handle().clone(),
        fixture.app.state(),
        fixture.app.state(),
        "check".into(),
        false,
    )
    .unwrap();
    assert_eq!(initial.phase, "checking");
    let failed = receive.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(failed["error"].as_str().unwrap().contains("另一地区"));
    assert_eq!(fixture.status().phase, "failed");
}

#[test]
fn automatic_check_uses_detection_without_persisting_it() {
    let mut env = crate::tests::EnvGuard::new();
    let fixture = Fixture::new(Distribution::Cn, true);
    env.set("NTE_ROOT", fixture.dir.path());
    fixture
        .app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .install_root = None;
    let (status, effective, _) = prepare_task(
        &fixture.app.state::<AppState>(),
        &fixture.app.state::<DownloadManager>(),
        "check",
        false,
        &Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(
        status.install_root,
        fixture.dir.path().display().to_string()
    );
    assert_eq!(effective.install_root, Some(status.install_root));
    assert!(fixture
        .app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .install_root
        .is_none());
}
