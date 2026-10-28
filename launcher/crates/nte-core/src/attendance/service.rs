use super::{
    client::{random_hex, Client, Session},
    *,
};
use md5::{Digest, Md5};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

struct FileLock(File);
impl Drop for FileLock {
    fn drop(&mut self) {
        // 显式解锁, 避免并发启动子进程时继承的描述符延迟释放。
        let _ = self.0.unlock();
    }
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StoredStatus {
    accounts: Vec<AccountStatus>,
    preferences: Preferences,
    configured: bool,
    masked_phone: String,
    remember_password: bool,
    last_report: Option<Report>,
    attention: Option<Failure>,
}
pub struct Service<V: Vault> {
    dir: PathBuf,
    vault: V,
    client: Client,
}
impl Service<KeyringVault> {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref().to_path_buf();
        let scope: String = Md5::digest(dir.as_os_str().as_encoded_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Self {
            dir,
            vault: KeyringVault::new(scope),
            client: Client::default(),
        }
    }
}
impl<V: Vault> Service<V> {
    fn lock(&self, wait: bool) -> Result<FileLock> {
        self.lock_named("lock", wait, false)
    }
    // 顺序固定为账号集合、任务、状态锁; 网络任务只持有前两者。
    fn lock_named(&self, name: &str, wait: bool, shared: bool) -> Result<FileLock> {
        fs::create_dir_all(&self.dir).map_err(storage_error)?;
        fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700)).map_err(storage_error)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(self.dir.join(name))
            .map_err(storage_error)?;
        if wait {
            if shared {
                file.lock_shared()
            } else {
                file.lock()
            }
            .map_err(storage_error)?;
        } else {
            if shared {
                file.try_lock_shared()
            } else {
                file.try_lock()
            }
            .map_err(|error| match error {
                std::fs::TryLockError::WouldBlock => {
                    Failure::new(FailureKind::Busy, "账号或同类任务正在处理, 请稍后重试")
                }
                std::fs::TryLockError::Error(error) => storage_error(error),
            })?;
        }
        Ok(FileLock(file))
    }
    pub fn status(&self) -> Result<Status> {
        let stored = self.read_stored()?;
        if Self::needs_migration(&stored) {
            match self.lock(false) {
                Ok(_lock) => return self.status_locked(),
                Err(e) if e.kind == FailureKind::Busy => {}
                Err(e) => return Err(e),
            }
        }
        self.restore_status(stored)
    }
    fn read_stored(&self) -> Result<StoredStatus> {
        match fs::read(self.dir.join("state.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
                Failure::new(FailureKind::Storage, "签到配置格式无效, 请检查 state.json")
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StoredStatus::default()),
            Err(e) => Err(storage_error(e)),
        }
    }
    fn needs_migration(stored: &StoredStatus) -> bool {
        stored.accounts.is_empty() && (stored.configured || !stored.masked_phone.is_empty())
    }
    fn restore_status(&self, stored: StoredStatus) -> Result<Status> {
        let migration = Self::needs_migration(&stored);
        let mut status = Status {
            accounts: stored.accounts,
            preferences: stored.preferences,
        };
        if migration {
            status.accounts = self
                .vault
                .load_all()?
                .iter()
                .map(AccountStatus::new)
                .collect();
            if let Some(first) = status.accounts.first_mut() {
                first.remember_password = stored.remember_password;
                first.last_report = stored.last_report;
                first.attention = stored.attention;
            }
        }
        for account in &mut status.accounts {
            account.refresh_day();
        }
        Ok(status)
    }
    fn status_locked(&self) -> Result<Status> {
        let stored = self.read_stored()?;
        let migration = Self::needs_migration(&stored);
        let status = self.restore_status(stored)?;
        if migration {
            self.write(&status)?;
        }
        Ok(status)
    }
    fn write(&self, status: &Status) -> Result<()> {
        let path = self.dir.join("state.json.tmp");
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(storage_error(e)),
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(storage_error)?;
        file.write_all(&serde_json::to_vec_pretty(status).map_err(storage_error)?)
            .map_err(storage_error)?;
        file.sync_all().map_err(storage_error)?;
        fs::rename(path, self.dir.join("state.json")).map_err(storage_error)?;
        File::open(&self.dir)
            .and_then(|f| f.sync_all())
            .map_err(storage_error)
    }
    pub fn login(&self, phone: &str, password: &str, remember_password: bool) -> Result<Status> {
        let _accounts_lock = self.lock_named("accounts.lock", false, false)?;
        let password = Zeroizing::new(password.to_string());
        if phone.len() != 11
            || !phone.bytes().all(|b| b.is_ascii_digit())
            || password.is_empty()
            || password.len() > 1024
        {
            return Err(Failure::new(
                FailureKind::Auth,
                "请输入 11 位手机号码和有效密码",
            ));
        }
        let _lock = self.lock(false)?;
        let status = self.status_locked()?;
        // 登录前先确认密钥环可用, 已有同一账号复用设备身份。
        let existing = self.vault.load_all()?;
        let device = match existing.iter().find(|a| a.phone == phone) {
            Some(a) => a.device_id.clone(),
            None => random_hex(16)?,
        };
        let session = self.client.login(phone, &password, &device)?;
        let account = Account {
            phone: phone.into(),
            password: remember_password.then(|| password.to_string()),
            device_id: device,
            uid: session.uid,
            refresh_token: session.refresh,
            laohu_token: session.laohu_token,
            laohu_user_id: session.laohu_user,
        };
        self.save_login(status, &account)
    }
    fn save_login(&self, mut status: Status, account: &Account) -> Result<Status> {
        self.vault.save(account)?;
        let next = AccountStatus::new(account);
        if let Some(current) = status.accounts.iter_mut().find(|a| a.id == next.id) {
            *current = next;
        } else {
            status.accounts.push(next);
        }
        self.write(&status)?;
        Ok(status)
    }
    pub fn send_code(&self, phone: &str) -> Result<String> {
        let _accounts_lock = self.lock_named("accounts.lock", false, false)?;
        validate_phone(phone)?;
        let _lock = self.lock(false)?;
        let existing = self.vault.load_all()?;
        let device = match existing.iter().find(|a| a.phone == phone) {
            Some(a) => a.device_id.clone(),
            None => random_hex(16)?,
        };
        self.client.send_captcha(phone, &device)?;
        Ok(device)
    }
    pub fn login_sms(&self, phone: &str, code: &str, device_id: &str) -> Result<Status> {
        let _accounts_lock = self.lock_named("accounts.lock", false, false)?;
        validate_phone(phone)?;
        let code = Zeroizing::new(code.trim().to_string());
        if code.is_empty()
            || code.chars().count() > 16
            || code.chars().any(char::is_control)
            || device_id.is_empty()
            || device_id.chars().count() > 128
            || device_id.chars().any(char::is_control)
        {
            return Err(Failure::new(
                FailureKind::Auth,
                "请输入有效的短信验证码和设备标识",
            ));
        }
        let _lock = self.lock(false)?;
        let status = self.status_locked()?;
        // 先读取凭据, 密钥环不可用时不消耗验证码。
        let existing = self.vault.load_all()?;
        let session = self.client.login_sms(phone, &code, device_id)?;
        let account = Account {
            phone: phone.into(),
            password: existing
                .iter()
                .find(|a| a.phone == phone)
                .and_then(|a| a.password.clone()),
            device_id: device_id.into(),
            uid: session.uid,
            refresh_token: session.refresh,
            laohu_token: session.laohu_token,
            laohu_user_id: session.laohu_user,
        };
        self.save_login(status, &account)
    }
    pub fn configure(&self, preferences: Preferences, executable: &Path) -> Result<Status> {
        preferences.validate()?;
        let _lock = self.lock(false)?;
        let mut status = self.status_locked()?;
        if preferences.trigger != Trigger::Off && status.accounts.is_empty() {
            return Err(Failure::new(FailureKind::Auth, "请先登录塔吉多账号"));
        }
        let previous = status.preferences.clone();
        status.preferences = preferences;
        self.write(&status)?;
        if let Err(error) = schedule::apply(executable, &self.dir, &status.preferences) {
            status.preferences = previous;
            self.write(&status)?;
            schedule::apply(executable, &self.dir, &status.preferences)?;
            return Err(error);
        }
        Ok(status)
    }
    pub fn logout(&self, account_id: &str, executable: &Path) -> Result<Status> {
        let _accounts_lock = self.lock_named("accounts.lock", false, false)?;
        let _lock = self.lock(false)?;
        let mut status = self.status_locked()?;
        let last = status.accounts.len() == 1 && status.accounts[0].id == account_id;
        // 先停定时器再删凭据, 定时器失败时账号仍然完整。
        if last {
            schedule::apply(executable, &self.dir, &Preferences::default())?;
        }
        if let Err(error) = self.vault.delete(account_id) {
            if last {
                // 凭据未删除, 恢复原定时器, 避免自动签到被静默关闭。
                let _ = schedule::apply(executable, &self.dir, &status.preferences);
            }
            return Err(error);
        }
        status.accounts.retain(|a| a.id != account_id);
        if status.accounts.is_empty() {
            status.preferences = Preferences::default();
        }
        self.write(&status)?;
        Ok(status)
    }
    fn session(&self, account: &Account) -> Result<Session> {
        match self.client.refresh(account) {
            Ok(s) => return Ok(s),
            Err(e) if e.kind == FailureKind::Auth => {}
            Err(e) => return Err(e),
        }
        match self.client.user_center(
            &account.laohu_token,
            &account.laohu_user_id,
            &account.device_id,
        ) {
            Ok(s) => return Ok(s),
            Err(e) if e.kind == FailureKind::Auth => {}
            Err(e) => return Err(e),
        }
        match &account.password {
            Some(password) => self
                .client
                .login(&account.phone, password, &account.device_id),
            None => Err(Failure::new(
                FailureKind::Auth,
                "登录已失效, 请使用手机号和密码重新登录",
            )),
        }
    }
    fn prepare_session(&self, id: &str) -> Result<(Account, Session)> {
        let _lock = self.lock(true)?;
        let mut account = self
            .vault
            .load_all()?
            .into_iter()
            .find(|a| account_id(&a.phone) == id)
            .ok_or_else(|| Failure::new(FailureKind::Auth, "账号凭据不存在, 请重新登录"))?;
        let session = self.session(&account)?;
        account.uid = session.uid.clone();
        account.refresh_token = session.refresh.clone();
        account.laohu_token = session.laohu_token.clone();
        account.laohu_user_id = session.laohu_user.clone();
        self.vault.save(&account)?;
        Ok((account, session))
    }
    fn cloud_duration(&self, account: &mut Account) -> Step {
        let result = (|| -> Result<Step> {
            {
                let _lock = self.lock(true)?;
                // 查询可能已轮换令牌, 云任务不能用旧快照覆盖凭据。
                *account = self
                    .vault
                    .load_all()?
                    .into_iter()
                    .find(|a| a.phone == account.phone)
                    .ok_or_else(|| Failure::new(FailureKind::Auth, "账号凭据不存在, 请重新登录"))?;
                if account.laohu_token.is_empty() || account.laohu_user_id.is_empty() {
                    let Some(password) = account.password.as_deref().filter(|p| !p.is_empty())
                    else {
                        return Ok(Step::optional(
                            "云异环时长",
                            true,
                            "已跳过: 缺少老虎会话且未保存密码",
                            None,
                        ));
                    };
                    let (token, user) =
                        self.client
                            .login_laohu(&account.phone, password, &account.device_id)?;
                    account.laohu_token = token;
                    account.laohu_user_id = user;
                    self.vault.save(account)?;
                }
            }
            self.client.cloud_duration(account)
        })();
        result.unwrap_or_else(|e| Step::optional_failure("云异环时长", e))
    }
    pub fn refresh_game_records(&self) -> Result<Status> {
        let _accounts_lock = self.lock_named("accounts.lock", false, true)?;
        let _task_lock = self.lock_named("records.lock", false, false)?;
        let accounts = {
            let _lock = self.lock(false)?;
            let status = self.status_locked()?;
            if status.accounts.is_empty() {
                return Ok(status);
            }
            self.vault.load_all()?;
            status.accounts
        };
        for current in accounts {
            let result = self
                .prepare_session(&current.id)
                .and_then(|(account, session)| self.client.game_record(&account, &session.access));
            let _lock = self.lock(true)?;
            let mut status = self.status_locked()?;
            if let Some(current) = status.accounts.iter_mut().find(|a| a.id == current.id) {
                match result {
                    Ok(record) => {
                        current.game_record = Some(record);
                        current.game_record_error = None;
                    }
                    Err(mut error) => {
                        error.message = error.message.replace("; 自动任务已暂停", "");
                        current.game_record_error = Some(error);
                    }
                }
            }
            self.write(&status)?;
        }
        self.status()
    }
    pub fn run(&self, automatic: Option<Trigger>) -> Result<Status> {
        let _accounts_lock = self.lock_named("accounts.lock", automatic.is_some(), true)?;
        let _task_lock = self.lock_named("attendance.lock", automatic.is_some(), false)?;
        let status = {
            let _lock = self.lock(automatic.is_some())?;
            let mut status = self.status_locked()?;
            if !status.preferences.tasks.any() {
                return Ok(status);
            }
            if let Some(trigger) = automatic {
                if matches!(trigger, Trigger::Off | Trigger::Manual)
                    || status.preferences.trigger != trigger
                    || status
                        .accounts
                        .iter()
                        .all(|a| a.tasks_complete(&status.preferences.tasks))
                {
                    return Ok(status);
                }
            } else if status.accounts.is_empty() {
                return Err(Failure::new(FailureKind::Auth, "请先登录塔吉多账号"));
            }
            let accounts = self.vault.load_all()?;
            status.accounts = accounts
                .iter()
                .map(|account| {
                    let id = account_id(&account.phone);
                    status
                        .accounts
                        .iter()
                        .find(|a| a.id == id)
                        .cloned()
                        .unwrap_or_else(|| AccountStatus::new(account))
                })
                .collect();
            self.write(&status)?;
            status
        };
        let tasks = &status.preferences.tasks;
        for current in &status.accounts {
            if current.tasks_complete(tasks)
                || current.attention.as_ref().is_some_and(Failure::needs_login)
            {
                continue;
            }
            let previous = current
                .last_report
                .as_ref()
                .filter(|report| china_day(report.at) == china_day(now()))
                .cloned();
            let signed = previous
                .as_ref()
                .is_some_and(|report| report.ok && report.core_complete(tasks));
            let coin_complete = previous.as_ref().is_some_and(|r| r.coin_complete(tasks));
            let cloud_complete = previous.as_ref().is_some_and(Report::cloud_complete);
            let result = (|| -> Result<Vec<Step>> {
                let (mut account, session) = self.prepare_session(&current.id)?;
                // 当天已成功的任务保留结果, 金币剩余次数仍以服务端为准。
                let mut steps: Vec<Step> = if signed {
                    previous
                        .as_ref()
                        .unwrap()
                        .steps
                        .iter()
                        .filter(|step| !step.optional && tasks.allows(&step.label))
                        .cloned()
                        .collect()
                } else {
                    let mut pending = *tasks;
                    if let Some(report) = &previous {
                        pending.community_sign &=
                            !report.steps.iter().any(|s| s.label == "社区签到" && s.ok);
                        pending.role_sign &= !report.core_complete(&TaskPermissions {
                            community_sign: false,
                            ..*tasks
                        });
                    }
                    let mut steps = self.client.sign(&account, &session.access, &pending);
                    if let Some(report) = &previous {
                        steps.extend(
                            report
                                .steps
                                .iter()
                                .filter(|s| {
                                    !s.optional
                                        && tasks.allows(&s.label)
                                        && !pending.allows(&s.label)
                                })
                                .cloned(),
                        );
                    }
                    steps
                };
                if tasks.coin_enabled() && coin_complete {
                    steps.extend(
                        previous
                            .as_ref()
                            .unwrap()
                            .steps
                            .iter()
                            .filter(|step| {
                                step.optional
                                    && tasks.allows(&step.label)
                                    && step.label != "云异环时长"
                                    && step.label != "附加任务准备"
                            })
                            .cloned(),
                    );
                } else if tasks.coin_enabled()
                    && !steps.iter().any(|s| {
                        s.kind.is_some_and(|k| {
                            matches!(k, FailureKind::Auth | FailureKind::Verification)
                        })
                    })
                {
                    steps.extend(self.client.coin_tasks(&account, &session.access, tasks));
                }
                if tasks.cloud_duration && cloud_complete {
                    steps.extend(
                        previous
                            .as_ref()
                            .unwrap()
                            .steps
                            .iter()
                            .filter(|step| step.label == "云异环时长")
                            .cloned(),
                    );
                } else if tasks.cloud_duration {
                    steps.push(self.cloud_duration(&mut account));
                }
                Ok(steps)
            })();
            let steps = result.unwrap_or_else(|error| {
                if signed {
                    let mut steps = previous.as_ref().unwrap().steps.clone();
                    steps.retain(|step| step.label != "附加任务准备");
                    steps.push(Step::optional_failure("附加任务准备", error));
                    steps
                } else {
                    vec![Step::from_result("登录与准备", Err(error))]
                }
            });
            let report = Report {
                at: now(),
                ok: !steps.is_empty()
                    && if tasks.community_sign || tasks.role_sign {
                        steps.iter().filter(|s| !s.optional).all(|s| s.ok)
                    } else {
                        steps.iter().all(|s| s.ok)
                    },
                steps,
            };
            let _lock = self.lock(true)?;
            let mut latest = self.status_locked()?;
            let Some(current) = latest.accounts.iter_mut().find(|a| a.id == current.id) else {
                continue;
            };
            current.attention = report.steps.iter().filter(|s| !s.optional).find_map(|s| {
                s.kind
                    .filter(|k| matches!(k, FailureKind::Auth | FailureKind::Verification))
                    .map(|k| Failure::new(k, s.message.clone()))
            });
            current.last_report = Some(report);
            current.refresh_day();
            self.write(&latest)?;
        }
        self.status()
    }
}

fn validate_phone(phone: &str) -> Result<()> {
    if phone.len() != 11 || !phone.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Failure::new(FailureKind::Auth, "请输入 11 位手机号码"));
    }
    Ok(())
}

#[cfg(test)]
impl<V: Vault> Service<V> {
    pub(super) fn testing(dir: &Path, vault: V, base: &str) -> Self {
        Self {
            dir: dir.into(),
            vault,
            client: Client::new(base, base).without_delays(),
        }
    }
    pub(super) fn set_tasks(&self, tasks: TaskPermissions) {
        let _lock = self.lock(false).unwrap();
        let mut status = self.status_locked().unwrap();
        status.preferences.tasks = tasks;
        self.write(&status).unwrap();
    }
    pub(super) fn set_trigger(&self, trigger: Trigger) {
        let _lock = self.lock(false).unwrap();
        let mut s = self.status_locked().unwrap();
        s.preferences.trigger = trigger;
        self.write(&s).unwrap();
    }
}
