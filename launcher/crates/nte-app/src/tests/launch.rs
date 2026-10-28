use super::*;
use crate::*;
use std::time::Instant;

#[test]
fn gamescope_preserves_the_umu_handoff_and_environment() {
    let fixture = LaunchFixture::new("gamescope-launch");
    let options_log = fixture.dir.join("gamescope-args");
    write_script(&fixture.dir.join("bin"), "gamescope", &format!(
        "#!/bin/sh\nwhile [ \"$1\" != -- ]; do printf '%s\\n' \"$1\" >> '{}'; shift; done\nshift\nexec \"$@\"\n", options_log.display()
    ));
    let mut settings = fixture.settings("direct");
    settings.game_mode.gamescope = nte_core::game_mode::gamescope::Options {
        enabled: true,
        game_width: 1280,
        game_height: 720,
        fps: 60,
        ..Default::default()
    };
    let invocation = launch_impl(&settings).unwrap();
    let dump = fixture.dump();
    assert!(dump.contains("HTGame.exe") && dump.contains("WINEPREFIX="));
    let options = std::fs::read_to_string(options_log).unwrap();
    assert!(options.contains("-w\n1280\n-h\n720\n-r\n60\n"));
    fixture.stop(invocation);
}

/// 一次完整的启动环境：安装目录、prefix、Proton、假 umu-run。
struct LaunchFixture {
    dir: TempDir,
    launcher: PathBuf,
    prefix: PathBuf,
    proton: PathBuf,
    profile: nte_core::environment::RuntimeEnvironment,
    dump: PathBuf,
    wine_log: PathBuf,
    path: String,
    env: EnvGuard,
}

impl LaunchFixture {
    fn new(tag: &str) -> Self {
        let dir = TempDir::new(tag);
        let mut env = EnvGuard::new();
        env.set("HOME", dir.join("home"));
        env.set("XDG_DATA_HOME", dir.join("data"));
        std::fs::create_dir_all(dir.join("home")).expect("create fake home");
        env.remove("WINEPREFIX");
        env.remove("PROTONPATH");
        env.remove("NTE_ROOT");
        env.remove("NTE_INSTALL");
        env.remove("STEAM_COMPAT_CLIENT_INSTALL_PATH");
        env.remove("NTE_DISABLE_PRESENCE_TUNING");

        let launcher = dir.join("install/NTELauncher");
        std::fs::create_dir_all(&launcher).expect("create launcher dir");
        dir.write_text(
            "install/NTELauncher/Config/Config.ini",
            "[VERSION]\nVersion=1.4.16\n",
        );
        dir.write(
            "install/Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe",
            b"MZ",
        );
        dir.write("install/NTELauncher/NTEGame.exe", b"MZ");

        let proton = fake_proton(&dir.join("tools"), "GE-Proton9-1");
        let profile = managed_environment(&proton, &dir.join("install"), Distribution::Cn);
        let prefix = profile.prefix(&data_dir()).unwrap();
        std::fs::create_dir_all(proton.join("files/bin")).expect("create wine dir");
        let wine_log = dir.join("wine.log");
        write_script(
            &proton.join("files/bin"),
            "wine",
            &format!("#!/bin/sh\necho \"$@\" > {}\nexit 0\n", wine_log.display()),
        );

        let dump = dir.join("umu-dump.txt");
        let script = format!(
            "#!/bin/sh\n{{ echo \"args: $@\"; echo \"cwd: $(pwd)\"; env; }} > {dump}.tmp\nmv {dump}.tmp {dump}\nsleep 10\n",
            dump = dump.display()
        );
        let bin = dir.join("bin");
        write_script(&bin, "umu-run", &script);
        // 假脚本自己也要能找到 env、mv 这些系统命令
        let inherited = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{inherited}", bin.display());
        env.set("PATH", &path);

        Self {
            dir,
            launcher,
            prefix,
            proton,
            profile,
            dump,
            wine_log,
            path,
            env,
        }
    }

    fn settings(&self, mode: &str) -> Settings {
        Settings {
            install_root: Some(self.launcher.display().to_string()),
            runtime_environments: vec![self.profile.clone()],
            active_environment: Some(self.profile.id.clone()),
            launch_mode: mode.to_string(),
            ..Default::default()
        }
    }

