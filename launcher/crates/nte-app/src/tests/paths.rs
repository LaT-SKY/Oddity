use super::*;
use crate::*;

#[test]
fn expands_home_paths_only() {
    let mut env = EnvGuard::new();
    let home = TempDir::new("expand-home");
    env.set("HOME", home.path());
    assert_eq!(expand_path("~"), home.path());
    assert_eq!(expand_path("~/games"), home.join("games"));
    assert_eq!(expand_path("/abs/path"), PathBuf::from("/abs/path"));
    assert_eq!(expand_path("rel/path"), PathBuf::from("rel/path"));
    assert_eq!(expand_path("~other"), PathBuf::from("~other"));
}

#[test]
fn install_seeds_cover_the_known_layouts() {
    let mut seeds = Vec::new();
    install_seeds(Path::new("/mnt/games"), Distribution::Cn, &mut seeds);
    assert!(seeds.contains(&PathBuf::from(
        "/mnt/games/Program Files (x86)/Neverness To Everness/NTELauncher"
    )));
    assert!(seeds.contains(&PathBuf::from("/mnt/games/NTELauncher")));
    seeds.clear();
    install_seeds(Path::new("/mnt/games"), Distribution::Global, &mut seeds);
    assert!(seeds.contains(&PathBuf::from(
        "/mnt/games/Program Files (x86)/Neverness To Everness/NTEGlobal"
    )));
    assert!(seeds.iter().all(|path| path.ends_with("NTEGlobal")));
}

#[test]
fn which_only_accepts_executable_files() {
    let mut env = EnvGuard::new();
    let dir = TempDir::new("which");
    let exe = write_script(dir.path(), "umu-run", "#!/bin/sh\nexit 0\n");
    dir.write("plain-file", b"x");
    std::fs::create_dir_all(dir.join("a-directory")).expect("create directory");

    env.set("PATH", dir.path());
    assert_eq!(which("umu-run"), Some(exe));
    assert_eq!(which("plain-file"), None);
    assert_eq!(which("a-directory"), None);
    assert_eq!(which("missing"), None);

    env.remove("PATH");
    assert_eq!(which("umu-run"), None);
}

#[test]
fn normalizes_launcher_roots() {
    let dir = TempDir::new("root-normalize");
    assert_eq!(
        normalize_launcher_root_for(dir.path(), Distribution::Cn),
        None
    );
    dir.write_text("Config/Config.ini", "[VERSION]\n");
    assert_eq!(
        normalize_launcher_root_for(dir.path(), Distribution::Cn),
        Some(dir.path().to_path_buf())
    );

    // 安装目录形态：<install>/NTELauncher/Config/Config.ini
    let install = TempDir::new("root-normalize-nested");
    install.write_text("NTELauncher/Config/Config.ini", "[VERSION]\n");
    assert_eq!(
        normalize_launcher_root_for(install.path(), Distribution::Cn),
        Some(install.join("NTELauncher"))
    );
}

#[test]
fn configured_root_is_authoritative() {
    let mut env = EnvGuard::new();
    let home = TempDir::new("detect-home");
    let install = TempDir::new("detect-install");
    env.set("HOME", home.path());
    env.remove("XDG_DATA_HOME");
    env.remove("NTE_ROOT");
    env.remove("NTE_INSTALL");
    install.write_text("NTELauncher/Config/Config.ini", "[VERSION]\n");
    let root = install.join("NTELauncher");

    let settings = Settings {
        install_root: Some(install.path().display().to_string()),
        ..Default::default()
    };
    assert_eq!(detect_launcher_root(&settings), Some(root));

    // 配置目录无效时不能改用环境变量里的其他安装.
    let stale = Settings {
        install_root: Some("/nope".to_string()),
        ..Default::default()
    };
    env.set("NTE_ROOT", install.path());
    assert_eq!(detect_launcher_root(&stale), None);
}

#[test]
fn detects_the_launcher_from_environment_and_seeds_without_configuration() {
    let mut env = EnvGuard::new();
    let home = TempDir::new("detect-seeds");
    let install = TempDir::new("detect-seeds-install");
    env.set("HOME", home.path());
    env.remove("XDG_DATA_HOME");
    env.remove("NTE_ROOT");
    env.remove("NTE_INSTALL");
    install.write_text("NTELauncher/Config/Config.ini", "[VERSION]\n");

    env.set("NTE_ROOT", install.path());
    assert_eq!(
        detect_launcher_root(&Settings::default()),
        Some(install.join("NTELauncher"))
    );
    env.remove("NTE_ROOT");

    home.write_text(
        "Games/Neverness To Everness/NTELauncher/Config/Config.ini",
        "[VERSION]\n",
    );
    assert_eq!(
        detect_launcher_root(&Settings::default()),
        Some(home.join("Games/Neverness To Everness/NTELauncher"))
    );
}

