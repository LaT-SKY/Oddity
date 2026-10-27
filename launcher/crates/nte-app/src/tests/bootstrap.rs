use super::*;
use crate::*;

/// 一个最小可用的安装目录树：<root>/install/NTELauncher/...
struct Install {
    dir: TempDir,
    launcher: PathBuf,
}

impl Install {
    fn new(tag: &str) -> Self {
        let dir = TempDir::new(tag);
        let launcher = dir.join("install/NTELauncher");
        std::fs::create_dir_all(&launcher).expect("create launcher dir");
        Self { dir, launcher }
    }

    fn launcher_config(&self, body: &str) {
        self.dir
            .write_text("install/NTELauncher/Config/Config.ini", body);
    }

    fn user_config(&self, body: &str) {
        self.dir
            .write_text("install/NTELauncher/UserData/Config/Config.ini", body);
    }

    fn game_exe(&self) {
        self.dir.write(
            "install/Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe",
            b"MZ",
        );
    }

    fn settings(&self) -> Settings {
        Settings {
            install_root: Some(self.launcher.display().to_string()),
            ..Default::default()
        }
    }
}

/// 隔离所有会影响安装目录探测的环境变量。
fn isolated_env(dir: &TempDir) -> EnvGuard {
    let mut env = EnvGuard::new();
    env.set("HOME", dir.join("home"));
    env.set("XDG_DATA_HOME", dir.join("data"));
    env.set("XDG_CONFIG_HOME", dir.join("config"));
    env.remove("WINEPREFIX");
    env.remove("PROTONPATH");
    env.remove("NTE_ROOT");
    env.remove("NTE_INSTALL");
    env.remove("STEAM_COMPAT_CLIENT_INSTALL_PATH");
    std::fs::create_dir_all(dir.join("home")).expect("create fake home");
    env
}

#[test]
fn bootstrap_reports_a_complete_install() {
    let install = Install::new("bootstrap");
    let mut env = isolated_env(&install.dir);
    install.launcher_config(
        "[General]\nLauncher=NTELauncher.exe\nPatcher=NTEUpdate.exe\nClient=NTEGame.exe\nGameID=1289\n\
         [VERSION]\nVersion=1.4.16\nBuild=20240101\n",
    );
    install.user_config("[Game]\nVersion_=1.4.20\n");
    install.game_exe();
    install
        .dir
        .write("install/NTELauncher/WPGameClientSDK_64.dll", b"MZ");

    install.dir.write("install/NTELauncher/NTEGame.exe", b"MZ");

    std::fs::create_dir_all(install.dir.join("prefix/drive_c")).expect("create prefix");
    let proton = fake_proton(&install.dir.join("tools"), "GE-Proton9-1");
    let bin = install.dir.join("bin");
    write_script(&bin, "umu-run", "#!/bin/sh\nexit 0\n");
    env.set("PATH", &bin);

    let profile = managed_environment(&proton, &install.dir.join("install"), Distribution::Cn);
    let settings = Settings {
        runtime_environments: vec![profile.clone()],
        active_environment: Some(profile.id.clone()),
        ..install.settings()
    };
    let bootstrap = bootstrap_impl(&settings);

    assert!(bootstrap.ok);
    assert!(bootstrap.error.is_none());
    assert_eq!(
        bootstrap.install_root,
        install.dir.join("install").display().to_string()
    );
    assert_eq!(
        bootstrap.launcher_root,
        install.launcher.display().to_string()
    );
    assert_eq!(
        bootstrap.prefix,
        profile.prefix(&data_dir()).unwrap().display().to_string()
    );
    assert!(bootstrap.prefix_exists);
    assert_eq!(bootstrap.proton_name, "GE-Proton9-1");
    assert_eq!(bootstrap.proton_path, proton.display().to_string());
    assert!(bootstrap.umu_available);
    assert_eq!(bootstrap.launcher_version, "1.4.16");
    assert_eq!(bootstrap.launcher_build, "20240101");
    assert_eq!(bootstrap.game_version, "1.4.20");
    assert!(bootstrap.game_exists);
    assert!(bootstrap.game_exe.ends_with("HTGame.exe"));

    let labels: Vec<&str> = bootstrap
        .checks
        .iter()
        .map(|check| check.label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec![
            "启动器配置",
            "游戏可执行文件",
            "umu-launcher",
            "Proton",
            "Wine prefix",
            "官方启动器"
        ]
    );
    assert!(
        bootstrap.checks.iter().all(|check| check.ok),
        "{:?}",
        bootstrap.checks
    );
    assert_eq!(
        bootstrap.checks[5].detail,
        install
            .dir
            .join("install/NTELauncher/NTEGame.exe")
            .display()
            .to_string()
    );
    assert_eq!(bootstrap.settings.install_root, settings.install_root);
    assert_eq!(bootstrap.app_version, env!("CARGO_PKG_VERSION"));
    assert!(bootstrap.os.starts_with(std::env::consts::OS));
}