    /// 等假 umu-run 把参数与环境变量写出来。
    fn dump(&self) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.dump.is_file() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        std::fs::read_to_string(&self.dump).expect("read umu dump")
    }

    /// 和应用的监控线程一样：先有人回收组长，进程组才可能真正消失。
    fn stop(&self, invocation: LaunchInvocation) {
        let mut child = invocation.child;
        let waiter = thread::spawn(move || child.wait());
        terminate_process_group(invocation.pgid).expect("terminate launched group");
        let _ = waiter.join();
    }
}

/// 期待启动失败；万一真的起来了，先收掉进程再报错。
fn expect_launch_error(result: Result<LaunchInvocation, String>) -> String {
    match result {
        Ok(invocation) => {
            let _ = terminate_process_group(invocation.pgid);
            panic!("expected the launch to fail");
        }
        Err(error) => error,
    }
}

/// 从 `env` 输出里取一个变量的值。
fn env_value<'a>(dump: &'a str, key: &str) -> Option<&'a str> {
    dump.lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
}

#[test]
fn launch_official_mode_sets_the_proton_environment() {
    let fixture = LaunchFixture::new("launch-official");
    fixture.dir.write_text(
        "install/NTELauncher/UserData/Config/Config.ini",
        "[Setting]\r\nautoLogin=1\r\n[User]\r\nuser_t92=deadbeef\r\n",
    );

    let invocation = launch_impl(&fixture.settings("official")).expect("launch");
    assert_eq!(invocation.result.mode, "official");
    assert_eq!(invocation.result.pid, invocation.pgid as u32);
    assert!(invocation.result.exe.ends_with("NTEGame.exe"));
    assert_eq!(
        invocation.result.prefix,
        fixture.prefix.display().to_string()
    );
    assert!(invocation.result.started_at > 1_600_000_000_000);
    assert!(Path::new(&invocation.result.log).is_file());
    assert!(invocation.result.log.ends_with("nte-app-launch.log"));
    let dump = fixture.dump();
    assert_eq!(
        env_value(&dump, "WINEPREFIX"),
        Some(fixture.prefix.display().to_string().as_str())
    );
    assert_eq!(
        env_value(&dump, "PROTONPATH"),
        Some(fixture.proton.display().to_string().as_str())
    );
    assert_eq!(env_value(&dump, "GAMEID"), Some("0"));
    assert_eq!(env_value(&dump, "LauncherStarted"), Some("1"));
    assert_eq!(env_value(&dump, "pgp_scale_value"), Some("1"));
    assert_eq!(env_value(&dump, "UMU_LOG"), Some("info"));
    assert_eq!(env_value(&dump, "PROTON_LOG"), Some("0"));
    assert_eq!(env_value(&dump, "PROTON_VERB"), Some("run"));
    // Proton 的 Wayland 开关必须被显式移除
    assert!(env_value(&dump, "PROTON_ENABLE_WAYLAND").is_none());
    assert_eq!(
        dump.lines()
            .find_map(|line| line.strip_prefix("cwd: "))
            .expect("umu cwd"),
        fixture.dir.join("install").display().to_string()
    );
    let args = dump
        .lines()
        .find_map(|line| line.strip_prefix("args: "))
        .expect("umu args");
    assert!(args.contains("NTEGame.exe"), "{args}");
    assert!(args.ends_with("/launcher /directly"), "{args}");

    // 官方启动器会被改成“随游戏退出”的薄壳
    let presence = std::fs::read_to_string(
        fixture
            .dir
            .join("install/NTELauncher/UserData/Config/Config.ini"),
    )
    .expect("read presence config");
    assert!(presence.contains("user_t92=deadbeef"), "{presence}");
    assert!(presence.contains("autoUpdate=1"), "{presence}");
    assert!(presence.contains("quitWithGame=1"), "{presence}");
    assert!(presence.contains("NeedUpdate=0"), "{presence}");

    // Wine 托盘图标被隐藏
    let wine = std::fs::read_to_string(&fixture.wine_log).expect("read wine log");
    assert!(wine.contains("NoTrayItemsDisplay"), "{wine}");

    assert!(nte_core::install_lock::InstallLock::acquire(&fixture.dir.join("install")).is_err());
    fixture.stop(invocation);
    assert!(nte_core::install_lock::InstallLock::acquire(&fixture.dir.join("install")).is_ok());
}

