use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn persistent_stdio_roundtrip_errors_events_and_shutdown() {
    let directory = std::env::temp_dir().join(format!("oddity-ipc-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::create_dir_all(directory.join("data/oddity/logs")).unwrap();
    std::fs::write(
        directory.join("data/oddity/logs/test.log"),
        "协议导出测试\n",
    )
    .unwrap();
    std::fs::create_dir_all(directory.join("commands")).unwrap();
    let systemctl = directory.join("commands/systemctl");
    std::fs::write(&systemctl, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&systemctl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_oddity-backend"))
        .arg("--ipc")
        .env("HOME", &directory)
        .env("XDG_CONFIG_HOME", directory.join("config"))
        .env("XDG_DATA_HOME", directory.join("data"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent/oddity-protocol-bus",
        )
        .env("PATH", directory.join("commands"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    writeln!(input, "{{invalid json").unwrap();
    let output = child.stdout.take().unwrap();
    let (sender, replies) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            sender
                .send(serde_json::from_str::<Value>(&line.unwrap()).unwrap())
                .unwrap();
        }
    });
    let mut id = 0;
    let mut call = |command: &str, args: Value| {
        id += 1;
        writeln!(input, "{}", json!({"id":id,"command":command,"args":args})).unwrap();
        loop {
            let value = replies.recv_timeout(Duration::from_secs(15)).unwrap();
            if value.get("id") == Some(&json!(id)) {
                break value;
            }
            assert!(value.get("event").is_some(), "{value}");
        }
    };
    let bootstrap = call("bootstrap", json!({}));
    let mut settings = bootstrap["result"]["settings"].clone();
    assert!(settings.is_object(), "{bootstrap}");
    settings["installRoot"] = json!(directory.join("installation-cn"));
    settings["globalInstallRoot"] = json!(directory.join("installation-global"));
    assert!(call("save_settings", json!({"settings":settings}))
        .get("result")
        .is_some());
    for command in [
        "attendance_login",
        "attendance_send_code",
        "attendance_login_sms",
        "attendance_configure",
        "attendance_logout",
        "game_download_start",
        "environment_create",
        "environment_action",
        "browse_files",
        "game_mode_gpu_apply",
        "game_mode_tuning_apply",
        "gamepad_feedback",
        "gamepad_profile_validate",
        "gamepad_profile_load",
        "gamepad_profile_save",
        "save_log_export",
        "discard_log_export",
        "check_update",
        "fetch_bulletins",
        "save_settings",
        "open_url",
        "open_directory",
        "playtime_export_poster",
        "environment_snapshot_compare",
        "environment_snapshot_save_reference",
        "host_state",
        "external_launch",
    ] {
        let reply = call(command, json!(false));
        assert!(reply["error"].is_string(), "{command}: {reply}");
    }
    for command in [
        "launcher_background",
        "game_download_status",
        "game_download_cancel",
        "game_mode_gpu_info",
        "game_mode_tuning_info",
        "gamepad_poll",
        "gamepad_release_idle",
        "game_mode_status",
        "game_status",
        "take_launch_error",
        "cancel_launch",
        "close_game",
        "playtime_history",
        "environment_snapshot_overview",
        "environment_snapshot_clear_history",
        "environment_snapshot_delete_reference",
        "attendance_status",
    ] {
        let reply = call(command, json!({}));
        assert!(reply.get("result").is_some(), "{command}: {reply}");
    }
    for (command, args) in [
        ("unregistered", json!({})),
        (
            "external_launch",
            json!({"argv":["--launch","--region","invalid"],"cold":false}),
        ),
        (
            "environment_create",
            json!({"name":"test","protonPath":"/missing/proton","sourceId":null,"distribution":"cn"}),
        ),
        ("environment_action", json!({"action":"invalid","id":null})),
        (
            "game_download_start",
            json!({"mode":"invalid","full":false}),
        ),
        (
            "game_mode_gpu_apply",
            json!({"config":nte_core::game_mode::gpu::Config::default()}),
        ),
        (
            "game_mode_tuning_apply",
            json!({"config":nte_core::game_mode::tuning::Config::default()}),
        ),
        ("open_url", json!({"url":"file:///etc/passwd"})),
        ("open_directory", json!({"path":directory.join("missing")})),
        ("environment_snapshot_compare", json!({"target":"missing"})),
        (
            "playtime_export_poster",
            json!({"destination":directory.join("poster.png"),"data":"invalid"}),
        ),
    ] {
        let reply = call(command, args);
        assert!(reply.get("error").is_some(), "{command}: {reply}");
    }
    let reference = call(
        "environment_snapshot_save_reference",
        json!({"label":"测试参考","attemptId":null}),
    );
    assert_eq!(reference["result"]["label"], "测试参考", "{reference}");
    let listing = call("browse_files", json!({"path":directory}));
    assert!(listing["result"]["entries"].is_array(), "{listing}");
    let update = call("check_update", json!({"full":false}));
    assert_eq!(update["result"]["installed"], false, "{update}");
    let feedback = call("gamepad_feedback", json!({"deviceId":0,"effect":"stop"}));
    assert!(feedback.get("result").is_some(), "{feedback}");
    let profile = nte_core::gamepad::Profile::default();
    let profile_path = directory.join("profile.json");
    for (command, args) in [
        ("gamepad_profile_validate", json!({"profile":profile})),
        (
            "gamepad_profile_save",
            json!({"path":profile_path,"profile":profile}),
        ),
        ("gamepad_profile_load", json!({"path":profile_path})),
    ] {
        let reply = call(command, args);
        assert!(reply.get("result").is_some(), "{command}: {reply}");
    }
    let prepared = call("prepare_log_export", json!({}));
    let saved = call(
        "save_log_export",
        json!({"id":prepared["result"]["id"],"destination":directory}),
    );
    assert!(saved["result"].is_string(), "{saved}");
    assert!(std::path::Path::new(saved["result"].as_str().unwrap()).is_file());
    let prepared = call("prepare_log_export", json!({}));
    assert!(
        call("discard_log_export", json!({"id":prepared["result"]["id"]}))
            .get("result")
            .is_some()
    );
    settings["theme"] = json!("dark");
    settings["distribution"] = json!("global");
    let saved = call("save_settings", json!({"settings":settings}));
    assert_eq!(saved["result"]["settings"]["theme"], "dark", "{saved}");
    for (command, args) in [
        ("attendance_refresh_game_records", json!({})),
        (
            "attendance_login",
            json!({"phone":"","password":"","rememberPassword":false}),
        ),
        ("attendance_send_code", json!({"phone":""})),
        (
            "attendance_login_sms",
            json!({"phone":"","code":"","deviceId":""}),
        ),
        ("attendance_run", json!({})),
    ] {
        let reply = call(command, args);
        assert_eq!(reply["error"]["kind"], "service", "{reply}");
        assert!(reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("仅支持国服"));
    }
    let logout = call("attendance_logout", json!({"accountId":"missing"}));
    assert!(logout.get("error").is_some(), "{logout}");
    let keyring = call("attendance_keyring_check", json!({}));
    assert!(keyring["error"].is_object(), "{keyring}");
    let preferences = call(
        "attendance_configure",
        json!({"preferences":nte_core::attendance::Preferences::default()}),
    );
    assert!(preferences.get("result").is_some(), "{preferences}");
    let launch = call("launch_official", json!({}));
    assert!(launch["error"].is_string(), "{launch}");
    assert!(call("host_state", json!({"focused":false,"visible":false}))
        .get("result")
        .is_some());
    drop(input);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            panic!("stdin 关闭后后台服务未退出");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    std::fs::remove_dir_all(directory).unwrap();
}