#[test]
fn global_install_uses_its_own_root_and_game_id() {
    let install = TempDir::new("detect-global");
    install.write_text("NTELauncher/Config/Config.ini", "[General]\nGameID=1289\n");
    install.write_text("NTEGlobal/Config/Config.ini", "[General]\nGameID=3000001\n");
    let settings = Settings {
        install_root: Some(install.path().display().to_string()),
        global_install_root: Some(install.path().display().to_string()),
        distribution: Distribution::Global,
        ..Default::default()
    };
    assert_eq!(
        detect_launcher_root(&settings),
        Some(install.join("NTEGlobal"))
    );
    assert_eq!(selected_install_root(&settings), install.path());
    install.write_text("NTEGlobal/Config/Config.ini", "[General]\nGameID=1289\n");
    assert_eq!(
        normalize_launcher_root_for(install.path(), Distribution::Global),
        None
    );
}

#[test]
fn detects_the_chinese_home_seed() {
    let mut env = EnvGuard::new();
    let home = TempDir::new("detect-home-cn");
    env.set("HOME", home.path());
    env.remove("XDG_DATA_HOME");
    env.remove("NTE_ROOT");
    env.remove("NTE_INSTALL");
    home.write_text("Games/异环/NTELauncher/Config/Config.ini", "[VERSION]\n");
    assert_eq!(
        detect_launcher_root(&Settings::default()),
        Some(home.join("Games/异环/NTELauncher"))
    );
}

#[test]
fn finds_case_insensitive_components() {
    let dir = TempDir::new("find-ci");
    dir.write("Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe", b"MZ");
    assert_eq!(
        find_ci(
            dir.path(),
            &[
                "client",
                "windowsnoeditor",
                "ht",
                "binaries",
                "win64",
                "htgame.exe"
            ]
        ),
        Some(dir.join("Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe"))
    );
    assert_eq!(find_ci(dir.path(), &["missing"]), None);
    assert_eq!(find_ci(&dir.join("nope"), &["Client"]), None);
}

#[test]
fn locates_the_game_executable_and_the_sdk() {
    let dir = TempDir::new("game-exe");
    dir.write(
        "install/Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe",
        b"MZ",
    );
    dir.write("install/NTELauncher/WPGameClientSDK_64.dll", b"MZ");
    let launcher = dir.join("install/NTELauncher");
    assert_eq!(
        game_exe_for(&launcher),
        Some(dir.join("install/Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe"))
    );
    assert_eq!(
        find_sdk(&launcher),
        Some(launcher.join("WPGameClientSDK_64.dll"))
    );
    // 没有父目录时无法推断安装位置
    assert_eq!(game_exe_for(Path::new("/")), None);
    assert_eq!(find_sdk(&dir.join("empty")), None);
}

#[test]
fn reads_the_game_version_from_user_config() {
    let dir = TempDir::new("game-version");
    let launcher = dir.join("NTELauncher");
    assert_eq!(game_version_for(&launcher), "");

    dir.write_text(
        "NTELauncher/UserData/Config/Config.ini",
        "[Game]\nVersion_=1.4.20\n",
    );
    assert_eq!(game_version_for(&launcher), "1.4.20");

    // 空值等同于没有版本
    dir.write_text(
        "NTELauncher/UserData/Config/Config.ini",
        "[Game]\nVersion_=\n",
    );
    assert_eq!(game_version_for(&launcher), "");
}

#[test]
fn global_auto_detection_honors_environment_and_home_seeds() {
    let mut env = EnvGuard::new();
    let home = TempDir::new("global-detect-home");
    env.set("HOME", home.path());
    env.remove("NTE_ROOT");
    env.remove("NTE_INSTALL");
    let settings = Settings {
        distribution: Distribution::Global,
        ..Default::default()
    };
    home.write_text(
        "Games/Neverness To Everness/NTEGlobal/Config/Config.ini",
        "[General]\nGameID=3000001\n",
    );
    let expected = home.join("Games/Neverness To Everness/NTEGlobal");
    assert_eq!(detect_launcher_root(&settings), Some(expected.clone()));
    home.write_text(
        "custom/NTEGlobal/Config/Config.ini",
        "[General]\nGameID=3000001\n",
    );
    env.set("NTE_ROOT", home.join("custom"));
    assert_eq!(
        detect_launcher_root(&settings),
        Some(home.join("custom/NTEGlobal"))
    );
    home.write_text(
        "cn/NTELauncher/Config/Config.ini",
        "[General]\nGameID=1289\n",
    );
    env.set("NTE_ROOT", home.join("cn/NTELauncher"));
    assert_eq!(detect_launcher_root(&settings), Some(expected));
}
