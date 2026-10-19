use super::*;
use crate::*;
use std::time::Instant;

/// 一次完整的启动环境：安装目录、prefix、Proton、假 umu-run。
struct LaunchFixture {
    dir: TempDir,
    launcher: PathBuf,
    prefix: PathBuf,
    proton: PathBuf,
    bridge: PathBuf,
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

        let prefix = dir.join("prefix");
        std::fs::create_dir_all(prefix.join("drive_c")).expect("create prefix");

        let proton = fake_proton(&dir.join("tools"), "GE-Proton9-1");
        std::fs::create_dir_all(proton.join("files/bin")).expect("create wine dir");
        let wine_log = dir.join("wine.log");
        write_script(
            &proton.join("files/bin"),
            "wine",
            &format!("#!/bin/sh\necho \"$@\" > {}\nexit 0\n", wine_log.display()),
        );

        let bridge = dir.join("winroot");
        std::fs::create_dir_all(bridge.join("NTELauncher")).expect("create bridge dir");
        dir.write("winroot/NTELauncher/NTEGame.exe", b"MZ");

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
            bridge,
            dump,
            wine_log,
            path,
            env,
        }
    }

    fn settings(&self, mode: &str) -> Settings {
        Settings {
            install_root: Some(self.launcher.display().to_string()),
            bridge_root: Some(self.bridge.display().to_string()),
            prefix: Some(self.prefix.display().to_string()),
            proton_path: Some(self.proton.display().to_string()),
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
        "winroot/NTELauncher/UserData/Config/Config.ini",
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
    assert!(args.ends_with("/launcher /directly /autoplay"), "{args}");

    // 官方启动器会被改成“随游戏退出”的薄壳
    let presence = std::fs::read_to_string(
        fixture
            .bridge
            .join("NTELauncher/UserData/Config/Config.ini"),
    )
    .expect("read presence config");
    assert!(presence.contains("user_t92=deadbeef"), "{presence}");
    assert!(presence.contains("autoUpdate=0"), "{presence}");
    assert!(presence.contains("quitWithGame=1"), "{presence}");
    assert!(presence.contains("NeedUpdate=0"), "{presence}");

    // Wine 托盘图标被隐藏
    let wine = std::fs::read_to_string(&fixture.wine_log).expect("read wine log");
    assert!(wine.contains("NoTrayItemsDisplay"), "{wine}");

    fixture.stop(invocation);
}

#[test]
fn launch_background_mode_uses_the_window_helper() {
    let fixture = LaunchFixture::new("launch-background");
    fixture.dir.write_text(
        "winroot/NTELauncher/UserData/Config/Config.ini",
        "[Setting]\nautoLogin=1\n",
    );

    let invocation = launch_impl(&fixture.settings("background")).expect("launch");
    assert_eq!(invocation.result.mode, "background");
    assert!(invocation.result.exe.ends_with("NTEGame.exe"));

    let dump = fixture.dump();
    assert_eq!(
        env_value(&dump, "NTE_BRIDGE_EXE"),
        Some(windows_path(&fixture.bridge.join("NTELauncher/NTEGame.exe")).as_str())
    );
    assert_eq!(
        env_value(&dump, "NTE_BRIDGE_CWD"),
        Some(windows_path(&fixture.bridge.join("NTELauncher")).as_str())
    );
    assert_eq!(
        env_value(&dump, "NTE_BRIDGE_ARGS"),
        Some("/launcher /directly /autoplay")
    );
    assert!(env_value(&dump, "NTE_HELPER_LOG")
        .expect("helper log path")
        .ends_with("helper-window.log"));
    // 直接跑的是内置窗口助手，而不是官方 exe
    assert!(env_value(&dump, "NTE_GAME_EXE").is_none());
    let args = dump
        .lines()
        .find_map(|line| line.strip_prefix("args: "))
        .expect("umu args");
    assert!(args.contains("nte-launch-helper.exe"), "{args}");

    fixture.stop(invocation);
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
    let config = fixture
        .bridge
        .join("NTELauncher/UserData/Config/Config.ini");
    std::fs::create_dir_all(config.parent().expect("config parent")).expect("create config dir");
    std::fs::write(&config, "[Setting]\nautoLogin=1\n").expect("write config");
    fixture.env.set("NTE_DISABLE_PRESENCE_TUNING", "1");

    let invocation = launch_impl(&fixture.settings("background")).expect("launch");
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

    let error = expect_launch_error(launch_impl(&fixture.settings("background")));
    assert!(
        error.contains("官方后台桥接启动失败（退出码 1）"),
        "{error}"
    );
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

    // 找不到 Proton
    let without_proton = Settings {
        proton_path: Some("/nope".to_string()),
        ..settings.clone()
    };
    let error = expect_launch_error(launch_impl(&without_proton));
    assert!(error.contains("未找到可用的 Proton"), "{error}");

    // prefix 还没初始化
    let raw_prefix = Settings {
        prefix: Some(fixture.dir.join("raw-prefix").display().to_string()),
        ..settings.clone()
    };
    let error = expect_launch_error(launch_impl(&raw_prefix));
    assert!(error.contains("Wine prefix 尚未初始化"), "{error}");

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
        bridge_root: None,
        ..settings.clone()
    };
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