#[test]
fn managed_environment_launch_uses_its_prefix_proton_and_log_directory() {
    let mut fixture = LaunchFixture::new("launch-managed-environment");
    fixture.env.set("NTE_DISABLE_PRESENCE_TUNING", "1");
    let target = fake_proton(fixture.dir.path(), "GE-Proton-New");
    let profile = nte_core::environment::create(
        &data_dir(),
        Some((&fixture.prefix, &fixture.proton)),
        &target,
        &fixture.dir.join("install"),
        Distribution::Cn,
        "新版测试",
    )
    .unwrap();
    let mut settings = fixture.settings("official");
    settings.active_environment = Some(profile.id.clone());
    settings.runtime_environments.push(profile.clone());
    let invocation = launch_impl(&settings).unwrap();
    let prefix = profile.prefix(&data_dir()).unwrap();
    let dump = fixture.dump();
    assert_eq!(env_value(&dump, "WINEPREFIX"), prefix.to_str());
    assert_eq!(env_value(&dump, "PROTONPATH"), target.to_str());
    assert!(invocation
        .result
        .log
        .contains(&format!("logs/environments/{}", profile.id)));
    assert!(nte_core::environment::lock_prefix(&prefix).is_err());
    fixture.stop(invocation);
    assert!(nte_core::environment::lock_prefix(&prefix).is_ok());
    fixture.dir.write("GE-Proton-New/proton", "changed version");
    assert!(expect_launch_error(launch_impl(&settings)).contains("Proton 已变化"));
    settings.active_environment = None;
    assert!(environment_commands::prefix(&settings)
        .unwrap_err()
        .contains("请先"));
}

#[test]
fn launch_background_mode_configures_the_native_session() {
    let fixture = LaunchFixture::new("launch-background");
    fixture.dir.write_text(
        "install/NTELauncher/UserData/Config/Config.ini",
        "[Setting]\nautoLogin=1\n",
    );

    let invocation = launch_impl_with_profile(&fixture.settings("background"), |_| {
        Ok(ClickProfile {
            width: 1280,
            height: 720,
            x: 1120,
            y: 636,
        })
    })
    .expect("launch");
    assert_eq!(invocation.result.mode, "background");
    assert!(invocation.result.exe.ends_with("NTEGame.exe"));
    let dump = fixture.dump();
    assert_eq!(
        env_value(&dump, "NTE_BRIDGE_EXE"),
        Some(windows_path(&fixture.launcher.join("NTEGame.exe")).as_str())
    );
    assert_eq!(
        env_value(&dump, "NTE_BRIDGE_CWD"),
        Some(windows_path(&fixture.launcher).as_str())
    );
    assert_eq!(
        env_value(&dump, "NTE_BRIDGE_ARGS"),
        Some("/launcher /directly")
    );
    assert!(env_value(&dump, "NTE_HELPER_LOG")
        .expect("helper log path")
        .ends_with("helper-window.log"));
    assert_eq!(env_value(&dump, "PROTON_VERB"), Some("run"));
    assert_eq!(
        env_value(&dump, "NTE_SESSION_CLICK"),
        Some("1280,720,1120,636")
    );
    assert!(env_value(&dump, "NTE_SESSION_HELPER")
        .unwrap()
        .ends_with("nte-launch-helper.exe"));
    for key in [
        "NTE_SESSION_OUTPUT",
        "NTE_SESSION_SIGNAL",
        "NTE_SESSION_CANCEL",
    ] {
        assert!(env_value(&dump, key).unwrap().starts_with("Z:"), "{key}");
    }
    // 会话控制器驱动官方正常启动流程.
    assert!(env_value(&dump, "NTE_GAME_EXE").is_none());
    let args = dump
        .lines()
        .find_map(|line| line.strip_prefix("args: "))
        .expect("umu args");
    assert!(args.contains("nte-session-helper.exe"), "{args}");

    fixture.stop(invocation);
}