#[test]
fn bootstrap_reports_an_unreadable_launcher_config() {
    let install = Install::new("bootstrap-broken");
    let _env = isolated_env(&install.dir);
    // 非法 UTF-8：文件存在但读不出配置
    install
        .dir
        .write("install/NTELauncher/Config/Config.ini", [0xff, 0xfe, 0x00]);

    let bootstrap = bootstrap_impl(&install.settings());
    assert!(!bootstrap.ok);
    assert!(
        bootstrap
            .error
            .as_deref()
            .expect("error text")
            .contains("读取启动器配置失败"),
        "{:?}",
        bootstrap.error
    );
    assert!(bootstrap.launcher_root.ends_with("NTELauncher"));
    assert_eq!(bootstrap.launcher_version, "");
}

#[test]
fn resolves_the_official_background_from_the_install() {
    let install = Install::new("background");
    let _env = isolated_env(&install.dir);
    install.launcher_config("[General]\nGameID=1289\n[VERSION]\nVersion=1.0\n");
    install
        .dir
        .write("install/NTELauncher/ResFilesM/1289/bgimgs/bg.mp4", b"video");
    install.dir.write(
        "install/NTELauncher/ResFilesM/1289/bgimgs/bg_0.png",
        b"image",
    );
    install.dir.write_text(
        "install/NTELauncher/ResFilesM/1289/bgimgs/config.json",
        r#"{"video":"bg.mp4","imgs":[{"file":"bg_0.png"}]}"#,
    );

    let background = launcher_background_paths(&install.settings()).expect("find background");
    assert_eq!(
        background.video,
        Some(
            install
                .dir
                .join("install/NTELauncher/ResFilesM/1289/bgimgs/bg.mp4")
        )
    );
    assert_eq!(
        background.image,
        Some(
            install
                .dir
                .join("install/NTELauncher/ResFilesM/1289/bgimgs/bg_0.png")
        )
    );
}

#[test]
fn bootstrap_reports_a_missing_install_when_nothing_is_found() {
    let dir = TempDir::new("bootstrap-missing");
    let _env = isolated_env(&dir);
    let settings = Settings::default();
    let bootstrap = bootstrap_impl(&settings);

    // 机器上 /mnt、/media 里可能真的装着游戏，只有探测不到时才校验失败分支
    if detect_launcher_root(&settings).is_none() {
        assert!(!bootstrap.ok);
        assert!(
            bootstrap
                .error
                .as_deref()
                .expect("error text")
                .contains("尚未找到"),
            "{:?}",
            bootstrap.error
        );
        assert_eq!(bootstrap.launcher_root, "");
        // 未找到安装时仍会给出所选区服的默认目录，供首次安装使用。
        assert_eq!(
            bootstrap.install_root,
            selected_install_root(&settings).display().to_string()
        );
        assert!(!bootstrap.game_exists);
        assert_eq!(bootstrap.checks[0].detail, "未找到");
        assert_eq!(
            bootstrap.checks[1].detail,
            "未找到 HTGame.exe（注意 Win64 大小写）"
        );
        assert!(bootstrap.checks[3].detail.contains("未找到 Proton"));
        assert_eq!(bootstrap.checks[4].detail, bootstrap.prefix);
        assert_eq!(bootstrap.checks[5].detail, "未找到");
    }
}

