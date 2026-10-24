use super::*;

fn record_response() -> &'static str {
    r#"{"code":0,"data":{"roleid":9007199254740993,"rolename":"海特洛调查员","servername":"国服","staminaValue":0,"staminaMaxValue":"240","citystaminaValue":420,"citystaminaMaxValue":700,"dayvalue":60,"weekcopiesremainCnt":0}}"#
}

#[test]
fn game_records_refresh_all_accounts_without_attendance_and_persist_rotated_tokens() {
    let (dir, server, vault, service) = fixture();
    service.login("13800138000", "password", false).unwrap();
    service.login("13900139000", "password", false).unwrap();
    server.route("/apihub/awapi/yh/roleHome", record_response());
    let status = service.refresh_game_records().unwrap();
    assert_eq!(status.accounts.len(), 2);
    for account in &status.accounts {
        let record = account.game_record.as_ref().unwrap();
        assert_eq!(record.role_id, "9007199254740993");
        assert_eq!(record.stamina, Some(0));
        assert_eq!(record.stamina_max, Some(240));
        assert_eq!(record.city_stamina, Some(420));
        assert_eq!(record.weekly_remaining, Some(0));
        assert!(record.updated_at > 0);
        assert!(account.game_record_error.is_none());
        assert!(account.last_report.is_none());
    }
    assert_eq!(status.preferences.trigger, Trigger::Off);
    assert_eq!(server.hits("/apihub/awapi/yh/roleHome"), 2);
    assert_eq!(server.hits("/apihub/api/signin"), 0);
    assert_eq!(server.hits("/apihub/awapi/sign"), 0);
    assert_eq!(server.hits("/apihub/api/getUserTasks"), 0);
    assert!(vault
        .load_all()
        .unwrap()
        .iter()
        .all(|a| a.refresh_token == "new-refresh"));
    let stored = dir.read_text("state.json");
    for secret in [
        "13800138000",
        "13900139000",
        "password",
        "new-refresh",
        "new-access",
        "laohu-secret",
    ] {
        assert!(!stored.contains(secret));
    }
    assert_eq!(
        service.status().unwrap().accounts[0]
            .game_record
            .as_ref()
            .unwrap()
            .stamina,
        Some(0)
    );
}

#[test]
fn game_record_failure_keeps_previous_data_and_does_not_stop_other_accounts() {
    let (_dir, server, vault, service) = fixture();
    service.login("13800138000", "password", false).unwrap();
    service.login("13900139000", "password", false).unwrap();
    server.route("/apihub/awapi/yh/roleHome", record_response());
    service.refresh_game_records().unwrap();
    *vault.fail_save_phone.lock().unwrap() = Some("13800138000".into());
    server.route(
        "/apihub/awapi/yh/roleHome",
        record_response().replace("\"staminaValue\":0", "\"staminaValue\":120"),
    );
    let status = service.refresh_game_records().unwrap();
    assert_eq!(
        status.accounts[0].game_record.as_ref().unwrap().stamina,
        Some(0)
    );
    assert_eq!(
        status.accounts[0].game_record_error.as_ref().unwrap().kind,
        FailureKind::Keyring
    );
    assert_eq!(
        status.accounts[1].game_record.as_ref().unwrap().stamina,
        Some(120)
    );
    assert!(status.accounts[1].game_record_error.is_none());
    assert!(status.accounts.iter().all(|a| a.attention.is_none()));
    *vault.fail_save_phone.lock().unwrap() = None;
    assert!(service
        .refresh_game_records()
        .unwrap()
        .accounts
        .iter()
        .all(|a| a.game_record_error.is_none()));
}

#[test]
fn game_record_rejects_wrong_identity_and_preserves_unknown_values() {
    let (_dir, server, _vault, service) = fixture();
    service.login("13800138000", "password", false).unwrap();
    server.route(
        "/apihub/awapi/yh/roleHome",
        r#"{"code":0,"data":{"roleid":"wrong","staminaValue":240}}"#,
    );
    let status = service.refresh_game_records().unwrap();
    assert!(status.accounts[0].game_record.is_none());
    assert_eq!(
        status.accounts[0].game_record_error.as_ref().unwrap().kind,
        FailureKind::Protocol
    );
    server.route("/apihub/awapi/yh/roleHome", r#"{"code":0,"data":{"roleid":"9007199254740993","rolename":"bad\nname","staminaValue":-1,"staminaMaxValue":0,"citystaminaValue":"bad","dayvalue":4294967296,"weekcopiesremainCnt":null}}"#);
    let status = service.refresh_game_records().unwrap();
    let record = status.accounts[0].game_record.as_ref().unwrap();
    assert_eq!(record.role_name, "测试角色甲");
    assert_eq!(record.stamina, None);
    assert_eq!(record.stamina_max, Some(0));
    assert_eq!(record.city_stamina, None);
    assert_eq!(record.activity, None);
    assert_eq!(record.weekly_remaining, None);
}

#[test]
fn game_record_refresh_respects_lock_keyring_empty_roles_and_authentication_errors() {
    use std::fs::OpenOptions;
    let (dir, server, vault, service) = fixture();
    assert!(service.refresh_game_records().unwrap().accounts.is_empty());
    service.login("13800138000", "password", false).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("lock"))
        .unwrap();
    lock.lock().unwrap();
    assert_eq!(
        service.refresh_game_records().unwrap_err().kind,
        FailureKind::Busy
    );
    lock.unlock().unwrap();
    vault.fail.store(true, Ordering::Relaxed);
    assert_eq!(
        service.refresh_game_records().unwrap_err().kind,
        FailureKind::Keyring
    );
    assert_eq!(server.hits("/usercenter/api/refreshToken"), 0);
    vault.fail.store(false, Ordering::Relaxed);
    server.route(
        "/usercenter/api/v2/getGameRoles",
        r#"{"code":0,"data":{"roles":[]}}"#,
    );
    assert_eq!(
        service.refresh_game_records().unwrap().accounts[0]
            .game_record_error
            .as_ref()
            .unwrap()
            .kind,
        FailureKind::Protocol
    );
    server.route(
        "/usercenter/api/refreshToken",
        r#"{"code":1,"message":"需要人工验证"}"#,
    );
    assert_eq!(
        service.refresh_game_records().unwrap().accounts[0]
            .game_record_error
            .as_ref()
            .unwrap()
            .kind,
        FailureKind::Verification
    );
    assert_eq!(server.hits("/apihub/api/signin"), 0);
}