#[test]
fn launch_reports_a_configured_empty_root_as_not_installed() {
    let fixture = LaunchFixture::new("launch-empty-root");
    let settings = Settings {
        install_root: Some(fixture.dir.join("empty-install").display().to_string()),
        ..fixture.settings("official")
    };

    let error = expect_launch_error(launch_impl(&settings));
    assert!(error.contains("尚未安装启动器文件"), "{error}");
    assert!(!fixture.dump.exists());
}

#[test]
fn launch_reports_unwritable_runtime_config_before_starting_the_game() {
    let fixture = LaunchFixture::new("launch-readonly-config");
    let bin = fixture
        .dir
        .join("install/Client/WindowsNoEditor/HT/Binaries/Win64");
    let config = bin.join("GameLauncher.config");
    std::fs::write(&config, "E:/WindowsLauncher/WPGameClientSDK_64.dll").expect("Windows SDK path");
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o444))
        .expect("readonly config");

    let error = expect_launch_error(launch_impl_with_profile(
        &fixture.settings("background"),
        |_| {
            Ok(ClickProfile {
                width: 1280,
                height: 720,
                x: 1120,
                y: 636,
            })
        },
    ));
    assert!(error.contains("游戏运行配置不可写"), "{error}");
    assert!(error.contains("GameLauncher.config"), "{error}");
    assert!(!fixture.dump.exists());
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644))
        .expect("restore config permissions");

    let cache = bin.join("patchersdk_serverlist");
    std::fs::create_dir(&cache).expect("server cache");
    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o555))
        .expect("readonly server cache");
    let error = expect_launch_error(launch_impl_with_profile(
        &fixture.settings("background"),
        |_| {
            Ok(ClickProfile {
                width: 1280,
                height: 720,
                x: 1120,
                y: 636,
            })
        },
    ));
    assert!(error.contains("patchersdk_serverlist"), "{error}");
    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o755))
        .expect("restore server cache permissions");

    check_game_write_access(&bin.join("HTGame.exe")).expect("writable runtime config");
}

#[test]
fn launch_direct_mode_targets_the_game_executable() {
    let fixture = LaunchFixture::new("launch-direct");
    let game = fixture
        .dir
        .join("install/Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe");
    // 直接模式不碰官方启动器的配置，这里放一个内容不变的文件作为对照
    let config = fixture.launcher.join("UserData/Config/Config.ini");
    std::fs::create_dir_all(config.parent().expect("config parent")).expect("create config dir");
    std::fs::write(&config, "[Setting]\nautoLogin=1\n").expect("write config");

    let invocation = launch_impl(&fixture.settings("direct")).expect("launch");
    assert_eq!(invocation.result.mode, "direct");
    assert!(invocation.result.exe.ends_with("HTGame.exe"));

    let dump = fixture.dump();
    assert_eq!(
        env_value(&dump, "NTE_GAME_EXE"),
        Some(windows_path(&game).as_str())
    );
    assert_eq!(
        env_value(&dump, "NTE_GAME_CWD"),
        Some(windows_path(game.parent().expect("game parent")).as_str())
    );
    assert_eq!(
        env_value(&dump, "NTE_GAME_ARGS"),
        Some("/Game/LoginAndCreate/Map/Updater/Updater_P -SAVEWINPOS=1")
    );
    assert!(env_value(&dump, "NTE_BRIDGE_EXE").is_none());

    // 直接模式不改官方启动器配置，也不去动 Wine 托盘
    assert_eq!(
        std::fs::read_to_string(&config).expect("read config"),
        "[Setting]\nautoLogin=1\n"
    );
    assert!(!fixture.wine_log.exists());

    fixture.stop(invocation);
}