#[test]
fn bootstrap_requires_a_managed_environment() {
    let install = Install::new("bootstrap-defaults");
    let _env = isolated_env(&install.dir);
    install.launcher_config("[VERSION]\nVersion=1.4.16\n");
    let bootstrap = bootstrap_impl(&install.settings());
    assert!(!bootstrap.ok);
    assert!(bootstrap.error.unwrap().contains("请先"));
    assert!(bootstrap.prefix.is_empty());
    assert!(!bootstrap.prefix_exists);
    assert!(bootstrap.proton_path.is_empty());
}

#[test]
fn bootstrap_does_not_fall_back_from_a_configured_empty_root() {
    let dir = TempDir::new("bootstrap-selected-root");
    let _env = isolated_env(&dir);
    // 旧安装仍在默认位置，但不能覆盖用户明确选择的新目录。
    dir.write_text(
        "home/Games/Neverness To Everness/NTELauncher/Config/Config.ini",
        "[VERSION]\nVersion=1.4.16\n",
    );
    let selected = dir.join("new-install");
    let settings = Settings {
        install_root: Some(selected.display().to_string()),
        ..Default::default()
    };

    let bootstrap = bootstrap_impl(&settings);
    assert_eq!(bootstrap.install_root, selected.display().to_string());
    assert_eq!(bootstrap.launcher_root, "");
    assert!(!bootstrap.ok);
    assert!(
        bootstrap
            .error
            .as_deref()
            .expect("error text")
            .contains("尚未安装启动器文件"),
        "{:?}",
        bootstrap.error
    );
    assert!(!bootstrap.game_exists);

    // 未指定目录时仍使用探测结果。
    let detected = bootstrap_impl(&Settings::default());
    assert_eq!(
        detected.install_root,
        dir.join("home/Games/Neverness To Everness")
            .display()
            .to_string()
    );
    assert!(!detected.ok);
    assert!(detected.error.unwrap().contains("请先"));
}

fn check_install(install: &Install, full: bool) -> Result<CheckResult, String> {
    let install_root = install.launcher.parent().expect("install root");
    check_impl(&install.launcher, install_root, Distribution::Cn, full)
}

fn all_files(entries: &[(&str, u64, &str)]) -> String {
    let mut xml = String::from(
        "<AllFiles><ProductVersion Version=\"1.4.20\"/><Url BaseUrl=\"http://cdn.test/\"/>",
    );
    for (path, size, checksum) in entries {
        xml.push_str(&format!(
            "<File Path=\"{path}\" Checksum=\"{checksum}\" Size=\"{size}\"/>"
        ));
    }
    xml.push_str("</AllFiles>");
    xml
}

