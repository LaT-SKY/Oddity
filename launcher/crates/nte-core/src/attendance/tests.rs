use super::*;
#[path = "game_record_tests.rs"]
mod game_record_tests;
use crate::testing::{HttpServer, TempDir};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

fn completed_tasks() -> &'static str {
    r#"{"code":0,"data":{"task_list1":[
        {"code":"signin_c","completeTimes":1,"limitTimes":1},
        {"code":"browse_post_c","completeTimes":5,"limitTimes":5},
        {"code":"like_post_c","completeTimes":5,"limitTimes":5},
        {"code":"share","completeTimes":1,"limitTimes":1}
    ]}}"#
}

#[derive(Clone, Default)]
struct MemoryVault {
    account: Arc<Mutex<Vec<Account>>>,
    fail: Arc<AtomicBool>,
    fail_save: Arc<AtomicBool>,
    fail_save_phone: Arc<Mutex<Option<String>>>,
}
impl Vault for MemoryVault {
    fn load_all(&self) -> Result<Vec<Account>> {
        if self.fail.load(Ordering::Relaxed) {
            return Err(Failure::new(FailureKind::Keyring, "Keyring 锁定"));
        }
        Ok(self.account.lock().unwrap().clone())
    }
    fn save(&self, a: &Account) -> Result<()> {
        if self.fail.load(Ordering::Relaxed)
            || self.fail_save.load(Ordering::Relaxed)
            || self.fail_save_phone.lock().unwrap().as_deref() == Some(a.phone.as_str())
        {
            return Err(Failure::new(FailureKind::Keyring, "Keyring 锁定"));
        }
        let mut accounts = self.account.lock().unwrap();
        if let Some(existing) = accounts.iter_mut().find(|v| v.phone == a.phone) {
            *existing = a.clone();
        } else {
            accounts.push(a.clone());
        }
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<()> {
        self.account
            .lock()
            .unwrap()
            .retain(|a| account_id(&a.phone) != id);
        Ok(())
    }
}
fn fixture() -> (TempDir, HttpServer, MemoryVault, Service<MemoryVault>) {
    let dir = TempDir::new("taygedo");
    let server = HttpServer::start();
    let vault = MemoryVault::default();
    server.route(
        "/openApi/secureLogin",
        r#"{"code":0,"result":{"token":"laohu-secret","userId":9007199254740993}}"#,
    );
    server.route("/usercenter/api/login",r#"{"code":0,"data":{"accessToken":"access-secret","refreshToken":"refresh-secret","uid":10001}}"#);
    server.route(
        "/usercenter/api/refreshToken",
        r#"{"code":0,"data":{"accessToken":"new-access","refreshToken":"new-refresh"}}"#,
    );
    server.route("/apihub/api/signin", r#"{"code":0}"#);
    server.route(
        "/usercenter/api/v2/getGameRoles",
        r#"{"code":0,"data":{"roles":[{"roleId":9007199254740993,"roleName":"测试角色甲"},{"roleId":"role-2","roleName":"测试角色乙"}]}}"#,
    );
    server.route("/apihub/awapi/sign", r#"{"code":0}"#);
    server.route("/apihub/api/getUserTasks", completed_tasks());
    server.route(
        "/apihub/api/getUserCoinTaskState",
        r#"{"code":0,"data":{"todayCoin":110,"limitCoin":150}}"#,
    );
    server.route(
        "/cloud/game/getUserInfo",
        r#"{"code":0,"result":{"perDayFirstLoginGiveDuration":"15","remainedDuration":"120"}}"#,
    );
    let service = Service::testing(dir.path(), vault.clone(), &server.base());
    (dir, server, vault, service)
}
#[test]
fn login_stores_secrets_only_in_vault_and_does_not_enable_attendance() {
    let (dir, server, vault, service) = fixture();
    assert!(service.status().unwrap().accounts.is_empty());
    let status = service
        .login("13800138000", "test-password", false)
        .unwrap();
    assert_eq!(status.accounts.len(), 1);
    assert_eq!(status.accounts[0].masked_phone, "138****8000");
    assert_eq!(status.preferences.trigger, Trigger::Off);
    let account = vault.load_all().unwrap().remove(0);
    assert!(account.password.is_none());
    assert_eq!(account.laohu_user_id, "9007199254740993");
    let file = dir.read_text("state.json");
    for secret in [
        "13800138000",
        "test-password",
        "laohu-secret",
        "refresh-secret",
        "access-secret",
    ] {
        assert!(!file.contains(secret));
    }
    assert_eq!(server.hits("/apihub/api/signin"), 0);
}
#[test]
fn keyring_failure_prevents_login_network_requests() {
    let (_dir, server, vault, service) = fixture();
    vault.fail.store(true, Ordering::Relaxed);
    assert_eq!(
        service
            .login("13800138000", "test-password", false)
            .unwrap_err()
            .kind,
        FailureKind::Keyring
    );
    assert!(server.targets().is_empty());
    assert!(service.status().unwrap().accounts.is_empty());
}
#[test]
fn enabled_launch_signs_once_and_rotates_keyring_token() {
    let (_dir, server, vault, service) = fixture();
    service.login("13800138000", "test-password", true).unwrap();
    assert_eq!(
        vault.load_all().unwrap().remove(0).password.as_deref(),
        Some("test-password")
    );
    service.run(Some(Trigger::Launch)).unwrap();
    assert_eq!(server.hits("/apihub/api/signin"), 0);
    service.set_trigger(Trigger::Launch);
    let status = service.run(Some(Trigger::Launch)).unwrap();
    assert!(status.accounts[0].last_report.as_ref().unwrap().ok);
    assert_eq!(server.hits("/apihub/awapi/sign"), 2);
    assert_eq!(
        vault.load_all().unwrap().remove(0).refresh_token,
        "new-refresh"
    );
    service.run(Some(Trigger::Launch)).unwrap();
    assert_eq!(server.hits("/usercenter/api/refreshToken"), 1);
}
#[test]
fn verification_pauses_automatic_runs_and_does_not_relogin() {
    let (_dir, server, _vault, service) = fixture();
    service.login("13800138000", "test-password", true).unwrap();
    service.set_trigger(Trigger::Launch);
    server.route(
        "/usercenter/api/refreshToken",
        r#"{"code":7,"msg":"需要人机验证 private-token"}"#,
    );
    let status = service.run(Some(Trigger::Launch)).unwrap();
    assert_eq!(
        status.accounts[0].attention.as_ref().unwrap().kind,
        FailureKind::Verification
    );
    service.run(Some(Trigger::Launch)).unwrap();
    assert_eq!(server.hits("/usercenter/api/refreshToken"), 1);
    assert_eq!(server.hits("/openApi/secureLogin"), 1);
    assert!(service.run(None).unwrap().accounts[0].attention.is_some());
    assert!(!serde_json::to_string(&service.status().unwrap())
        .unwrap()
        .contains("private-token"));
}
#[test]
fn partial_failure_remains_retryable_and_keeps_rotated_token() {
    let (_dir, server, vault, service) = fixture();
    service
        .login("13800138000", "test-password", false)
        .unwrap();
    server.route(
        "/apihub/api/signin",
        r#"{"code":500,"msg":"private-value"}"#,
    );
    let status = service.run(None).unwrap();
    let report = status.accounts[0].last_report.as_ref().unwrap();
    assert!(!report.ok);
    assert!(report.steps[1].ok);
    assert_eq!(
        vault.load_all().unwrap().remove(0).refresh_token,
        "new-refresh"
    );
    service.run(None).unwrap();
    assert_eq!(server.hits("/usercenter/api/refreshToken"), 2);
}
#[test]
fn invalid_inputs_do_not_make_requests_and_logout_clears_vault() {
    let (_dir, server, vault, service) = fixture();
    assert!(service.login("bad", "secret", false).is_err());
    assert!(server.targets().is_empty());
    service.login("13800138000", "secret", false).unwrap();
    // 不创建定时器, 默认模式可直接移除账号。
    let mut env = crate::testing::EnvGuard::new();
    let config = TempDir::new("logout-config");
    env.set("XDG_CONFIG_HOME", config.path());
    assert!(service
        .logout(&account_id("13800138000"), std::path::Path::new("/oddity"))
        .unwrap()
        .accounts
        .is_empty());
    assert!(vault.load_all().unwrap().is_empty());
    for time in ["24:00", "12:60", "1:00", "你好好"] {
        assert!(Preferences {
            time: time.into(),
            ..Preferences::default()
        }
        .validate()
        .is_err());
    }
    assert_eq!(china_day(16 * 3600 - 1), 0);
    assert_eq!(china_day(16 * 3600), 1);
}

#[test]
fn refresh_rejection_rebuilds_session_without_password_login() {
    let (_dir, server, _vault, service) = fixture();
    service
        .login("13800138000", "test-password", false)
        .unwrap();
    server.route(
        "/usercenter/api/refreshToken",
        r#"{"code":1,"msg":"token 已失效"}"#,
    );
    let status = service.run(None).unwrap();
    let report = status.accounts[0].last_report.as_ref().unwrap();
    assert!(report.ok, "{report:?}");
    assert_eq!(server.hits("/openApi/secureLogin"), 1);
    assert_eq!(server.hits("/usercenter/api/login"), 2);
}
#[test]
fn expired_session_without_password_requires_login_and_pauses() {
    let (_dir, server, _vault, service) = fixture();
    service
        .login("13800138000", "test-password", false)
        .unwrap();
    service.set_trigger(Trigger::Launch);
    server.route(
        "/usercenter/api/refreshToken",
        r#"{"code":1,"msg":"token 已失效"}"#,
    );
    server.route("/usercenter/api/login", r#"{"code":1,"msg":"登录失效"}"#);
    let status = service.run(Some(Trigger::Launch)).unwrap();
    assert_eq!(
        status.accounts[0].attention.as_ref().unwrap().kind,
        FailureKind::Auth
    );
    service.run(Some(Trigger::Launch)).unwrap();
    assert_eq!(server.hits("/usercenter/api/refreshToken"), 1);
    assert_eq!(server.hits("/openApi/secureLogin"), 1);
}
#[test]
fn saved_password_allows_one_relogin_attempt_and_verification_stops_it() {
    let (_dir, server, _vault, service) = fixture();
    service.login("13800138000", "test-password", true).unwrap();
    server.route(
        "/usercenter/api/refreshToken",
        r#"{"code":1,"msg":"token 已失效"}"#,
    );
    server.route("/usercenter/api/login", r#"{"code":1,"msg":"登录失效"}"#);
    server.route("/openApi/secureLogin", r#"{"code":9,"msg":"人机验证"}"#);
    let status = service.run(None).unwrap();
    assert_eq!(
        status.accounts[0].attention.as_ref().unwrap().kind,
        FailureKind::Verification
    );
    assert_eq!(server.hits("/openApi/secureLogin"), 2);
    assert!(service.run(None).unwrap().accounts[0].attention.is_some());
    assert_eq!(server.hits("/openApi/secureLogin"), 2);
}
#[test]
fn failed_keyring_rotation_prevents_signin_requests() {
    let (_dir, server, vault, service) = fixture();
    service
        .login("13800138000", "test-password", false)
        .unwrap();
    vault.fail_save.store(true, Ordering::Relaxed);
    let status = service.run(None).unwrap();
    assert!(!status.accounts[0].last_report.as_ref().unwrap().ok);
    assert_eq!(server.hits("/apihub/api/signin"), 0);
    assert_eq!(
        vault.load_all().unwrap().remove(0).refresh_token,
        "refresh-secret"
    );
}
#[test]
fn automatic_runs_serialize_and_share_daily_deduplication() {
    let (dir, server, vault, service) = fixture();
    service
        .login("13800138000", "test-password", false)
        .unwrap();
    service.set_trigger(Trigger::Launch);
    let other = Service::testing(dir.path(), vault, &server.base());
    let worker = std::thread::spawn(move || other.run(Some(Trigger::Launch)).unwrap());
    assert!(
        service.run(Some(Trigger::Launch)).unwrap().accounts[0]
            .last_report
            .as_ref()
            .unwrap()
            .ok
    );
    assert!(
        worker.join().unwrap().accounts[0]
            .last_report
            .as_ref()
            .unwrap()
            .ok
    );
    assert_eq!(server.hits("/usercenter/api/refreshToken"), 1);
}
#[test]
fn duplicate_response_requires_confirmed_state_and_empty_roles_fail() {
    let (_dir, server, _vault, service) = fixture();
    service
        .login("13800138000", "test-password", false)
        .unwrap();
    server.route("/apihub/awapi/sign", r#"{"code":1,"msg":"重复签到"}"#);
    server.route(
        "/apihub/awapi/signin/state",
        r#"{"code":0,"data":{"todaySign":false}}"#,
    );
    assert!(
        !service.run(None).unwrap().accounts[0]
            .last_report
            .as_ref()
            .unwrap()
            .ok
    );
    server.route(
        "/apihub/awapi/signin/state",
        r#"{"code":0,"data":{"todaySign":true}}"#,
    );
    assert!(
        service.run(None).unwrap().accounts[0]
            .last_report
            .as_ref()
            .unwrap()
            .ok
    );
    service
        .login("13800138000", "test-password", false)
        .unwrap();
    server.route(
        "/usercenter/api/v2/getGameRoles",
        r#"{"code":0,"data":{"roles":[]}}"#,
    );
    assert!(
        !service.run(None).unwrap().accounts[0]
            .last_report
            .as_ref()
            .unwrap()
            .ok
    );
}

#[test]
fn daily_configuration_updates_units_and_restores_preferences_on_failure() {
    let dir = TempDir::new("attendance-preferences");
    let (_data, _server, _vault, service) = fixture();
    let mut env = crate::testing::EnvGuard::new();
    env.set("XDG_CONFIG_HOME", dir.path());
    env.set("PATH", dir.path());
    env.set("ODDITY_TEST_FAIL", "0");
    crate::testing::write_script(dir.path(),"systemctl","#!/bin/sh\ncase \"$*\" in *enable*) if [ \"$ODDITY_TEST_FAIL\" = 1 ]; then exit 1; fi;; esac\nexit 0\n");
    assert!(service
        .configure(
            Preferences {
                trigger: Trigger::Launch,
                ..Preferences::default()
            },
            std::path::Path::new("/oddity")
        )
        .is_err());
    service
        .login("13800138000", "test-password", false)
        .unwrap();
    let daily = Preferences {
        trigger: Trigger::Daily,
        time: "19:30".into(),
    };
    service
        .configure(daily.clone(), std::path::Path::new("/oddity"))
        .unwrap();
    assert!(dir.is_file("systemd/user/oddity-taygedo.timer"));
    service
        .configure(Preferences::default(), std::path::Path::new("/oddity"))
        .unwrap();
    assert!(!dir.is_file("systemd/user/oddity-taygedo.timer"));
    for trigger in [Trigger::Daily, Trigger::Boot] {
        env.set("ODDITY_TEST_FAIL", "1");
        assert!(service
            .configure(
                Preferences {
                    trigger,
                    ..daily.clone()
                },
                std::path::Path::new("/oddity")
            )
            .is_err());
        assert_eq!(service.status().unwrap().preferences.trigger, Trigger::Off);
        assert!(!dir.is_file("systemd/user/oddity-taygedo.timer"));
        assert!(!dir.is_file("systemd/user/oddity-taygedo-boot.timer"));
        assert!(!dir.is_file("systemd/user/oddity-taygedo.service"));
    }
}

#[test]
fn multiple_logins_preserve_order_and_reuse_device_on_relogin() {
    let (_dir, _server, vault, service) = fixture();
    service
        .login("13800138000", "first-password", true)
        .unwrap();
    let device = vault.load_all().unwrap()[0].device_id.clone();
    let status = service
        .login("13900139000", "second-password", false)
        .unwrap();
    assert_eq!(status.accounts.len(), 2);
    assert_eq!(status.accounts[0].id, account_id("13800138000"));
    assert_eq!(status.accounts[1].id, account_id("13900139000"));
    assert_eq!(vault.load_all().unwrap().len(), 2);
    service.run(None).unwrap();
    let status = service.login("13800138000", "replacement", false).unwrap();
    assert_eq!(status.accounts.len(), 2);
    assert!(!status.accounts[0].remember_password);
    assert!(!status.accounts[0].signed_today);
    assert!(status.accounts[0].last_report.is_none());
    assert!(status.accounts[0].attention.is_none());
    assert!(status.accounts[1].signed_today);
    assert_eq!(vault.load_all().unwrap()[0].device_id, device);
}

#[test]
fn all_accounts_sign_and_successes_are_skipped_in_manual_and_automatic_runs() {
    let (_dir, server, _vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    service.login("13900139000", "secret", false).unwrap();
    service.set_trigger(Trigger::Launch);
    let status = service.run(None).unwrap();
    assert!(status.accounts.iter().all(|a| a.signed_today));
    assert_eq!(server.hits("/apihub/awapi/sign"), 4);
    service.run(None).unwrap();
    service.run(Some(Trigger::Launch)).unwrap();
    assert_eq!(server.hits("/apihub/awapi/sign"), 4);
    service.login("13900139000", "secret", false).unwrap();
    service.run(Some(Trigger::Launch)).unwrap();
    assert_eq!(server.hits("/apihub/awapi/sign"), 6);
    service.login("13900139000", "secret", false).unwrap();
    service.run(None).unwrap();
    assert_eq!(server.hits("/apihub/awapi/sign"), 8);
}

#[test]
fn verification_only_pauses_affected_account() {
    let (dir, server, _vault, service) = fixture();
    service.login("13800138000", "secret", true).unwrap();
    server.route(
        "/usercenter/api/refreshToken",
        r#"{"code":7,"msg":"人机验证 private-secret"}"#,
    );
    let first = service.run(None).unwrap();
    assert_eq!(
        first.accounts[0].attention.as_ref().unwrap().kind,
        FailureKind::Verification
    );
    service.login("13900139000", "secret", false).unwrap();
    server.route(
        "/usercenter/api/refreshToken",
        r#"{"code":0,"data":{"accessToken":"new-access","refreshToken":"new-refresh"}}"#,
    );
    let status = service.run(None).unwrap();
    assert_eq!(
        status.accounts[0].attention.as_ref().unwrap().kind,
        FailureKind::Verification
    );
    assert!(!status.accounts[0].last_report.as_ref().unwrap().ok);
    assert!(status.accounts[1].signed_today);
    assert_eq!(server.hits("/usercenter/api/refreshToken"), 2);
    assert!(!dir.read_text("state.json").contains("private-secret"));
}

#[test]
fn logout_removes_only_target_and_resets_preferences_for_last_account() {
    let (_dir, _server, vault, service) = fixture();
    let mut env = crate::testing::EnvGuard::new();
    let config = TempDir::new("multi-logout");
    env.set("XDG_CONFIG_HOME", config.path());
    service.login("13800138000", "secret", false).unwrap();
    service.login("13900139000", "secret", false).unwrap();
    service.set_trigger(Trigger::Launch);
    let status = service
        .logout(&account_id("13800138000"), std::path::Path::new("/oddity"))
        .unwrap();
    assert_eq!(status.accounts.len(), 1);
    assert_eq!(status.accounts[0].id, account_id("13900139000"));
    assert_eq!(status.preferences.trigger, Trigger::Launch);
    assert_eq!(vault.load_all().unwrap().len(), 1);
    let status = service
        .logout(&account_id("13900139000"), std::path::Path::new("/oddity"))
        .unwrap();
    assert!(status.accounts.is_empty());
    assert_eq!(status.preferences.trigger, Trigger::Off);
    assert!(vault.load_all().unwrap().is_empty());
}

fn legacy_state(at: u64) -> serde_json::Value {
    serde_json::json!({
        "configured": true, "maskedPhone": "138****8000", "rememberPassword": true,
        "preferences": { "trigger": "launch", "time": "09:30" },
        "lastReport": { "at": at, "ok": true, "steps": [] },
        "attention": { "kind": "auth", "message": "请重新登录" }
    })
}

#[test]
fn legacy_state_migrates_once_and_preserves_login_report_and_attention() {
    let (dir, _server, vault, service) = fixture();
    service.login("13800138000", "secret", true).unwrap();
    let at = now();
    dir.write("state.json", serde_json::to_vec(&legacy_state(at)).unwrap());
    let status = service.status().unwrap();
    assert_eq!(status.accounts.len(), 1);
    let account = &status.accounts[0];
    assert_eq!(account.id, account_id("13800138000"));
    assert_eq!(account.masked_phone, "138****8000");
    assert!(account.remember_password);
    assert!(account.signed_today);
    assert_eq!(account.last_report.as_ref().unwrap().at, at);
    assert_eq!(account.attention.as_ref().unwrap().kind, FailureKind::Auth);
    assert_eq!(status.preferences.trigger, Trigger::Launch);
    assert_eq!(status.preferences.time, "09:30");
    let saved = dir.read_text("state.json");
    assert!(!saved.contains("configured"));
    assert!(!saved.contains("secret"));
    vault.fail.store(true, Ordering::Relaxed);
    assert_eq!(service.status().unwrap().accounts.len(), 1);
}

#[test]
fn busy_migration_returns_status_without_writing_and_retries_later() {
    let (dir, _server, _vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    let legacy = serde_json::to_vec(&legacy_state(now())).unwrap();
    dir.write("state.json", &legacy);
    let lock = std::fs::File::open(dir.join("lock")).unwrap();
    lock.lock().unwrap();
    assert_eq!(service.status().unwrap().accounts.len(), 1);
    assert_eq!(dir.read("state.json"), legacy);
    lock.unlock().unwrap();
    drop(lock);
    service.status().unwrap();
    assert!(!dir.read_text("state.json").contains("configured"));
}

#[test]
fn day_status_is_recomputed_from_china_day_and_report_result() {
    let (dir, _server, vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    let mut status = service.status().unwrap();
    vault.fail.store(true, Ordering::Relaxed);
    for (at, ok, signed, failed) in [
        (now() - 86400, true, false, false),
        (now() - 86400, false, false, false),
        (now(), true, true, false),
        (now(), false, false, true),
    ] {
        status.accounts[0].signed_today = !signed;
        status.accounts[0].failed_today = !failed;
        status.accounts[0].last_report = Some(Report {
            at,
            ok,
            steps: vec![],
        });
        dir.write("state.json", serde_json::to_vec(&status).unwrap());
        let current = service.status().unwrap().accounts[0].clone();
        assert_eq!(current.signed_today, signed);
        assert_eq!(current.failed_today, failed);
    }
}

#[test]
fn automatic_empty_is_quiet_and_manual_empty_requires_login() {
    let (_dir, _server, vault, service) = fixture();
    service.set_trigger(Trigger::Launch);
    vault.fail.store(true, Ordering::Relaxed);
    assert!(service
        .run(Some(Trigger::Launch))
        .unwrap()
        .accounts
        .is_empty());
    assert_eq!(service.run(None).unwrap_err().kind, FailureKind::Auth);
}

#[test]
fn one_account_sign_failure_does_not_stop_next_account() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    let (dir, _server, vault, login_service) = fixture();
    login_service.login("13800138000", "secret", false).unwrap();
    login_service.login("13900139000", "secret", false).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut requests = 0;
        let mut communities = 0;
        while requests < 16 && Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(value) => value,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let path = line.split_whitespace().nth(1).unwrap().to_string();
            let mut content_length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header == "\r\n" || header.is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = value.trim().parse::<usize>().unwrap();
                    }
                }
            }
            let mut request_body = vec![0; content_length];
            reader.read_exact(&mut request_body).unwrap();
            let body = if path == "/usercenter/api/refreshToken" {
                r#"{"code":0,"data":{"accessToken":"rotated-access","refreshToken":"rotated-refresh"}}"#
            } else if path == "/apihub/api/signin" {
                communities += 1;
                if communities == 1 {
                    r#"{"code":500,"msg":"private-value"}"#
                } else {
                    r#"{"code":0}"#
                }
            } else if path.starts_with("/usercenter/api/v2/getGameRoles") {
                r#"{"code":0,"data":{"roles":[{"roleId":"one"},{"roleId":"two"}]}}"#
            } else if path.starts_with("/apihub/api/getUserTasks") {
                completed_tasks()
            } else if path == "/apihub/api/getUserCoinTaskState" {
                r#"{"code":0,"data":{"todayCoin":110}}"#
            } else if path == "/cloud/game/getUserInfo" {
                r#"{"code":0,"result":{"perDayFirstLoginGiveDuration":0}}"#
            } else {
                assert_eq!(path, "/apihub/awapi/sign");
                r#"{"code":0}"#
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            stream.flush().unwrap();
            requests += 1;
        }
        requests
    });
    let service = Service::testing(dir.path(), vault, &base);
    let status = service.run(None).unwrap();
    assert_eq!(worker.join().unwrap(), 16);
    assert!(!status.accounts[0].signed_today);
    let first = status.accounts[0].last_report.as_ref().unwrap();
    assert!(!first.ok);
    assert!(!first.steps[0].ok);
    assert!(first.steps[1..].iter().all(|s| s.ok));
    assert!(status.accounts[0].attention.is_none());
    assert!(status.accounts[1].signed_today);
    assert!(status.accounts[1].last_report.as_ref().unwrap().ok);
    assert!(!dir.read_text("state.json").contains("private-value"));
}

#[test]
fn run_reconciles_state_with_vault_order_and_missing_accounts() {
    let (_dir, _server, vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    service.login("13900139000", "secret", false).unwrap();
    let mut accounts = vault.account.lock().unwrap();
    accounts.reverse();
    let mut extra = accounts[0].clone();
    extra.phone = "13700137000".into();
    accounts.push(extra);
    accounts.retain(|a| a.phone != "13800138000");
    drop(accounts);
    let status = service.run(None).unwrap();
    assert_eq!(status.accounts.len(), 2);
    assert_eq!(status.accounts[0].id, account_id("13900139000"));
    assert_eq!(status.accounts[1].id, account_id("13700137000"));
    assert!(status.accounts.iter().all(|a| a.signed_today));
}

#[test]
fn migration_attaches_legacy_report_and_attention_only_to_first_account() {
    let (dir, _server, _vault, service) = fixture();
    service.login("13800138000", "secret", true).unwrap();
    service.login("13900139000", "secret", false).unwrap();
    dir.write(
        "state.json",
        serde_json::to_vec(&legacy_state(now())).unwrap(),
    );
    let status = service.status().unwrap();
    assert_eq!(status.accounts.len(), 2);
    assert!(status.accounts[0].last_report.is_some());
    assert!(status.accounts[0].attention.is_some());
    assert!(status.accounts[0].remember_password);
    assert!(status.accounts[1].last_report.is_none());
    assert!(status.accounts[1].attention.is_none());
    assert!(!status.accounts[1].remember_password);
}

#[test]
fn boot_runs_only_for_matching_trigger_and_signs_all_accounts() {
    let (_dir, server, _vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    service.login("13900139000", "secret", false).unwrap();
    service.set_trigger(Trigger::Boot);
    service.run(Some(Trigger::Daily)).unwrap();
    service.run(Some(Trigger::Launch)).unwrap();
    assert_eq!(server.hits("/usercenter/api/refreshToken"), 0);
    assert_eq!(serde_json::to_string(&Trigger::Boot).unwrap(), "\"boot\"");
    assert!(service
        .run(Some(Trigger::Boot))
        .unwrap()
        .accounts
        .iter()
        .all(|a| a.signed_today));
    service.run(Some(Trigger::Boot)).unwrap();
    assert_eq!(server.hits("/apihub/awapi/sign"), 4);
}

fn sms_routes(server: &HttpServer) {
    server.route("/m/newApi/sendPhoneCaptchaWithOutLogin", r#"{"code":0}"#);
    server.route("/m/newApi/checkPhoneCaptchaWithOutLogin", r#"{"code":0}"#);
    server.route(
        "/openApi/sms/new/login",
        r#"{"code":0,"result":{"token":"sms-token-secret","userId":"sms-user"}}"#,
    );
}

#[test]
fn sms_login_creates_and_resumes_account_without_storing_code_or_tokens_in_state() {
    let (dir, server, vault, service) = fixture();
    sms_routes(&server);
    let device = service.send_code("13800138000").unwrap();
    assert!(!device.is_empty());
    assert_eq!(server.hits("/m/newApi/sendPhoneCaptchaWithOutLogin"), 1);
    let status = service
        .login_sms("13800138000", " 654321 ", &device)
        .unwrap();
    assert_eq!(status.accounts.len(), 1);
    assert!(!status.accounts[0].remember_password);
    assert!(vault.load_all().unwrap()[0].password.is_none());
    assert_eq!(vault.load_all().unwrap()[0].device_id, device);
    service.run(None).unwrap();
    server.route(
        "/usercenter/api/refreshToken",
        r#"{"code":7,"msg":"人机验证"}"#,
    );
    service.login_sms("13800138000", "654321", &device).unwrap();
    assert!(service.run(None).unwrap().accounts[0].attention.is_some());
    assert_eq!(service.send_code("13800138000").unwrap(), device);
    let status = service.login_sms("13800138000", "654321", &device).unwrap();
    assert_eq!(status.accounts.len(), 1);
    assert!(status.accounts[0].last_report.is_none());
    assert!(status.accounts[0].attention.is_none());
    assert!(!status.accounts[0].signed_today);
    assert_eq!(server.hits("/m/newApi/checkPhoneCaptchaWithOutLogin"), 3);
    assert_eq!(server.hits("/openApi/sms/new/login"), 3);
    for text in [
        dir.read_text("state.json"),
        serde_json::to_string(&status).unwrap(),
    ] {
        for secret in [
            "654321",
            "sms-token-secret",
            "refresh-secret",
            "access-secret",
            "13800138000",
        ] {
            assert!(!text.contains(secret));
        }
    }
}

#[test]
fn sms_login_preserves_saved_password() {
    let (_dir, server, vault, service) = fixture();
    sms_routes(&server);
    service
        .login("13800138000", "saved-password", true)
        .unwrap();
    let device = service.send_code("13800138000").unwrap();
    let status = service.login_sms("13800138000", "123456", &device).unwrap();
    assert!(status.accounts[0].remember_password);
    assert_eq!(
        vault.load_all().unwrap()[0].password.as_deref(),
        Some("saved-password")
    );
}

#[test]
fn sms_business_failures_are_friendly_and_check_precedes_login() {
    let (_dir, server, vault, service) = fixture();
    sms_routes(&server);
    server.route(
        "/m/newApi/sendPhoneCaptchaWithOutLogin",
        r#"{"code":1,"msg":"短信正在发送"}"#,
    );
    let device = service.send_code("13800138000").unwrap();
    server.route(
        "/m/newApi/checkPhoneCaptchaWithOutLogin",
        r#"{"code":7,"msg":"短信验证码 private-secret"}"#,
    );
    let error = service
        .login_sms("13800138000", "123456", &device)
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Verification);
    assert_eq!(error.message, "短信验证码错误或已过期, 请重新获取.");
    assert!(vault.load_all().unwrap().is_empty());
    assert_eq!(server.hits("/openApi/sms/new/login"), 0);
    for (msg, expected) in [
        ("操作频繁 private-secret", "短信发送过于频繁, 请稍后再试."),
        (
            "短信验证码 private-secret",
            "短信验证码发送失败, 请稍后重试.",
        ),
    ] {
        server.route(
            "/m/newApi/sendPhoneCaptchaWithOutLogin",
            serde_json::json!({"code":7,"msg":msg}).to_string(),
        );
        assert_eq!(
            service.send_code("13800138000").unwrap_err().message,
            expected
        );
    }
}

#[test]
fn invalid_sms_inputs_and_keyring_failure_make_no_requests() {
    let (_dir, server, vault, service) = fixture();
    assert!(service.send_code("bad").is_err());
    for (code, device) in [
        ("", "device"),
        ("123\n456", "device"),
        ("12345678901234567", "device"),
        ("123", ""),
        ("123", "bad\ndevice"),
    ] {
        assert!(service.login_sms("13800138000", code, device).is_err());
    }
    assert!(service
        .login_sms("13800138000", "123", &"d".repeat(129))
        .is_err());
    vault.fail.store(true, Ordering::Relaxed);
    assert!(service.send_code("13800138000").is_err());
    assert!(service.login_sms("13800138000", "123", "device").is_err());
    assert!(server.targets().is_empty());
}

#[test]
fn daily_coin_tasks_and_cloud_duration_run_once_and_record_results() {
    let (dir, server, _vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    server.route(
        "/apihub/api/getUserTasks",
        r#"{"code":0,"data":{"task_list1":[
        {"taskKey":"signin_c","completeTimes":"0","limitTimes":"1"},
        {"code":"browse_post_c","completeTimes":3,"limitTimes":5},
        {"code":"like_post_c","completeTimes":4,"limitTimes":5},
        {"code":"share","completeTimes":0,"limitTimes":1}
    ]}}"#,
    );
    server.route(
        "/bbs/api/getRecommendPostList",
        r#"{"code":0,"data":{"posts":[
        {"postId":"first"},{"postId":"first"},{"id":"second"}
    ]}}"#,
    );
    server.route(
        "/bbs/api/getPostFull",
        r#"{"code":0,"data":{"post":{},"selfOperation":{"liked":false}}}"#,
    );
    server.route("/bbs/api/post/like", r#"{"code":0}"#);
    server.route("/bbs/api/post/share", r#"{"code":0}"#);
    service.set_trigger(Trigger::Launch);
    let status = service.run(None).unwrap();
    let report = status.accounts[0].last_report.as_ref().unwrap();
    assert!(status.accounts[0].signed_today);
    assert!(report.steps.iter().all(|s| s.ok));
    assert_eq!(server.hits("/apihub/api/signin"), 2);
    assert_eq!(server.hits("/bbs/api/getPostFull"), 2);
    assert_eq!(server.hits("/bbs/api/post/like"), 1);
    assert_eq!(server.hits("/bbs/api/post/share"), 1);
    assert!(report.steps.iter().any(|s| s.message == "今日金币 110/150"));
    assert!(report
        .steps
        .iter()
        .any(|s| s.message == "本次赠送 15 分钟, 剩余 120 分钟"));
    service.run(None).unwrap();
    service.run(Some(Trigger::Launch)).unwrap();
    assert_eq!(server.hits("/cloud/game/getUserInfo"), 1);
    assert_eq!(server.hits("/apihub/api/getUserTasks"), 1);
    let saved = dir.read_text("state.json");
    for secret in [
        "13800138000",
        "secret",
        "new-access",
        "new-refresh",
        "9007199254740993",
    ] {
        assert!(!saved.contains(secret));
    }
}

#[test]
fn completed_coin_tasks_only_read_state_and_do_not_fetch_posts() {
    let (_dir, server, _vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    let status = service.run(None).unwrap();
    assert!(status.accounts[0].signed_today);
    assert_eq!(server.hits("/apihub/api/signin"), 1);
    assert_eq!(server.hits("/bbs/api/getRecommendPostList"), 0);
    assert_eq!(server.hits("/apihub/api/getUserCoinTaskState"), 1);
    assert_eq!(server.hits("/openApi/secureLogin"), 1);
}

#[test]
fn liked_posts_are_not_toggled_and_incomplete_actions_remain_visible() {
    let (_dir, server, _vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    server.route(
        "/apihub/api/getUserTasks",
        r#"{"code":0,"data":{"task_list1":[]}}"#,
    );
    server.route(
        "/bbs/api/getRecommendPostList",
        r#"{"code":0,"data":[
        {"postId":"first","selfOperation":{"liked":false}},
        {"postId":"second","selfOperation":{"liked":true}}
    ]}"#,
    );
    server.route(
        "/bbs/api/getPostFull",
        r#"{"code":0,"data":{"post":{},"selfOperation":{"liked":true}}}"#,
    );
    server.route("/bbs/api/post/share", r#"{"code":0}"#);
    let status = service.run(None).unwrap();
    assert!(status.accounts[0].signed_today);
    assert!(status.accounts[0].attention.is_none());
    assert_eq!(server.hits("/bbs/api/post/like"), 0);
    assert_eq!(server.hits("/bbs/api/post/share"), 1);
    let steps = &status.accounts[0].last_report.as_ref().unwrap().steps;
    assert!(steps
        .iter()
        .any(|s| s.label == "浏览帖子" && !s.ok && s.message.contains("2/5")));
    assert!(steps
        .iter()
        .any(|s| s.label == "点赞帖子" && !s.ok && s.message.contains("0/5")));
}

#[test]
fn empty_recommendations_do_not_claim_coin_actions_succeeded() {
    let (_dir, server, _vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    server.route(
        "/apihub/api/getUserTasks",
        r#"{"code":0,"data":{"task_list1":[]}}"#,
    );
    server.route(
        "/bbs/api/getRecommendPostList",
        r#"{"code":0,"data":{"list":[]}}"#,
    );
    let status = service.run(None).unwrap();
    assert!(status.accounts[0].signed_today);
    let steps = &status.accounts[0].last_report.as_ref().unwrap().steps;
    for label in ["浏览帖子", "点赞帖子", "分享帖子"] {
        assert!(steps
            .iter()
            .any(|s| s.label == label && !s.ok && s.optional));
    }
    assert_eq!(server.hits("/apihub/api/getUserCoinTaskState"), 1);
    assert_eq!(server.hits("/cloud/game/getUserInfo"), 1);
}

#[test]
fn optional_authentication_failures_do_not_pause_core_or_other_accounts() {
    let (dir, server, _vault, service) = fixture();
    service.login("13800138000", "secret", true).unwrap();
    service.login("13900139000", "secret", false).unwrap();
    server.route(
        "/apihub/api/getUserTasks",
        r#"{"code":1,"msg":"token 已过期 private-value"}"#,
    );
    server.route(
        "/cloud/game/getUserInfo",
        r#"{"code":7,"message":"需要人工验证 private-token"}"#,
    );
    let status = service.run(None).unwrap();
    assert!(status
        .accounts
        .iter()
        .all(|a| a.signed_today && a.attention.is_none()));
    assert_eq!(server.hits("/apihub/awapi/sign"), 4);
    assert_eq!(server.hits("/apihub/api/getUserCoinTaskState"), 2);
    assert_eq!(server.hits("/cloud/game/getUserInfo"), 2);
    assert_eq!(server.hits("/openApi/secureLogin"), 2);
    for account in &status.accounts {
        let steps = &account.last_report.as_ref().unwrap().steps;
        assert!(steps
            .iter()
            .any(|s| !s.ok && s.optional && s.kind == Some(FailureKind::Auth)));
        assert!(steps
            .iter()
            .any(|s| !s.ok && s.optional && s.kind == Some(FailureKind::Verification)));
    }
    assert!(!dir.read_text("state.json").contains("private-"));
}

#[test]
fn missing_cloud_credentials_without_saved_password_skip_only_cloud() {
    let (_dir, server, vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    {
        let mut accounts = vault.account.lock().unwrap();
        accounts[0].laohu_token.clear();
        accounts[0].laohu_user_id.clear();
    }
    let status = service.run(None).unwrap();
    assert!(status.accounts[0].signed_today);
    assert_eq!(server.hits("/cloud/game/getUserInfo"), 0);
    assert_eq!(server.hits("/openApi/secureLogin"), 1);
    assert_eq!(server.hits("/apihub/api/getUserCoinTaskState"), 1);
    assert!(status.accounts[0]
        .last_report
        .as_ref()
        .unwrap()
        .steps
        .iter()
        .any(|s| s.label == "云异环时长" && s.message.starts_with("已跳过:")));
}

#[test]
fn missing_cloud_token_is_restored_and_saved_even_when_cloud_request_fails() {
    let (dir, server, vault, service) = fixture();
    service
        .login("13800138000", "saved-password", true)
        .unwrap();
    vault.account.lock().unwrap()[0].laohu_token.clear();
    server.route(
        "/openApi/secureLogin",
        r#"{"code":0,"result":{"token":"new-laohu-secret","userId":"new-user"}}"#,
    );
    server.route(
        "/cloud/game/getUserInfo",
        r#"{"code":1,"msg":"服务错误 private-value"}"#,
    );
    let status = service.run(None).unwrap();
    assert!(status.accounts[0].signed_today);
    assert!(status.accounts[0].attention.is_none());
    assert_eq!(server.hits("/openApi/secureLogin"), 2);
    assert_eq!(server.hits("/usercenter/api/login"), 1);
    assert_eq!(vault.load_all().unwrap()[0].laohu_token, "new-laohu-secret");
    assert_eq!(vault.load_all().unwrap()[0].laohu_user_id, "new-user");
    assert!(!dir.read_text("state.json").contains("new-laohu-secret"));
    assert!(
        !status.accounts[0]
            .last_report
            .as_ref()
            .unwrap()
            .steps
            .last()
            .unwrap()
            .ok
    );
}

#[test]
fn cloud_password_verification_does_not_invalidate_taygedo_session() {
    let (_dir, server, vault, service) = fixture();
    service
        .login("13800138000", "saved-password", true)
        .unwrap();
    vault.account.lock().unwrap()[0].laohu_user_id.clear();
    server.route(
        "/openApi/secureLogin",
        r#"{"code":7,"message":"人机验证 private-value"}"#,
    );
    let status = service.run(None).unwrap();
    assert!(status.accounts[0].signed_today);
    assert!(status.accounts[0].attention.is_none());
    assert_eq!(server.hits("/cloud/game/getUserInfo"), 0);
    assert_eq!(
        status.accounts[0]
            .last_report
            .as_ref()
            .unwrap()
            .steps
            .last()
            .unwrap()
            .kind,
        Some(FailureKind::Verification)
    );
}

#[test]
fn game_role_names_are_preserved_and_duplicate_ids_sign_once() {
    let (dir, server, _vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    server.route(
        "/usercenter/api/v2/getGameRoles",
        r#"{"code":0,"data":{"roles":[
        {"roleId":9007199254740993},
        {"roleId":"9007199254740993","roleName":" 测试角色甲 "},
        {"roleId":"role-2","roleName":""},
        {"roleId":"role-2","roleName":"测试角色乙"},
        {"roleName":"没有 ID"}
    ]}}"#,
    );
    let status = service.run(None).unwrap();
    assert!(status.accounts[0].signed_today);
    let roles: Vec<_> = status.accounts[0]
        .last_report
        .as_ref()
        .unwrap()
        .steps
        .iter()
        .filter(|step| step.label.starts_with("角色 "))
        .collect();
    assert_eq!(roles.len(), 2);
    assert_eq!(roles[0].label, "角色 测试角色甲");
    assert_eq!(roles[1].label, "角色 测试角色乙");
    assert_eq!(server.hits("/apihub/awapi/sign"), 2);
    assert!(dir.read_text("state.json").contains("角色 测试角色甲"));
}

#[test]
fn unnamed_role_uses_id_and_failed_signin_keeps_role_name() {
    let (_dir, server, _vault, service) = fixture();
    service.login("13800138000", "secret", false).unwrap();
    server.route(
        "/usercenter/api/v2/getGameRoles",
        r#"{"code":0,"data":{"roles":[
        {"roleId":"role-a","roleName":"测试角色甲"},
        {"roleId":"role-b","roleName":" \n "},
        {"roleId":9007199254740993}
    ]}}"#,
    );
    server.route("/apihub/awapi/sign", r#"{"code":500,"msg":"服务错误"}"#);
    let status = service.run(None).unwrap();
    assert!(!status.accounts[0].signed_today);
    let roles: Vec<_> = status.accounts[0]
        .last_report
        .as_ref()
        .unwrap()
        .steps
        .iter()
        .filter(|step| step.label.starts_with("角色 "))
        .collect();
    assert_eq!(roles.len(), 3);
    assert!(roles.iter().all(|step| !step.ok));
    assert!(roles.iter().any(|step| step.label == "角色 测试角色甲"));
    assert!(roles.iter().any(|step| step.label == "角色 role-b"));
    assert!(roles
        .iter()
        .any(|step| step.label == "角色 9007199254740993"));
}