#[test]
fn launch_can_skip_the_presence_tuning() {
    let mut fixture = LaunchFixture::new("launch-no-presence");
    let config = fixture.launcher.join("UserData/Config/Config.ini");
    std::fs::create_dir_all(config.parent().expect("config parent")).expect("create config dir");
    std::fs::write(&config, "[Setting]\nautoLogin=1\n").expect("write config");
    fixture.env.set("NTE_DISABLE_PRESENCE_TUNING", "1");

    let invocation = launch_impl_with_profile(&fixture.settings("background"), |_| {
        Ok(ClickProfile {
            width: 1280,
            height: 720,
            x: 1120,
            y: 636,
        })
    })
    .expect("launch");
    assert_eq!(
        std::fs::read_to_string(&config).expect("read config"),
        "[Setting]\nautoLogin=1\n"
    );
    assert!(!fixture.wine_log.exists());
    fixture.stop(invocation);
}

#[test]
fn launch_reports_an_immediate_exit_with_the_log_tail() {
    let fixture = LaunchFixture::new("launch-exit");
    // 假的 umu-run 立刻失败：应当把日志尾部带回错误信息
    write_script(
        &fixture.dir.join("bin"),
        "umu-run",
        "#!/bin/sh\necho 'wine: cannot find L\"C:\\\\windows\"' >&2\nexit 1\n",
    );

    let error = expect_launch_error(launch_impl(&fixture.settings("official")));
    assert!(error.contains("官方启动器启动失败（退出码 1）"), "{error}");
    assert!(error.contains("cannot find"), "{error}");

    let error = expect_launch_error(launch_impl(&fixture.settings("direct")));
    assert!(error.contains("游戏启动失败（退出码 1）"), "{error}");

    let mut invocation = launch_impl_with_profile(&fixture.settings("background"), |_| {
        Ok(ClickProfile {
            width: 1280,
            height: 720,
            x: 1120,
            y: 636,
        })
    })
    .expect("background monitor owns cleanup");
    let background = invocation.background.as_ref().unwrap();
    let error = background
        .monitor(&mut invocation.child)
        .expect_err("early exit fails the session");
    assert!(error.contains("后台包装进程"), "{error}");
    assert!(background.finished());
}

#[test]
fn launch_requires_the_expected_tools_and_files() {
    let mut fixture = LaunchFixture::new("launch-errors");
    let settings = fixture.settings("official");

    // PATH 里没有 umu-run
    fixture.env.set("PATH", fixture.dir.join("empty-bin"));
    let error = expect_launch_error(launch_impl(&settings));
    assert!(error.contains("PATH 中未找到 umu-run"), "{error}");

    let path = fixture.path.clone();
    fixture.env.set("PATH", path);

    let mut without_proton = settings.clone();
    without_proton.runtime_environments[0].proton.path = fixture.dir.join("missing");
    assert!(expect_launch_error(launch_impl(&without_proton)).contains("Proton"));
    let mut unselected = settings.clone();
    unselected.active_environment = None;
    assert!(expect_launch_error(launch_impl(&unselected)).contains("请先"));

    // 官方启动器目录里没有 NTEGame.exe
    std::fs::create_dir_all(fixture.dir.join("bare-install/NTELauncher")).expect("create dir");
    fixture
        .dir
        .write_text("bare-install/NTELauncher/Config/Config.ini", "[VERSION]\n");
    let bare = Settings {
        install_root: Some(
            fixture
                .dir
                .join("bare-install/NTELauncher")
                .display()
                .to_string(),
        ),
        ..settings.clone()
    };
    let mut bare = bare;
    bare.runtime_environments[0].install_root = fixture.dir.join("bare-install");
    let error = expect_launch_error(launch_impl(&bare));
    assert!(error.contains("官方启动器不存在"), "{error}");

    // 直接模式找不到游戏可执行文件
    let no_game = Settings {
        launch_mode: "direct".to_string(),
        ..bare.clone()
    };
    let error = expect_launch_error(launch_impl(&no_game));
    assert!(error.contains("未找到 HTGame.exe"), "{error}");
}

#[test]
fn launch_rejects_an_unverified_background_before_starting_umu() {
    let fixture = LaunchFixture::new("launch-unknown-background");
    let error = expect_launch_error(launch_impl(&fixture.settings("background")));
    assert!(error.contains("尚未验证后台启动"), "{error}");
    assert!(!fixture.dump.exists());
}

