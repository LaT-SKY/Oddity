//! 塔吉多登录和可选签到, 所有入口共享会话、锁和执行结果。
mod client;
pub mod schedule;
mod service;
mod vault;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub use service::Service;
pub use vault::{check_keyring, KeyringVault, Vault};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureKind {
    Network,
    Auth,
    Verification,
    Keyring,
    Storage,
    Busy,
    Protocol,
    Service,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
}
impl Failure {
    fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    fn at(mut self, stage: &str) -> Self {
        self.message = format!("{stage}: {}", self.message);
        self
    }
    fn needs_login(&self) -> bool {
        matches!(self.kind, FailureKind::Auth | FailureKind::Verification)
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}
pub type Result<T> = std::result::Result<T, Failure>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub phone: String,
    pub password: Option<String>,
    pub device_id: String,
    pub uid: String,
    pub refresh_token: String,
    pub laohu_token: String,
    pub laohu_user_id: String,
}
impl Drop for Account {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.phone.zeroize();
        self.password.zeroize();
        self.refresh_token.zeroize();
        self.laohu_token.zeroize();
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    #[default]
    Off,
    Launch,
    Boot,
    Daily,
    Manual,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    pub trigger: Trigger,
    pub time: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            trigger: Trigger::Off,
            time: "08:00".into(),
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<()> {
        let b = self.time.as_bytes();
        if b.len() != 5
            || b[2] != b':'
            || !b
                .iter()
                .enumerate()
                .all(|(i, b)| i == 2 || b.is_ascii_digit())
            || self.time[..2].parse::<u8>().unwrap_or(24) > 23
            || self.time[3..].parse::<u8>().unwrap_or(60) > 59
        {
            return Err(Failure::new(
                FailureKind::Protocol,
                "签到时间必须为 HH:MM 格式",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub label: String,
    pub ok: bool,
    pub message: String,
    pub kind: Option<FailureKind>,
}
impl Step {
    fn from_result(label: impl Into<String>, result: Result<()>) -> Self {
        match result {
            Ok(()) => Self {
                label: label.into(),
                ok: true,
                message: "签到完成或今日已签到".into(),
                kind: None,
            },
            Err(e) => Self {
                label: label.into(),
                ok: false,
                message: e.message.clone(),
                kind: Some(e.kind),
            },
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub at: u64,
    pub ok: bool,
    pub steps: Vec<Step>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub id: String,
    pub masked_phone: String,
    pub remember_password: bool,
    pub signed_today: bool,
    #[serde(default)]
    pub failed_today: bool,
    pub last_report: Option<Report>,
    pub attention: Option<Failure>,
}
impl AccountStatus {
    fn new(account: &Account) -> Self {
        Self {
            id: account_id(&account.phone),
            masked_phone: mask_phone(&account.phone),
            remember_password: account.password.is_some(),
            signed_today: false,
            failed_today: false,
            last_report: None,
            attention: None,
        }
    }
    fn refresh_day(&mut self) {
        let today = china_day(now());
        self.signed_today = self
            .last_report
            .as_ref()
            .is_some_and(|r| r.ok && china_day(r.at) == today);
        self.failed_today = self
            .last_report
            .as_ref()
            .is_some_and(|r| !r.ok && china_day(r.at) == today);
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Status {
    pub accounts: Vec<AccountStatus>,
    pub preferences: Preferences,
}
fn account_id(phone: &str) -> String {
    use md5::{Digest, Md5};
    Md5::digest(phone.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn mask_phone(phone: &str) -> String {
    if phone.len() == 11 && phone.bytes().all(|b| b.is_ascii_digit()) {
        format!("{}****{}", &phone[..3], &phone[7..])
    } else {
        "****".into()
    }
}

pub fn default_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("oddity/taygedo")
}
pub fn default_service() -> Service<KeyringVault> {
    Service::new(default_dir())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn china_day(at: u64) -> u64 {
    (at + 8 * 3600) / 86400
}
fn storage_error(_: impl fmt::Display) -> Failure {
    Failure::new(
        FailureKind::Storage,
        "无法读取或保存签到配置, 请检查配置目录权限",
    )
}
fn id(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
