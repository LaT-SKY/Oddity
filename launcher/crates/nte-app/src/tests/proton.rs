use super::*;
use crate::*;

#[test]
fn proton_install_needs_manifest_and_accepts_dir_or_exe() {
    let root = TempDir::new("proton");
    std::fs::write(root.join("proton"), b"").expect("write proton");
    assert!(proton_install(root.path()).is_none());

    std::fs::write(root.join("toolmanifest.vdf"), b"").expect("write manifest");
    let from_dir = proton_install(root.path()).expect("dir install");
    assert_eq!(from_dir.root, root.path());
    assert_eq!(
        from_dir.name,
        root.path().file_name().unwrap().to_string_lossy()
    );

    let from_exe = proton_install(&root.join("proton")).expect("exe install");
    assert_eq!(from_exe.root, root.path());

    // 上层目录不是 Proton 时，指向不存在的可执行文件也无效
    let bare = TempDir::new("proton-bare");
    std::fs::create_dir_all(bare.join("nested")).expect("create nested dir");
    assert!(proton_install(&bare.join("nested/proton")).is_none());
}

#[test]
fn detect_proton_prefers_explicit_paths() {
    let mut env = EnvGuard::new();
    let dir = TempDir::new("proton-detect");
    env.set("HOME", dir.join("home"));
    env.remove("XDG_DATA_HOME");
    env.remove("STEAM_COMPAT_CLIENT_INSTALL_PATH");
    env.remove("PROTONPATH");

    let proton = fake_proton(dir.path(), "GE-Proton9-1");
    let settings = Settings {
        proton_path: Some(proton.display().to_string()),
        ..Default::default()
    };
    let install = detect_proton(&settings).expect("from settings");
    assert_eq!(install.name, "GE-Proton9-1");
    assert_eq!(install.root, proton);

    // 明确指定的版本不可用时不能静默改用其他版本
    env.set("PROTONPATH", &proton);
    let stale = Settings {
        proton_path: Some("/nope".to_string()),
        ..Default::default()
    };
    assert!(detect_proton(&stale).is_none());
    assert_eq!(
        detect_proton(&Settings::default()).expect("from env").root,
        proton
    );

    env.remove("PROTONPATH");
    assert!(detect_proton(&Settings::default()).is_none());
}

#[test]
fn detect_proton_scores_steam_builds() {
    let mut env = EnvGuard::new();
    let dir = TempDir::new("proton-score");
    env.set("HOME", dir.join("home"));
    env.remove("XDG_DATA_HOME");
    env.remove("PROTONPATH");
    let steam = dir.join("steam");
    env.set("STEAM_COMPAT_CLIENT_INSTALL_PATH", &steam);

    let custom = steam.join("compatibilitytools.d");
    fake_proton(&custom, "GE-Proton9-1");
    fake_proton(&custom, "CachyOS-9-1");
    let common = steam.join("steamapps/common");
    fake_proton(&common, "Proton-9.0");
    fake_proton(&common, "Proton-8.0");
    fake_proton(&common, "Proton-Experimental");
    // steamapps/common 下不以 Proton 开头的目录会被跳过
    fake_proton(&common, "SomeOtherTool");
    // 只有 manifest 的目录不是有效的 Proton
    std::fs::create_dir_all(custom.join("ProtonIncomplete")).expect("create dir");
    std::fs::write(custom.join("ProtonIncomplete/toolmanifest.vdf"), b"").expect("write manifest");

    let pick = |settings: &Settings| detect_proton(settings).map(|install| install.name);
    assert_eq!(pick(&Settings::default()).as_deref(), Some("GE-Proton9-1"));

    std::fs::remove_dir_all(custom.join("GE-Proton9-1")).expect("remove GE-Proton");
    assert_eq!(pick(&Settings::default()).as_deref(), Some("CachyOS-9-1"));

    std::fs::remove_dir_all(custom.join("CachyOS-9-1")).expect("remove CachyOS");
    // 同分时按名字倒序，Proton-9.0 比 Proton-8.0 优先
    assert_eq!(pick(&Settings::default()).as_deref(), Some("Proton-9.0"));

    std::fs::remove_dir_all(common.join("Proton-9.0")).expect("remove Proton-9.0");
    std::fs::remove_dir_all(common.join("Proton-8.0")).expect("remove Proton-8.0");
    // Experimental 排最后
    assert_eq!(
        pick(&Settings::default()).as_deref(),
        Some("Proton-Experimental")
    );

    std::fs::remove_dir_all(common.join("Proton-Experimental")).expect("remove Experimental");
    assert_eq!(pick(&Settings::default()), None);
}