#[test]
fn global_launch_handoff_uses_global_client_for_official_and_background_modes() {
    for mode in ["official", "background"] {
        let fixture = LaunchFixture::new("launch-global");
        let global = fixture.dir.join("install/NTEGlobal");
        std::fs::rename(&fixture.launcher, &global).unwrap();
        std::fs::rename(global.join("NTEGame.exe"), global.join("NTEGlobalGame.exe")).unwrap();
        fixture.dir.write_text(
            "install/NTEGlobal/Config/Config.ini",
            "[General]\nGameID=3000001\n",
        );
        let mut settings = Settings {
            distribution: Distribution::Global,
            global_install_root: Some(global.display().to_string()),
            ..fixture.settings(mode)
        };
        settings.runtime_environments[0].distribution = Distribution::Global;
        // 注入测试坐标仅验证交接参数, 不代表国际服按钮已实机验证.
        let invocation = launch_impl_with_profile(&settings, |path| {
            assert_eq!(path, global.join("NTEGlobalGame.exe"));
            Ok(ClickProfile {
                width: 1280,
                height: 720,
                x: 1120,
                y: 636,
            })
        })
        .expect("global handoff");
        assert!(invocation.result.exe.ends_with("NTEGlobalGame.exe"));
        let dump = fixture.dump();
        if mode == "background" {
            assert_eq!(
                env_value(&dump, "NTE_BRIDGE_EXE"),
                Some(windows_path(&global.join("NTEGlobalGame.exe")).as_str())
            );
        } else {
            assert!(
                dump.contains("NTEGlobalGame.exe /launcher /directly"),
                "{dump}"
            );
        }
        fixture.stop(invocation);
    }
}

/// 显式提供安装、prefix 和 Proton 后, 验证后台交接并关闭本轮游戏.
#[test]
#[ignore = "需要国服安装、Proton 和图形环境; 见后台验证说明"]
fn live_cn_background_launch() {
    live_background_launch(Distribution::Cn, "ODDITY_LIVE_CN_ROOT");
}

#[test]
#[ignore = "需要国际服安装、Proton 和图形环境; 见后台验证说明"]
fn live_global_background_launch() {
    live_background_launch(Distribution::Global, "ODDITY_LIVE_GLOBAL_ROOT");
}

fn live_background_launch(distribution: Distribution, root_key: &str) {
    let read_path = |key| std::env::var(key).unwrap_or_else(|_| panic!("missing {key}"));
    let root = read_path(root_key);
    let settings = Settings {
        distribution,
        install_root: (distribution == Distribution::Cn).then_some(root.clone()),
        global_install_root: (distribution == Distribution::Global).then_some(root),
        runtime_environments: load_settings().runtime_environments,
        active_environment: Some(read_path("ODDITY_LIVE_ENVIRONMENT")),
        ..Default::default()
    };
    let invocation = launch_impl(&settings).expect("launch background");
    let session = invocation.background.clone().expect("background session");
    let monitored = session.clone();
    let monitor = thread::spawn(move || {
        let _installation_lock = invocation.install_lock;
        let mut child = invocation.child;
        monitored.monitor(&mut child)
    });
    let deadline = Instant::now() + Duration::from_secs(150);
    let mut started = false;
    let mut previous = String::new();
    while Instant::now() < deadline {
        let text = std::fs::read_to_string(session.status_path()).unwrap_or_default();
        if text != previous {
            println!("{text}");
            previous = text.clone();
        }
        let status = nte_core::launch::background::SessionStatus::parse(&text);
        if status.game_started {
            started = true;
            // 给外部窗口探针留出检查隐藏状态与登录界面的时间.
            thread::sleep(Duration::from_secs(60));
            break;
        }
        if status.finished() || status.cleanup_incomplete || monitor.is_finished() {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    let close = session.close();
    let monitored = monitor.join().expect("monitor thread");
    println!("close={close:?}, monitor={monitored:?}");
    println!(
        "{}",
        std::fs::read_to_string(session.status_path()).unwrap_or_default()
    );
    assert!(close.is_ok(), "close: {close:?}");
    assert!(started, "official game did not start");
    assert!(monitored.is_ok(), "monitor: {monitored:?}");
}
