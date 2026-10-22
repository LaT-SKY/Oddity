//! 直接访问桌面 Keyring 的 Secret Service 接口, 不发起解锁请求。
use super::{Account, Failure, FailureKind, Result};
use secret_service::{
    blocking::{Collection, SecretService},
    EncryptionType,
};
use std::collections::HashMap;
use zeroize::Zeroizing;

const SERVICE: &str = "io.github.oddity.Launcher.taygedo";
pub trait Vault {
    fn load_all(&self) -> Result<Vec<Account>>;
    fn save(&self, account: &Account) -> Result<()>;
    fn delete(&self, account_id: &str) -> Result<()>;
}
pub struct KeyringVault {
    scope: String,
}
impl KeyringVault {
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
        }
    }
    fn attributes(&self) -> HashMap<&str, &str> {
        HashMap::from([("service", SERVICE), ("username", self.scope.as_str())])
    }
    fn with_collection<T>(&self, action: impl FnOnce(&Collection<'_>) -> Result<T>) -> Result<T> {
        let service = SecretService::connect(EncryptionType::Dh).map_err(keyring_error)?;
        let collection = service.get_default_collection().map_err(keyring_error)?;
        collection.ensure_unlocked().map_err(keyring_error)?;
        action(&collection)
    }
}
fn keyring_error(_: impl std::fmt::Display) -> Failure {
    Failure::new(
        FailureKind::Keyring,
        "无法访问 Keyring, 请确认桌面密钥环已启动并解锁",
    )
}
fn ambiguous() -> Failure {
    Failure::new(
        FailureKind::Keyring,
        "Keyring 中存在重复的塔吉多条目, 请在密钥环管理器中检查",
    )
}
impl Vault for KeyringVault {
    fn load_all(&self) -> Result<Vec<Account>> {
        self.with_collection(|collection| {
            let items = collection
                .search_items(self.attributes())
                .map_err(keyring_error)?;
            if items.len() > 1 {
                return Err(ambiguous());
            }
            let Some(item) = items.first() else {
                return Ok(vec![]);
            };
            item.ensure_unlocked().map_err(keyring_error)?;
            let secret = Zeroizing::new(item.get_secret().map_err(keyring_error)?);
            decode_accounts(&secret)
        })
    }
    fn save(&self, account: &Account) -> Result<()> {
        let mut accounts = self.load_all()?;
        if let Some(existing) = accounts.iter_mut().find(|a| a.phone == account.phone) {
            *existing = account.clone();
        } else {
            accounts.push(account.clone());
        }
        self.write_all(&accounts)
    }
    fn delete(&self, account_id: &str) -> Result<()> {
        let mut accounts = self.load_all()?;
        accounts.retain(|a| super::account_id(&a.phone) != account_id);
        if !accounts.is_empty() {
            return self.write_all(&accounts);
        }
        self.with_collection(|collection| {
            let items = collection
                .search_items(self.attributes())
                .map_err(keyring_error)?;
            if items.len() > 1 {
                return Err(ambiguous());
            }
            if let Some(item) = items.first() {
                item.ensure_unlocked().map_err(keyring_error)?;
                item.delete().map_err(keyring_error)?;
            }
            Ok(())
        })
    }
}
fn decode_accounts(secret: &[u8]) -> Result<Vec<Account>> {
    serde_json::from_slice::<Vec<Account>>(secret)
        .or_else(|_| serde_json::from_slice::<Account>(secret).map(|a| vec![a]))
        .map_err(|_| Failure::new(FailureKind::Keyring, "Keyring 中的塔吉多账号格式无效"))
}
impl KeyringVault {
    fn write_all(&self, accounts: &[Account]) -> Result<()> {
        let secret = Zeroizing::new(serde_json::to_vec(accounts).map_err(super::storage_error)?);
        self.with_collection(|collection| {
            let items = collection
                .search_items(self.attributes())
                .map_err(keyring_error)?;
            if items.len() > 1 {
                return Err(ambiguous());
            }
            if let Some(item) = items.first() {
                item.ensure_unlocked().map_err(keyring_error)?;
                item.set_secret(&secret, "text/plain; charset=utf-8")
                    .map_err(keyring_error)?;
            } else {
                collection.ensure_unlocked().map_err(keyring_error)?;
                collection
                    .create_item(
                        "Oddity 塔吉多账号",
                        self.attributes(),
                        &secret,
                        true,
                        "text/plain; charset=utf-8",
                    )
                    .map_err(keyring_error)?;
            }
            Ok(())
        })
    }
}
/// 只读写独立的临时条目, 不访问用户已有账号。
pub fn check_keyring() -> Result<()> {
    let scope = format!(
        "check-{}-{}",
        std::process::id(),
        super::client::random_hex(8)?
    );
    let vault = KeyringVault::new(scope);
    let account = Account {
        phone: String::new(),
        password: None,
        device_id: "check".into(),
        uid: "check".into(),
        refresh_token: "Oddity Keyring check".into(),
        laohu_token: String::new(),
        laohu_user_id: String::new(),
    };
    vault.save(&account)?;
    let result = vault.load_all().and_then(|value| {
        if value.len() == 1 && value[0].refresh_token == "Oddity Keyring check" {
            Ok(())
        } else {
            Err(keyring_error("roundtrip"))
        }
    });
    let cleanup = vault.delete(&super::account_id(&account.phone));
    result.and(cleanup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_legacy_account_and_array_without_exposing_invalid_secret() {
        let legacy = serde_json::json!({
            "phone": "13800138000", "password": null, "deviceId": "device",
            "uid": "uid", "refreshToken": "refresh-secret",
            "laohuToken": "laohu-secret", "laohuUserId": "user"
        });
        let accounts = decode_accounts(&serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].phone, "13800138000");
        let encoded = serde_json::to_vec(&accounts).unwrap();
        assert!(encoded.starts_with(b"["));
        assert_eq!(
            decode_accounts(&encoded).unwrap()[0].refresh_token,
            "refresh-secret"
        );
        assert!(decode_accounts(b"[]").unwrap().is_empty());
        let error = decode_accounts(b"private-secret").err().unwrap();
        assert_eq!(error.kind, FailureKind::Keyring);
        assert!(!error.message.contains("private-secret"));
    }
}
