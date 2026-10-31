use super::*;
use crate::*;

#[test]
fn prepare_presence_writes_backup_and_keeps_credentials() {
    let root = TempDir::new("presence");
    let config_dir = root.join("UserData").join("Config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    let config = config_dir.join("Config.ini");
    std::fs::write(
        &config,
        "[Setting]\r\nautoLogin=1\r\n[User]\r\nuser_t92=deadbeef\r\n",
    )
    .expect("write config");

    prepare_launcher_presence(root.path()).expect("apply presence");
    let updated = std::fs::read_to_string(&config).expect("read updated");
    assert!(updated.contains("autoLogin=1"));
    assert!(updated.contains("user_t92=deadbeef"));
    assert!(updated.contains("autoUpdate=1"));
    assert!(updated.contains("quitWithGame=1"));
    assert!(updated.contains("showAfterGameQuit=0"));
    assert!(updated.contains("closeAction=0"));
    assert!(updated.contains("NeedUpdate=0"));

    let backup = config_dir.join("Config.ini.nte-presence.bak");
    assert!(backup.is_file());
    assert_eq!(
        std::fs::read_to_string(&backup).expect("read backup"),
        "[Setting]\r\nautoLogin=1\r\n[User]\r\nuser_t92=deadbeef\r\n"
    );
    // 配置里可能有账号信息，备份必须保持私有
    let mode = std::fs::metadata(&backup)
        .expect("backup metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    assert!(!config_dir.join("Config.ini.nte-presence.tmp").exists());

    // 第二次执行是空操作，不会重写备份
    let backup_before = std::fs::read(&backup).expect("read backup");
    prepare_launcher_presence(root.path()).expect("apply presence again");
    let backup_after = std::fs::read(&backup).expect("read backup again");
    assert_eq!(backup_before, backup_after);
}

#[test]
fn presence_is_a_noop_without_a_config() {
    let dir = TempDir::new("presence-missing");
    prepare_launcher_presence(dir.path()).expect("missing config is fine");
    assert!(!dir.join("UserData/Config").exists());
}

#[test]
fn presence_reports_unreadable_configs() {
    let dir = TempDir::new("presence-unreadable");
    std::fs::create_dir_all(dir.join("UserData/Config/Config.ini")).expect("create directory");
    let error = prepare_launcher_presence(dir.path()).expect_err("config is a directory");
    assert!(error.contains("read "), "{error}");
}

#[test]
fn presence_leaves_an_up_to_date_config_alone() {
    let dir = TempDir::new("presence-stable");
    let original = "[Setting]\nautoUpdate=1\nquitWithGame=1\nshowAfterGameQuit=0\ncloseAction=0\n[UPDATER]\nNeedUpdate=0\n";
    dir.write_text("UserData/Config/Config.ini", original);

    prepare_launcher_presence(dir.path()).expect("apply presence");

    assert_eq!(dir.read_text("UserData/Config/Config.ini"), original);
    assert!(!dir
        .join("UserData/Config/Config.ini.nte-presence.bak")
        .exists());
    assert!(!dir
        .join("UserData/Config/Config.ini.nte-presence.tmp")
        .exists());
}

#[test]
fn presence_settings_preserve_other_sections_and_crlf() {
    let input =
        "[Setting]\r\nautoLogin=1\r\n\r\n[UPDATER]\r\nNeedUpdate=1\r\n[Game]\r\nloginType=92\r\n";
    let output = apply_presence_settings(input);
    assert!(output.contains("autoLogin=1\r\n"));
    assert!(output.contains("autoUpdate=1\r\n"));
    assert!(output.contains("quitWithGame=1\r\n"));
    assert!(output.contains("showAfterGameQuit=0\r\n"));
    assert!(output.contains("closeAction=0\r\n"));
    assert!(output.contains("NeedUpdate=0\r\n"));
    assert!(!output.contains("NeedUpdate=1"));
    assert!(output.contains("loginType=92\r\n"));
    assert_eq!(apply_presence_settings(&output), output);
}

#[test]
fn presence_settings_add_missing_sections() {
    let output = apply_presence_settings("[GENERAL]\npreVersion=1\n");
    assert!(output.contains("[Setting]\n"));
    assert!(output.contains("autoUpdate=1\n"));
    assert!(output.contains("quitWithGame=1\n"));
    assert!(output.contains("showAfterGameQuit=0\n"));
    assert!(output.contains("closeAction=0\n"));
    assert!(output.contains("[UPDATER]\n"));
    assert!(output.contains("NeedUpdate=0\n"));
    assert!(output.contains("preVersion=1"));
}

#[test]
fn presence_settings_cover_an_empty_file() {
    assert_eq!(
        apply_presence_settings(""),
        "[Setting]\nautoUpdate=1\nquitWithGame=1\nshowAfterGameQuit=0\ncloseAction=0\n[UPDATER]\nNeedUpdate=0\n"
    );
}

#[test]
fn presence_settings_handle_section_and_key_case() {
    let output = apply_presence_settings("[SETTING]\nAUTOUPDATE=0\n[updater]\nneedupdate=1\n");
    assert!(output.contains("AUTOUPDATE=1"), "{output}");
    assert!(output.contains("quitWithGame=1"), "{output}");
    assert!(output.contains("needupdate=0"), "{output}");
    assert!(!output.contains("AUTOUPDATE=0"), "{output}");
    assert!(!output.contains("needupdate=1"), "{output}");
    // 两个段都存在，不再追加新段
    assert_eq!(output.matches("[SETTING]").count(), 1);
    assert_eq!(output.matches("[updater]").count(), 1);
}

#[test]
fn presence_settings_keep_a_last_line_without_newline() {
    let output = apply_presence_settings("[setting]\nautoUpdate=1");
    assert!(output.contains("autoUpdate=1"), "{output}");
    assert!(output.contains("quitWithGame=1\n"), "{output}");
    assert!(output.contains("[UPDATER]\n"), "{output}");

    // 使用 CRLF 的文件里补出来的键也要是 CRLF
    let output = apply_presence_settings("[General]\r\nkey=1");
    assert!(output.contains("key=1"), "{output}");
    assert!(output.ends_with("NeedUpdate=0\r\n"), "{output}");
}

#[test]
fn presence_settings_append_to_an_empty_section() {
    let output = apply_presence_settings("[Setting]\n[UPDATER]\n");
    assert_eq!(
        output,
        "[Setting]\nautoUpdate=1\nquitWithGame=1\nshowAfterGameQuit=0\ncloseAction=0\n[UPDATER]\nNeedUpdate=0\n"
    );
}

#[test]
fn line_endings_follow_the_file_style() {
    assert_eq!(line_ending("a\r\n", "\n"), "\r\n");
    assert_eq!(line_ending("a\n", "\r\n"), "\n");
    assert_eq!(line_ending("a", "\r\n"), "\r\n");
    assert_eq!(line_ending("a", "\n"), "\n");
}

#[test]
fn windows_path_uses_z_drive() {
    assert_eq!(
        windows_path(Path::new("/mnt/windows/Program Files (x86)/NTE/HTGame.exe")),
        "Z:\\mnt\\windows\\Program Files (x86)\\NTE\\HTGame.exe"
    );
    assert_eq!(windows_path(Path::new("/")), "Z:\\");
    assert_eq!(
        windows_path(Path::new("relative/path")),
        "Z:\\relative\\path"
    );
}

#[test]
fn bundled_helper_is_a_pe() {
    assert!(LAUNCH_HELPER_BYTES.starts_with(b"MZ"));
    assert!(LAUNCH_HELPER_BYTES.len() > 1024);
    assert!(WINDOW_GUARD_BYTES.starts_with(b"MZ"));
    assert!(WINDOW_GUARD_BYTES.len() > 1024);
}

#[test]
fn installs_and_refreshes_the_bundled_helper() {
    let mut env = EnvGuard::new();
    let dir = TempDir::new("helper");
    env.set("XDG_DATA_HOME", dir.path());

    let path = ensure_launch_helper().expect("install helper");
    assert_eq!(path, dir.join("oddity/helpers/nte-launch-helper.exe"));
    assert_eq!(
        std::fs::read(&path).expect("read helper"),
        LAUNCH_HELPER_BYTES
    );

    let guard = dir.join("oddity/helpers/nte-window-guard.dll");
    assert_eq!(std::fs::read(&guard).unwrap(), WINDOW_GUARD_BYTES);

    // 内容一致时不重写
    let stamp = std::fs::metadata(&path)
        .expect("helper metadata")
        .modified()
        .expect("mtime");
    ensure_launch_helper().expect("reinstall helper");
    assert_eq!(
        std::fs::metadata(&path)
            .expect("helper metadata")
            .modified()
            .expect("mtime"),
        stamp
    );

    // 内容不符时重新写入
    std::fs::write(&path, b"MZ stale").expect("write stale helper");
    std::fs::write(&guard, b"MZ stale guard").expect("write stale guard");
    ensure_launch_helper().expect("refresh helper");
    assert_eq!(
        std::fs::read(&path).expect("read helper"),
        LAUNCH_HELPER_BYTES
    );
    assert_eq!(std::fs::read(&guard).unwrap(), WINDOW_GUARD_BYTES);
    assert!(!dir.join("oddity/helpers/nte-window-guard.dll.new").exists());
    assert!(!dir
        .join("oddity/helpers/nte-launch-helper.exe.new")
        .exists());
}

#[test]
fn launch_log_tail_keeps_the_last_lines() {
    let dir = TempDir::new("log-tail");
    assert_eq!(launch_log_tail(&dir.join("missing.log")), "");

    dir.write_text("empty.log", "\n   \n");
    assert_eq!(launch_log_tail(&dir.join("empty.log")), "");

    dir.write_text("full.log", "first\n  second  \nthird\nfourth\n");
    assert_eq!(
        launch_log_tail(&dir.join("full.log")),
        "：second | third | fourth"
    );

    dir.write_text("long.log", &format!("{}\n", "x".repeat(500)));
    let tail = launch_log_tail(&dir.join("long.log"));
    assert_eq!(tail.chars().count(), 402);
    assert!(tail.ends_with('…'), "{tail}");
}
