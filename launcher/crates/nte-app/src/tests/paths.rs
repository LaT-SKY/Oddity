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
fn default_prefix_prefers_wineprefix() {
    let mut env = EnvGuard::new();
    let home = TempDir::new("prefix-home");
    env.set("HOME", home.path());
    env.remove("XDG_DATA_HOME");
    env.remove("WINEPREFIX");
    assert_eq!(default_prefix(), home.join(".local/share/oddity/prefix"));

    // 空值等于没设置
    env.set("WINEPREFIX", "");
    assert_eq!(default_prefix(), home.join(".local/share/oddity/prefix"));

    env.set("WINEPREFIX", "/custom/prefix");
    assert_eq!(default_prefix(), PathBuf::from("/custom/prefix"));
}

#[test]
fn steam_roots_deduplicate_and_follow_the_environment() {
    let mut env = EnvGuard::new();
    let home = TempDir::new("steam-home");
    env.set("HOME", home.path());
    env.remove("STEAM_COMPAT_CLIENT_INSTALL_PATH");
    env.remove("XDG_DATA_HOME");

    let roots = steam_roots();
    assert_eq!(
        roots,
        vec![
            home.join(".local/share/Steam"),
            home.join(".steam/steam"),
            home.join(".steam/root"),
        ]
    );

    // 客户端路径在前，重复项被去掉
    env.set("XDG_DATA_HOME", home.join("xdg"));
    env.set(
        "STEAM_COMPAT_CLIENT_INSTALL_PATH",
        home.join(".local/share/Steam"),
    );
    let roots = steam_roots();
    assert_eq!(roots[0], home.join(".local/share/Steam"));
    assert_eq!(roots[1], home.join("xdg/Steam"));
    assert_eq!(roots.len(), 4);

    // 空值直接忽略，此时 XDG 里的 Steam 排在最前
    env.set("STEAM_COMPAT_CLIENT_INSTALL_PATH", "");
    let roots = steam_roots();
    assert_eq!(roots.len(), 4);
    assert_eq!(roots[0], home.join("xdg/Steam"));

    env.remove("XDG_DATA_HOME");
    assert_eq!(steam_roots().len(), 3);
}

#[test]
fn install_seeds_cover_the_known_layouts() {
    let mut seeds = Vec::new();
    install_seeds(Path::new("/mnt/games"), &mut seeds);
    assert_eq!(
        seeds,
        vec![
            PathBuf::from("/mnt/games/Program Files (x86)/Neverness To Everness/NTELauncher"),
            PathBuf::from("/mnt/games/Program Files/Neverness To Everness/NTELauncher"),
            PathBuf::from("/mnt/games/Neverness To Everness/NTELauncher"),
            PathBuf::from("/mnt/games/NTELauncher"),
        ]
    );
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
    assert_eq!(normalize_launcher_root(dir.path()), None);
    dir.write_text("Config/Config.ini", "[VERSION]\n");
    assert_eq!(
        normalize_launcher_root(dir.path()),
        Some(dir.path().to_path_buf())
    );

    // 安装目录形态：<install>/NTELauncher/Config/Config.ini
    let install = TempDir::new("root-normalize-nested");
    install.write_text("NTELauncher/Config/Config.ini", "[VERSION]\n");
    assert_eq!(
        normalize_launcher_root(install.path()),
        Some(install.join("NTELauncher"))
    );
}

#[test]
fn detects_the_launcher_from_settings_environment_and_seeds() {
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
    assert_eq!(detect_launcher_root(&settings), Some(root.clone()));

    // 设置里的路径无效时继续找环境变量
    let stale = Settings {
        install_root: Some("/nope".to_string()),
        ..Default::default()
    };
    assert_ne!(detect_launcher_root(&stale), Some(PathBuf::from("/nope")));
    env.set("NTE_ROOT", install.path());
    assert_eq!(detect_launcher_root(&stale), Some(root.clone()));
    env.remove("NTE_ROOT");
    env.set("NTE_INSTALL", install.path());
    assert_eq!(detect_launcher_root(&stale), Some(root.clone()));
    env.remove("NTE_INSTALL");

    // 常见的 ~/Games 安装位置
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
fn bridge_root_follows_the_priority_order() {
    let mut env = EnvGuard::new();
    let home = TempDir::new("bridge-home");
    env.set("HOME", home.path());
    env.remove("XDG_DATA_HOME");
    std::fs::create_dir_all(home.join("games/install/NTELauncher")).expect("create launcher dir");
    let launcher = home.join("games/install/NTELauncher");

    // 没有桥接副本时退回安装目录的父目录
    assert_eq!(
        bridge_root_for(&Settings::default(), &launcher),
        home.join("games/install")
    );

    // data_dir/winroot 里有 NTEGame.exe 时优先使用
    home.write(".local/share/oddity/winroot/NTELauncher/NTEGame.exe", b"MZ");
    assert_eq!(
        bridge_root_for(&Settings::default(), &launcher),
        home.join(".local/share/oddity/winroot")
    );

    // 设置里显式指定的路径最优先
    let custom = TempDir::new("bridge-custom");
    custom.write("NTELauncher/NTEGame.exe", b"MZ");
    let settings = Settings {
        bridge_root: Some(custom.path().display().to_string()),
        ..Default::default()
    };
    assert_eq!(
        bridge_root_for(&settings, &launcher),
        custom.path().to_path_buf()
    );

    // 设置里的路径无效时忽略，回到 winroot
    let broken = Settings {
        bridge_root: Some("/nope".to_string()),
        ..Default::default()
    };
    assert_eq!(
        bridge_root_for(&broken, &launcher),
        home.join(".local/share/oddity/winroot")
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
