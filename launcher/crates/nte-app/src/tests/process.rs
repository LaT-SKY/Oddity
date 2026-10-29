use super::EnvGuard;
use crate::*;
use std::time::Instant;

mod playtime_export {
    use crate::playtime_commands::playtime_export_poster;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use std::fs;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\noddity";

    fn export(path: &std::path::Path, data: String) -> Result<String, String> {
        tauri::async_runtime::block_on(playtime_export_poster(
            path.display().to_string(),
            data,
        ))
    }

    #[test]
    fn replaces_existing_poster_and_leaves_no_temporary_files() {
        let dir = super::super::TempDir::new("playtime-export-replace");
        let path = dir.write("poster.png", b"old image");
        assert_eq!(
            export(&path, STANDARD.encode(PNG)).unwrap(),
            path.display().to_string()
        );
        assert_eq!(fs::read(path).unwrap(), PNG);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn normalizes_extensions_and_preserves_uppercase_png() {
        let dir = super::super::TempDir::new("playtime-export-extensions");
        for (requested, expected) in [
            ("poster", "poster.png"),
            ("poster.jpg", "poster.png"),
            ("poster.PNG", "poster.PNG"),
        ] {
            let path = dir.join(expected);
            assert_eq!(
                export(&dir.join(requested), STANDARD.encode(PNG)).unwrap(),
                path.display().to_string()
            );
            assert_eq!(fs::read(path).unwrap(), PNG);
        }
    }

    #[test]
    fn invalid_data_cannot_overwrite_an_existing_poster() {
        let dir = super::super::TempDir::new("playtime-export-invalid");
        let path = dir.write("poster.png", PNG);
        for data in [
            String::new(),
            "invalid base64!".into(),
            STANDARD.encode(b"not png"),
            STANDARD.encode(&PNG[..7]),
        ] {
            assert!(export(&path, data).is_err());
            assert_eq!(fs::read(&path).unwrap(), PNG);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn rejects_directory_created_by_extension_normalization() {
        let dir = super::super::TempDir::new("playtime-export-normalized-dir");
        let occupied = dir.join("poster.png");
        fs::create_dir(&occupied).unwrap();
        let error = export(&dir.join("poster.jpg"), STANDARD.encode(PNG)).unwrap_err();
        assert!(error.contains("文件夹"));
        assert!(occupied.is_dir());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn rejects_a_file_as_the_parent_directory() {
        let dir = super::super::TempDir::new("playtime-export-parent-file");
        let parent = dir.write("parent", b"keep");
        let error = export(&parent.join("poster.png"), STANDARD.encode(PNG)).unwrap_err();
        assert!(error.contains("文件夹不存在"));
        assert_eq!(fs::read(parent).unwrap(), b"keep");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn enforces_the_decoded_size_limit() {
        let dir = super::super::TempDir::new("playtime-export-size");
        let path = dir.join("poster.png");
        let limit = 20 * 1024 * 1024;
        let mut bytes = vec![0; limit];
        bytes[..8].copy_from_slice(&PNG[..8]);
        export(&path, STANDARD.encode(&bytes)).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), limit as u64);
        bytes.push(0);
        let error = export(&path, STANDARD.encode(&bytes)).unwrap_err();
        assert!(error.contains("大小上限"));
        assert_eq!(fs::metadata(path).unwrap().len(), limit as u64);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

#[test]
fn game_mode_status_reports_capabilities_and_releases_the_finished_session() {
    use nte_core::game_mode::{Effect, Options, Session};

    let mut env = EnvGuard::new();
    let dir = super::TempDir::new("game-mode-status");
    env.set(
        "DBUS_SESSION_BUS_ADDRESS",
        "unix:path=/nonexistent/oddity-test-bus",
    );
    env.set("PATH", dir.path());
    super::write_script(dir.path(), "mangohud", "#!/bin/sh\nexit 0\n");
    let app = tauri::test::mock_app();
    app.manage(AppState {
        settings: Mutex::new(Settings::default()),
        game: Mutex::new(GameTracker::default()),
    });
    let idle = tauri::async_runtime::block_on(game_mode_status(app.state())).unwrap();
    assert!(idle.session.is_none());
    assert_eq!(idle.capabilities.len(), 5);
    assert!(!idle.capabilities[0].available);
    assert!(idle.capabilities[3].available);

    let mut command = Command::new("true");
    let mode = Arc::new(Mutex::new(Session::start(
        &Options {
            overlay: true,
            ..Default::default()
        },
        &mut command,
    )));
    assert!(command
        .get_envs()
        .any(|(key, value)| key == "MANGOHUD" && value == Some(std::ffi::OsStr::new("1"))));
    let session = GameSession {
        id: 1,
        pid: 0,
        pgid: 0,
        exe: "game.exe".into(),
        started_at: 0,
        log: String::new(),
        mode: "background".into(),
        distribution: Distribution::Cn,
        closing: false,
        exit_notified: false,
        game_alive: Arc::new(AtomicBool::new(false)),
        _install_lock: None,
        _prefix_lock: None,
        background: None,
        game_mode: Some(mode.clone()),
    };
    app.state::<AppState>().game.lock().unwrap().session = Some(session.clone());
    let running = tauri::async_runtime::block_on(game_mode_status(app.state())).unwrap();
    assert_eq!(running.session.unwrap()[3].state, Effect::Requested);
    mode.lock().unwrap().release();
    let finished = tauri::async_runtime::block_on(game_mode_status(app.state())).unwrap();
    assert_eq!(finished.session.unwrap()[3].state, Effect::Finished);
}

#[test]
fn now_millis_is_a_wall_clock_timestamp() {
    assert!(now_millis() > 1_600_000_000_000);
}

#[test]
fn game_status_reflects_the_session() {
    let idle = GameStatus::idle();
    assert!(!idle.running);
    assert!(!idle.closing);
    assert!(idle.pid.is_none());
    assert!(idle.started_at.is_none());
    assert!(idle.exe.is_none());
    assert!(idle.log.is_none());

    assert!(!GameStatus::from_session(None).running);

    let session = GameSession {
        id: 7,
        pid: 1234,
        pgid: 1234,
        exe: "/game/HTGame.exe".to_string(),
        started_at: 99,
        log: "/tmp/launch.log".to_string(),
        mode: "official".into(),
        distribution: Distribution::Global,
        closing: true,
        exit_notified: false,
        game_alive: Arc::new(AtomicBool::new(false)),
        _install_lock: None,
        _prefix_lock: None,
        game_mode: None,
        background: None,
    };
    let status = GameStatus::from_session(Some(&session));
    assert!(status.running);
    assert!(status.closing);
    assert_eq!(status.pid, Some(1234));
    assert_eq!(status.started_at, Some(99));
    assert_eq!(status.exe.as_deref(), Some("/game/HTGame.exe"));
    assert_eq!(status.log.as_deref(), Some("/tmp/launch.log"));
}

fn wait_until(mut condition: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    condition()
}

#[test]
fn matches_only_the_game_executable() {
    assert!(is_game_process_name("HTGame.exe"));
    assert!(is_game_process_name("htgame"));
    assert!(is_game_process_name(r"Z:\games\HTGame.exe"));
    assert!(is_game_process_name("GameThread"));
    assert!(!is_game_process_name("NTEGame.exe"));
    assert!(!is_game_process_name("exec -a HTGame.exe sleep 30"));
}

#[test]
fn rejects_non_process_groups() {
    assert!(!process_group_alive(0));
    assert!(!process_group_alive(-1));
    assert_eq!(signal_process_group(0, libc::SIGTERM), Ok(()));
    assert_eq!(signal_process_group(-1, libc::SIGTERM), Ok(()));
    terminate_process_group(0).expect("nothing to terminate");
    assert!(!game_main_process_alive(0));
}

#[test]
fn detects_the_game_main_process() {
    let _env = EnvGuard::new();
    let mut child = Command::new("bash")
        .arg("-c")
        .arg("exec -a HTGame.exe sleep 30")
        .process_group(0)
        .spawn()
        .expect("spawn game process");
    let root = child.id();
    assert!(wait_until(
        || game_main_process_alive(root),
        Duration::from_secs(2)
    ));

    let pgid = i32::try_from(root).expect("pid fits i32");
    let waiter = thread::spawn(move || child.wait());
    terminate_process_group(pgid).expect("terminate game process");
    let _ = waiter.join().expect("join waiter").expect("wait for child");
    assert!(!game_main_process_alive(root));
}

#[test]
fn ignores_wine_cleanup_processes() {
    let _env = EnvGuard::new();
    // 游戏主进程结束后 shell 和收尾进程仍存活, 用来模拟 Wine 清理阶段。
    let mut child = Command::new("bash")
        .arg("-c")
        .arg(
            "exec -a HTGame.exe sleep 30 & game=$!; \
             sleep 0.5; kill \"$game\"; wait \"$game\"; sleep 30",
        )
        .process_group(0)
        .spawn()
        .expect("spawn fake wine tree");
    let root = child.id();
    assert!(
        wait_until(|| game_main_process_alive(root), Duration::from_secs(2)),
        "游戏主进程应在清理进程前被识别"
    );
    assert!(
        wait_until(|| !game_main_process_alive(root), Duration::from_secs(3)),
        "游戏主进程退出后不应继续等待收尾进程"
    );
    // 收尾进程仍在, 但不能再被当作游戏运行中。
    let pgid = i32::try_from(root).expect("pid fits i32");
    assert!(process_group_alive(pgid));

    let waiter = thread::spawn(move || child.wait());
    terminate_process_group(pgid).expect("terminate fake wine tree");
    let _ = waiter.join().expect("join waiter");
}

#[test]
fn signals_and_terminates_a_live_process_group() {
    // PATH 会被别的用例改写，启动外部命令前先拿到环境锁
    let _env = EnvGuard::new();
    let mut child = Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("spawn sleep");
    let pgid = i32::try_from(child.id()).expect("pid fits i32");
    assert!(process_group_alive(pgid));

    signal_process_group(pgid, libc::SIGTERM).expect("signal group");
    let _ = child.wait();

    // 组长被回收后进程组也不该再被认为存活
    let deadline = Instant::now() + Duration::from_secs(2);
    while process_group_alive(pgid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!process_group_alive(pgid));
    // 已经消失的进程组再发信号仍然算成功
    signal_process_group(pgid, libc::SIGTERM).expect("signal dead group");
    terminate_process_group(pgid).expect("terminate dead group");
}

#[test]
fn terminates_a_process_group_and_lets_the_leader_reap() {
    // PATH 会被别的用例改写，启动外部命令前先拿到环境锁
    let _env = EnvGuard::new();
    let mut child = Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("spawn sleep");
    let pgid = i32::try_from(child.id()).expect("pid fits i32");
    assert!(process_group_alive(pgid));

    // 应用里由监控线程回收组长；没有并发的 wait，组长会一直是僵尸进程，
    // 整个进程组也就永远不会消失。
    let waiter = thread::spawn(move || child.wait());
    terminate_process_group(pgid).expect("terminate group");
    let _ = waiter.join().expect("join waiter").expect("wait for child");

    let deadline = Instant::now() + Duration::from_secs(2);
    while process_group_alive(pgid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!process_group_alive(pgid));
}

#[test]
fn terminates_a_group_with_members_beyond_the_leader() {
    // PATH 会被别的用例改写，启动外部命令前先拿到环境锁
    let _env = EnvGuard::new();
    // 用 shell 起一个后台子进程：SIGTERM 只发给组长时组内还有成员
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("sleep 30 & wait")
        .process_group(0)
        .spawn()
        .expect("spawn shell");
    let pgid = i32::try_from(child.id()).expect("pid fits i32");
    let waiter = thread::spawn(move || child.wait());
    terminate_process_group(pgid).expect("terminate group");
    let _ = waiter.join().expect("join waiter");

    let deadline = Instant::now() + Duration::from_secs(2);
    while process_group_alive(pgid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!process_group_alive(pgid));
}

#[test]
fn reparented_game_remains_tracked_until_it_exits() {
    let _env = EnvGuard::new();
    let dir = super::TempDir::new("reparented-game");
    let mut child = Command::new("bash")
        .args([
            "-c",
            r#"
        bash -c 'bash -c "exec -a HTGame.exe sleep 30" & echo $! > "$1/game.pid"; wait' _ "$1" &
        echo $! > "$1/middle.pid"
        wait
        sleep 30
    "#,
            "probe",
        ])
        .arg(dir.path())
        .process_group(0)
        .spawn()
        .unwrap();
    let root = child.id();
    let mut watch = GameProcessWatch::new(root);
    assert!(wait_until(
        || watch.alive() && dir.join("middle.pid").is_file() && dir.join("game.pid").is_file(),
        Duration::from_secs(3)
    ));
    let middle: i32 = fs::read_to_string(dir.join("middle.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let game: u32 = fs::read_to_string(dir.join("game.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    unsafe {
        libc::kill(middle, libc::SIGTERM);
    }
    assert!(wait_until(
        || process_stat(game).is_some_and(|stat| stat.parent != middle as u32),
        Duration::from_secs(3)
    ));
    assert!(watch.alive());
    assert!(game_main_process_alive(root));
    let waiter = thread::spawn(move || child.wait());
    terminate_process_group(root as i32).unwrap();
    waiter.join().unwrap().unwrap();
    assert!(!watch.alive());
}

#[test]
fn tracks_playtime_while_the_game_process_is_alive() {
    use crate::playtime_commands::Playtime;
    use tauri::Manager;

    let _env = EnvGuard::new();
    let dir = super::TempDir::new("playtime-process");
    let app = tauri::test::mock_app();
    app.manage(Playtime::load(dir.path()));
    app.manage(AppState {
        settings: Mutex::new(Settings::default()),
        game: Mutex::new(GameTracker::default()),
    });
    let mut child = Command::new("bash")
        .arg("-c")
        .arg("exec -a HTGame.exe sleep 30")
        .process_group(0)
        .spawn()
        .expect("spawn game process");
    let root = child.id();
    let pgid = i32::try_from(root).expect("pid fits i32");
    let session = GameSession {
        id: 1,
        pid: root,
        pgid,
        exe: "HTGame.exe".into(),
        started_at: 0,
        log: String::new(),
        mode: "background".into(),
        distribution: Distribution::Cn,
        closing: false,
        exit_notified: false,
        game_alive: Arc::new(AtomicBool::new(true)),
        _install_lock: None,
        _prefix_lock: None,
        background: None,
        game_mode: None,
    };
    let done = Arc::new(AtomicBool::new(false));
    let alive = Arc::new(AtomicBool::new(true));
    watch_game_exit(
        app.handle().clone(),
        session,
        Arc::clone(&done),
        Arc::clone(&alive),
    );
    assert!(
        wait_until(
            || app.state::<Playtime>().history().unwrap().active.is_some(),
            Duration::from_secs(3)
        ),
        "游戏进程出现后应开始计时"
    );
    let waiter = thread::spawn(move || child.wait());
    terminate_process_group(pgid).expect("terminate game process");
    let _ = waiter.join().expect("join game process");
    assert!(
        wait_until(
            || app.state::<Playtime>().history().unwrap().active.is_none(),
            Duration::from_secs(3)
        ),
        "游戏进程退出后应结束计时"
    );
    assert!(
        wait_until(|| !alive.load(Ordering::Acquire), Duration::from_secs(3)),
        "游戏进程退出后应清除存活状态"
    );
    done.store(true, Ordering::Release);
}