#[test]
fn ensure_sdk_link_creates_and_refreshes_the_link() {
    let dir = TempDir::new("sdk-link");
    let prefix = dir.join("prefix");
    let install = dir.join("install");
    std::fs::create_dir_all(&install).expect("create install dir");

    let error = ensure_sdk_link(&prefix, &install).expect_err("no drive_c");
    assert!(error.contains("Wine prefix 尚未初始化"), "{error}");

    std::fs::create_dir_all(prefix.join("drive_c")).expect("create drive_c");
    ensure_sdk_link(&prefix, &install).expect("create link");
    let link = prefix.join("drive_c/Program Files (x86)/Neverness To Everness");
    assert!(std::fs::symlink_metadata(&link)
        .expect("link metadata")
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read_link(&link).expect("read link"), install);

    // 已经指向正确目标时保持原样
    ensure_sdk_link(&prefix, &install).expect("keep link");
    assert_eq!(std::fs::read_link(&link).expect("read link"), install);

    // 指向别处时替换掉
    std::fs::remove_file(&link).expect("remove link");
    let other = dir.join("other");
    std::fs::create_dir_all(&other).expect("create other dir");
    std::os::unix::fs::symlink(&other, &link).expect("create stale link");
    ensure_sdk_link(&prefix, &install).expect("refresh link");
    assert_eq!(std::fs::read_link(&link).expect("read link"), install);

    // 真实目录不会被替换
    std::fs::remove_file(&link).expect("remove link");
    std::fs::create_dir_all(&link).expect("create real dir");
    ensure_sdk_link(&prefix, &install).expect("keep real dir");
    assert!(!std::fs::symlink_metadata(&link)
        .expect("link metadata")
        .file_type()
        .is_symlink());
}

#[test]
fn hide_wine_tray_items_invokes_wine_reg() {
    let dir = TempDir::new("wine-tray");
    let log = dir.join("wine-args.txt");
    let proton_root = dir.join("proton");
    std::fs::create_dir_all(proton_root.join("files/bin")).expect("create wine dir");
    write_script(
        &proton_root.join("files/bin"),
        "wine",
        &format!("#!/bin/sh\necho \"$@\" > {}\nexit 0\n", log.display()),
    );
    let proton = ProtonInstall {
        name: "GE-Proton9-1".to_string(),
        root: proton_root,
    };
    let prefix = dir.join("prefix");

    hide_wine_tray_items(&proton, &prefix).expect("hide tray items");
    let args = std::fs::read_to_string(&log).expect("read wine log");
    assert!(args.contains("NoTrayItemsDisplay"), "{args}");
    assert!(args.contains("REG_DWORD"), "{args}");
    assert!(
        args.contains(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Policies\Explorer"),
        "{args}"
    );

    // 没有 wine 时明确报错
    let missing = ProtonInstall {
        name: "custom".to_string(),
        root: dir.join("empty-proton"),
    };
    let error = hide_wine_tray_items(&missing, &prefix).expect_err("missing wine");
    assert!(error.contains("未找到"), "{error}");

    // wine 返回非零时把状态带出来
    let failing_root = dir.join("failing-proton");
    std::fs::create_dir_all(failing_root.join("files/bin")).expect("create wine dir");
    write_script(
        &failing_root.join("files/bin"),
        "wine",
        "#!/bin/sh\nexit 7\n",
    );
    let failing = ProtonInstall {
        name: "custom".to_string(),
        root: failing_root,
    };
    let error = hide_wine_tray_items(&failing, &prefix).expect_err("wine failed");
    assert!(error.contains("wine reg 返回"), "{error}");
}
