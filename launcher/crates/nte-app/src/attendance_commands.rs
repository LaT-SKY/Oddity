//! 密钥环及网络操作放到后台线程, 命令只返回脱敏状态。
use nte_core::attendance::{
    self, Failure, FailureKind, Preferences, Result, Service, Status, Trigger,
};

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|_| Failure {
            kind: FailureKind::Service,
            message: "签到后台任务异常".into(),
        })?
}
fn executable() -> Result<std::path::PathBuf> {
    std::env::current_exe().map_err(|_| Failure {
        kind: FailureKind::Service,
        message: "无法确定启动器路径".into(),
    })
}
#[tauri::command]
pub async fn attendance_status() -> Result<Status> {
    blocking(|| attendance::default_service().status()).await
}
#[tauri::command]
pub async fn attendance_login(
    phone: String,
    password: String,
    remember_password: bool,
) -> Result<Status> {
    let password = zeroize::Zeroizing::new(password);
    blocking(move || {
        attendance::default_service().login(phone.trim(), &password, remember_password)
    })
    .await
}
#[tauri::command]
pub async fn attendance_send_code(phone: String) -> Result<String> {
    blocking(move || attendance::default_service().send_code(phone.trim())).await
}
#[tauri::command]
pub async fn attendance_login_sms(
    phone: String,
    code: String,
    device_id: String,
) -> Result<Status> {
    let code = zeroize::Zeroizing::new(code);
    blocking(move || attendance::default_service().login_sms(phone.trim(), &code, &device_id)).await
}
#[tauri::command]
pub async fn attendance_configure(preferences: Preferences) -> Result<Status> {
    blocking(move || attendance::default_service().configure(preferences, &executable()?)).await
}
#[tauri::command]
pub async fn attendance_run() -> Result<Status> {
    blocking(|| attendance::default_service().run(None)).await
}
#[tauri::command]
pub async fn attendance_logout(account_id: String) -> Result<Status> {
    blocking(move || attendance::default_service().logout(&account_id, &executable()?)).await
}
#[tauri::command]
pub async fn attendance_keyring_check() -> Result<()> {
    blocking(attendance::check_keyring).await
}

pub fn on_launch() {
    if attendance::default_service()
        .status()
        .is_ok_and(|s| s.preferences.trigger == Trigger::Launch)
    {
        tauri::async_runtime::spawn_blocking(|| {
            let _ = attendance::default_service().run(Some(Trigger::Launch));
        });
    }
}
pub fn headless_entry() -> bool {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !args.iter().any(|a| a == "--taygedo-attendance") {
        return false;
    }
    let dir = args
        .windows(2)
        .find(|a| a[0] == "--taygedo-config")
        .map(|a| std::path::PathBuf::from(&a[1]))
        .unwrap_or_else(attendance::default_dir);
    let service = Service::new(dir);
    let result = (|| -> Result<()> {
        let current = service.status()?;
        if current.accounts.is_empty() {
            return Err(Failure {
                kind: FailureKind::Auth,
                message: "请先登录塔吉多账号".into(),
            });
        }
        let trigger = current.preferences.trigger;
        if !matches!(trigger, Trigger::Daily | Trigger::Boot) {
            return Ok(());
        }
        let status = service.run(Some(trigger))?;
        if status.accounts.is_empty() {
            return Err(Failure {
                kind: FailureKind::Auth,
                message: "请先登录塔吉多账号".into(),
            });
        }
        let mut failed = false;
        for account in status.accounts {
            if let Some(error) = &account.attention {
                eprintln!("{}: {}", account.masked_phone, error);
                failed = true;
            }
            if let Some(report) = &account.last_report {
                if !report.ok {
                    failed = true;
                }
                for step in &report.steps {
                    println!("{}: {}: {}", account.masked_phone, step.label, step.message);
                }
            }
            if account.signed_today {
                println!("{}: 今日已签到", account.masked_phone);
            } else {
                failed = true;
            }
        }
        if failed {
            return Err(Failure {
                kind: FailureKind::Service,
                message: "签到未全部成功".into(),
            });
        }
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
    true
}