#[test]
fn check_impl_reports_the_local_state_against_the_manifest() {
    let server = HttpServer::start();
    let install = Install::new("check-impl");
    install.launcher_config(&format!(
        "[VERSION]\nVersion=1.4.16\nVersionInfoFileURL={}\n",
        server.url("/Version.ini")
    ));
    server.route(
        "/Version.ini",
        format!(
            "[VERSION]\nVersion=1.4.20\nBuild=20240102\nFileListURL={}\n",
            server.url("/AllFiles.xml")
        ),
    );

    let content = b"hello";
    install
        .dir
        .write("install/NTELauncher/data/ok.txt", content);
    install
        .dir
        .write("install/NTELauncher/data/changed.txt", b"hi");
    server.route(
        "/AllFiles.xml",
        all_files(&[
            ("/data/ok.txt", content.len() as u64, &md5_hex(content)),
            ("/data/changed.txt", 9, "00"),
            ("/missing.txt", 7, "11"),
        ]),
    );

    let quick = check_install(&install, false).expect("quick check");
    // Version.ini 请求带上了时间戳参数
    assert!(
        server
            .targets()
            .iter()
            .any(|target| target.starts_with("/Version.ini?t=")),
        "{:?}",
        server.targets()
    );
    assert!(quick.installed);
    assert_eq!(quick.distribution, "cn");
    assert_eq!(
        quick.install_root,
        install.dir.join("install").display().to_string()
    );
    assert_eq!(quick.remote_version, "1.4.20");
    assert_eq!(quick.local_version, "1.4.16");
    assert_eq!(quick.product_version, "1.4.20");
    assert_eq!(quick.files, 3);
    assert_eq!(quick.ok, 1);
    assert_eq!(quick.missing, 1);
    assert_eq!(quick.changed, 1);
    assert_eq!(quick.bytes, 16);
    assert_eq!(quick.missing_preview, vec!["/missing.txt"]);
    assert_eq!(quick.changed_preview, vec!["/data/changed.txt"]);

    // 完整校验会重新算 md5，结果一致
    let full = check_install(&install, true).expect("full check");
    assert_eq!(full.ok, 1);
    assert_eq!(full.missing, 1);
    assert_eq!(full.changed, 1);
    assert_eq!(full.files, 3);
}

#[test]
fn check_impl_truncates_the_preview() {
    let server = HttpServer::start();
    let install = Install::new("check-impl-preview");
    install.launcher_config(&format!(
        "[VERSION]\nVersion=1.4.16\nVersionInfoFileURL={}\n",
        server.url("/Version.ini")
    ));
    server.route(
        "/Version.ini",
        format!(
            "[VERSION]\nVersion=1.4.20\nFileListURL={}\n",
            server.url("/AllFiles.xml")
        ),
    );
    let entries: Vec<(String, u64, String)> = (0..15)
        .map(|index| (format!("/missing-{index}.txt"), 1, "00".to_string()))
        .collect();
    let borrowed: Vec<(&str, u64, &str)> = entries
        .iter()
        .map(|(path, size, checksum)| (path.as_str(), *size, checksum.as_str()))
        .collect();
    server.route("/AllFiles.xml", all_files(&borrowed));

    let result = check_install(&install, false).expect("check");
    assert_eq!(result.files, 15);
    assert_eq!(result.missing, 15);
    assert_eq!(result.missing_preview.len(), 12);
    assert!(result.changed_preview.is_empty());
}

#[test]
fn check_impl_reports_every_failure_step() {
    let server = HttpServer::start();
    let install = Install::new("check-impl-errors");

    // 启动器配置读不出来
    let error = check_install(&install, false).expect_err("no config");
    assert!(error.contains("读取启动器配置失败"), "{error}");

    // Version.ini 取不到
    install.launcher_config(&format!(
        "[VERSION]\nVersion=1.4.16\nVersionInfoFileURL={}\n",
        server.url("/Version.ini")
    ));
    let error = check_install(&install, false).expect_err("no version");
    assert!(error.contains("拉取 Version.ini 失败"), "{error}");

    // 远端缺少 FileListURL
    server.route("/Version.ini", "[VERSION]\nVersion=1.4.20\n");
    let error = check_install(&install, false).expect_err("no file list");
    assert!(error.contains("缺少 FileListURL"), "{error}");

    // AllFiles.xml 取不到
    server.route(
        "/Version.ini",
        format!(
            "[VERSION]\nVersion=1.4.20\nFileListURL={}\n",
            server.url("/AllFiles.xml")
        ),
    );
    let error = check_install(&install, false).expect_err("no manifest");
    assert!(error.contains("拉取 AllFiles.xml 失败"), "{error}");

    // 清单里没有文件同样算失败
    server.route("/AllFiles.xml", "<AllFiles/>");
    let error = check_install(&install, false).expect_err("empty manifest");
    assert!(error.contains("拉取 AllFiles.xml 失败"), "{error}");
}

/// 测试里只需要一个稳定的 md5 十六进制串。
fn md5_hex(bytes: &[u8]) -> String {
    nte_core::update::md5_bytes(bytes)
}
