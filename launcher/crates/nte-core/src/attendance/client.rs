//! 请求配置与签名参考仍在维护的 taygedo-auto-attendance, 许可见 THIRD_PARTY.md。
use super::{id, now, Account, Failure, FailureKind, Result, Step};
use base64::Engine;
use cipher::{BlockCipherEncrypt, KeyInit};
use md5::{Digest, Md5};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;

const BASE: &str = "https://bbs-api.tajiduo.com";
const LAOHU: &str = "https://user.laohu.com";
const APP_VERSION: &str = "1.2.5";
const LAOHU_SECRET: &str = "89155cc4e8634ec5b1b6364013b23e3e";
const GAME_ID: &str = "1289";

pub(super) struct Client {
    agent: ureq::Agent,
    base: String,
    laohu: String,
}
pub(super) struct Session {
    pub access: String,
    pub refresh: String,
    pub uid: String,
    pub laohu_token: String,
    pub laohu_user: String,
}
impl Default for Client {
    fn default() -> Self {
        Self::new(BASE, LAOHU)
    }
}
fn hash(text: impl AsRef<[u8]>) -> String {
    Md5::digest(text)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub(super) fn random_hex(len: usize) -> Result<String> {
    let mut bytes = vec![0u8; len];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(super::storage_error)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn ds(at: u64, nonce: &str) -> String {
    format!(
        "{at},{nonce},{}",
        hash(format!("{at}{nonce}{APP_VERSION}pUds3dfMkl"))
    )
}
fn encrypt(value: &str) -> String {
    let cipher =
        aes::Aes128::new_from_slice(&LAOHU_SECRET.as_bytes()[16..]).expect("固定 AES 密钥长度");
    let mut bytes = value.as_bytes().to_vec();
    let padding = 16 - bytes.len() % 16;
    bytes.resize(bytes.len() + padding, padding as u8);
    for chunk in bytes.as_chunks_mut::<16>().0 {
        let mut block = cipher::Block::<aes::Aes128>::default();
        block.copy_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
    }
    base64::prelude::BASE64_STANDARD.encode(bytes)
}
fn check(value: &Value) -> Result<()> {
    if value["code"].as_i64() == Some(0) {
        return Ok(());
    }
    let msg = value["message"]
        .as_str()
        .or_else(|| value["msg"].as_str())
        .unwrap_or("")
        .to_lowercase();
    if ["人机", "验证码", "滑块", "captcha", "robot", "验证", "风控"]
        .iter()
        .any(|s| msg.contains(s))
    {
        return Err(Failure::new(
            FailureKind::Verification,
            "登录需要人工验证, 请在官方渠道完成验证后重新登录; 自动任务已暂停",
        ));
    }
    if [
        "密码",
        "登录",
        "失效",
        "过期",
        "未授权",
        "token",
        "unauthorized",
    ]
    .iter()
    .any(|s| msg.contains(s))
    {
        return Err(Failure::new(
            FailureKind::Auth,
            "账号登录或会话已失效, 请重新登录",
        ));
    }
    Err(Failure::new(
        FailureKind::Protocol,
        format!(
            "塔吉多请求未成功 (业务码: {})",
            value["code"]
                .as_i64()
                .map_or("未知".into(), |c| c.to_string())
        ),
    ))
}
fn already(value: &Value) -> bool {
    ["msg", "message"]
        .iter()
        .filter_map(|k| value[k].as_str())
        .any(|msg| {
            ["已签到", "签到过", "重复签到"]
                .iter()
                .any(|s| msg.contains(s))
        })
}
fn required(value: &Value, field: &str) -> Result<String> {
    id(&value[field])
        .filter(|s| !s.contains(['\r', '\n']) && s.len() <= 16384)
        .ok_or_else(|| {
            Failure::new(
                FailureKind::Protocol,
                format!("塔吉多响应缺少有效的 {field}"),
            )
        })
}
impl Client {
    pub(super) fn new(base: &str, laohu: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .max_redirects(0)
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
            base: base.into(),
            laohu: laohu.into(),
        }
    }
    fn request_raw(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: Option<&BTreeMap<String, String>>,
    ) -> Result<Value> {
        let result = if let Some(body) = body {
            let mut req = self.agent.post(url);
            for (name, value) in headers {
                req = req.header(*name, value);
            }
            if body.is_empty() {
                req.send_empty()
            } else {
                req.send_form(body.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            }
        } else {
            let mut req = self.agent.get(url);
            for (name, value) in headers {
                req = req.header(*name, value);
            }
            req.call()
        };
        // 不显示原始响应或底层请求错误, 避免其中携带账号或令牌。
        let mut response =
            result.map_err(|_| Failure::new(FailureKind::Network, "请求失败, 请检查网络后重试"))?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .with_config()
            .limit(1024 * 1024)
            .read_to_string()
            .map_err(|_| Failure::new(FailureKind::Network, "无法读取服务响应"))?;
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if matches!(status, 401..=403) {
            return Err(Failure::new(
                FailureKind::Auth,
                "登录被拒绝或会话失效, 请重新登录",
            ));
        }
        if !(200..300).contains(&status) {
            return Err(Failure::new(
                FailureKind::Network,
                format!("服务返回 HTTP {status}"),
            ));
        }
        if !value.is_object() {
            return Err(Failure::new(FailureKind::Protocol, "服务返回了无效 JSON"));
        }
        Ok(value)
    }
    fn request(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: Option<&BTreeMap<String, String>>,
    ) -> Result<Value> {
        let value = self.request_raw(url, headers, body)?;
        if let Err(error) = check(&value) {
            if error.kind == FailureKind::Verification {
                return Err(error);
            }
        }
        Ok(value)
    }
    fn sms_request(
        &self,
        path: &str,
        phone: &str,
        code: Option<&str>,
        device: &str,
        login: bool,
    ) -> Result<Value> {
        let mut body = laohu_profile(device, !login);
        body.insert(
            "cellphone".into(),
            if login { encrypt(phone) } else { phone.into() },
        );
        if let Some(code) = code {
            body.insert(
                "captcha".into(),
                if login { encrypt(code) } else { code.into() },
            );
        }
        if login || code.is_none() {
            body.insert("areaCodeId".into(), "1".into());
            body.insert("type".into(), "16".into());
        }
        sign_form(&mut body, !login);
        self.request_raw(
            &format!("{}{path}", self.laohu),
            &[
                (
                    "Content-Type",
                    "application/x-www-form-urlencoded;charset=UTF-8".into(),
                ),
                (
                    "User-Agent",
                    "LaohuSDK/4.327.0 (android os 14;mobile  manufacturer Google; model Pixel 6)"
                        .into(),
                ),
            ],
            Some(&body),
        )
    }
    pub(super) fn send_captcha(&self, phone: &str, device: &str) -> Result<()> {
        let value = self.sms_request(
            "/m/newApi/sendPhoneCaptchaWithOutLogin",
            phone,
            None,
            device,
            false,
        )?;
        let messages: Vec<_> = ["message", "msg"]
            .iter()
            .filter_map(|k| value[k].as_str())
            .collect();
        if value["code"].as_i64() == Some(0)
            || (value["code"].as_i64() == Some(1)
                && messages.iter().any(|m| m.contains("短信正在发送")))
        {
            return Ok(());
        }
        Err(Failure::new(
            FailureKind::Protocol,
            if messages
                .iter()
                .any(|m| m.contains("频繁") || m.contains("稍后"))
            {
                "短信发送过于频繁, 请稍后再试."
            } else {
                "短信验证码发送失败, 请稍后重试."
            },
        ))
    }
    fn check_captcha(&self, phone: &str, code: &str, device: &str) -> Result<()> {
        let value = self.sms_request(
            "/m/newApi/checkPhoneCaptchaWithOutLogin",
            phone,
            Some(code),
            device,
            false,
        )?;
        if value["code"].as_i64() == Some(0) {
            Ok(())
        } else {
            Err(Failure::new(
                FailureKind::Verification,
                "短信验证码错误或已过期, 请重新获取.",
            ))
        }
    }
    pub(super) fn login_sms(&self, phone: &str, code: &str, device: &str) -> Result<Session> {
        self.check_captcha(phone, code, device)?;
        let value = self.sms_request("/openApi/sms/new/login", phone, Some(code), device, true)?;
        if value["code"].as_i64() != Some(0) {
            return Err(Failure::new(
                FailureKind::Auth,
                "短信登录失败, 请重新获取验证码后重试.",
            ));
        }
        self.user_center(
            &required(&value["result"], "token")?,
            &required(&value["result"], "userId")?,
            device,
        )
    }
    fn native(
        &self,
        path: &str,
        account: &Account,
        token: &str,
        body: Option<BTreeMap<String, String>>,
    ) -> Result<Value> {
        let h5 = path.starts_with("/apihub/awapi/");
        let mut headers = vec![
            ("Accept", "application/json".into()),
            ("Authorization", token.into()),
            ("Content-Type", "application/x-www-form-urlencoded".into()),
        ];
        if h5 {
            headers.extend([("Origin","https://webstatic.tajiduo.com".into()),("Referer","https://webstatic.tajiduo.com/".into()),("User-Agent","Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148 Tajiduo/1.2.2".into())]);
        } else {
            headers.extend([
                ("appVersion", APP_VERSION.into()),
                ("platform", "android".into()),
                ("uid", account.uid.clone()),
                ("deviceId", account.device_id.clone()),
                ("ds", ds(now(), &random_hex(4)?)),
                ("User-Agent", "okhttp/4.12.0".into()),
            ]);
            if path.starts_with("/usercenter/api/login")
                || path.starts_with("/usercenter/api/refreshToken")
            {
                headers.push(("debug-uid", "3".into()));
            }
        }
        self.request(&format!("{}{path}", self.base), &headers, body.as_ref())
    }
    pub(super) fn login(&self, phone: &str, password: &str, device: &str) -> Result<Session> {
        let mut body: BTreeMap<String, String> = [
            ("adm", ""),
            ("appId", "10550"),
            ("bid", "com.pwrd.htassistant"),
            ("channelId", "1"),
            ("deviceId", device),
            ("deviceModel", "Pixel 6"),
            ("deviceName", "Pixel 6"),
            ("deviceSys", "14"),
            ("deviceType", "Pixel 6"),
            ("idfa", ""),
            ("mac", ""),
            ("sdkVersion", "4.327.0"),
            ("version", "17"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        body.insert(
            "t".into(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .to_string(),
        );
        body.insert("username".into(), encrypt(phone));
        body.insert("password".into(), encrypt(password));
        let sign = hash(format!(
            "{}{LAOHU_SECRET}",
            body.values().cloned().collect::<String>()
        ));
        body.insert("sign".into(), sign);
        let value = self.request(
            &format!("{}/openApi/secureLogin", self.laohu),
            &[
                (
                    "Content-Type",
                    "application/x-www-form-urlencoded;charset=UTF-8".into(),
                ),
                (
                    "User-Agent",
                    "LaohuSDK/4.327.0 (android os 14;mobile  manufacturer Google; model Pixel 6) "
                        .into(),
                ),
                ("robot-auth-type", "2".into()),
            ],
            Some(&body),
        ).map_err(|e| e.at("账号密码登录"))?;
        check(&value).map_err(|e| e.at("账号密码登录"))?;
        self.user_center(
            &required(&value["result"], "token")?,
            &required(&value["result"], "userId")?,
            device,
        )
    }
    pub(super) fn user_center(&self, token: &str, user: &str, device: &str) -> Result<Session> {
        let account = Account {
            phone: String::new(),
            password: None,
            device_id: device.into(),
            uid: "0".into(),
            refresh_token: String::new(),
            laohu_token: String::new(),
            laohu_user_id: String::new(),
        };
        let value = self
            .native(
                "/usercenter/api/login",
                &account,
                "",
                Some(form(&[
                    ("token", token),
                    ("userIdentity", user),
                    ("appId", "10551"),
                ])),
            )
            .map_err(|e| e.at("塔吉多会话登录"))?;
        check(&value).map_err(|e| e.at("塔吉多会话登录"))?;
        Ok(Session {
            access: required(&value["data"], "accessToken")?,
            refresh: required(&value["data"], "refreshToken")?,
            uid: required(&value["data"], "uid")?,
            laohu_token: token.into(),
            laohu_user: user.into(),
        })
    }
    pub(super) fn refresh(&self, account: &Account) -> Result<Session> {
        let value = self
            .native(
                "/usercenter/api/refreshToken",
                account,
                &account.refresh_token,
                Some(BTreeMap::new()),
            )
            .map_err(|e| e.at("刷新塔吉多会话"))?;
        check(&value).map_err(|e| e.at("刷新塔吉多会话"))?;
        Ok(Session {
            access: required(&value["data"], "accessToken")?,
            refresh: required(&value["data"], "refreshToken")?,
            uid: id(&value["data"]["uid"]).unwrap_or_else(|| account.uid.clone()),
            laohu_token: account.laohu_token.clone(),
            laohu_user: account.laohu_user_id.clone(),
        })
    }
    pub(super) fn sign(&self, account: &Account, access: &str) -> Vec<Step> {
        let result = self
            .native(
                "/apihub/api/signin",
                account,
                access,
                Some(form(&[("communityId", "1")])),
            )
            .and_then(|v| if already(&v) { Ok(()) } else { check(&v) });
        let mut steps = vec![Step::from_result("社区签到", result)];
        if steps
            .iter()
            .any(|s| matches!(s.kind, Some(FailureKind::Auth | FailureKind::Verification)))
        {
            return steps;
        }
        let roles = (|| -> Result<Vec<String>> {
            let value = self.native(
                &format!("/usercenter/api/v2/getGameRoles?gameId={GAME_ID}"),
                account,
                access,
                None,
            )?;
            check(&value)?;
            let mut roles: Vec<String> = value["data"]["roles"]
                .as_array()
                .ok_or_else(|| Failure::new(FailureKind::Protocol, "角色列表格式无效"))?
                .iter()
                .filter_map(|r| id(&r["roleId"]))
                .collect();
            roles.sort();
            roles.dedup();
            if roles.is_empty() {
                return Err(Failure::new(
                    FailureKind::Protocol,
                    "未找到异环角色, 请先在塔吉多绑定角色",
                ));
            }
            Ok(roles)
        })();
        match roles {
            Err(error) => steps.push(Step::from_result("获取异环角色", Err(error))),
            Ok(roles) => {
                for (i, role) in roles.iter().enumerate() {
                    let result = (|| -> Result<()> {
                        let value = self.native(
                            "/apihub/awapi/sign",
                            account,
                            access,
                            Some(form(&[("gameId", GAME_ID), ("roleId", role)])),
                        )?;
                        if already(&value) {
                            let state = self.native(
                                &format!("/apihub/awapi/signin/state?gameId={GAME_ID}"),
                                account,
                                access,
                                None,
                            )?;
                            check(&state)?;
                            if state["data"]["todaySign"] == json!(true) {
                                return Ok(());
                            }
                            return Err(Failure::new(
                                FailureKind::Protocol,
                                "重复签到响应未获得今日状态确认",
                            ));
                        }
                        check(&value)
                    })();
                    let stop = result.as_ref().is_err_and(|e| e.needs_login());
                    steps.push(Step::from_result(format!("异环角色 {}", i + 1), result));
                    if stop {
                        break;
                    }
                }
            }
        }
        steps
    }
}
fn laohu_profile(device: &str, version_code: bool) -> BTreeMap<String, String> {
    let mut body = form(&[
        ("adm", ""),
        ("appId", "10550"),
        ("bid", "com.pwrd.htassistant"),
        ("channelId", "1"),
        ("deviceId", device),
        ("deviceModel", "Pixel 6"),
        ("deviceName", "Pixel 6"),
        ("deviceSys", "14"),
        ("deviceType", "Pixel 6"),
        ("idfa", ""),
        ("sdkVersion", "4.327.0"),
    ]);
    body.insert(if version_code { "imei" } else { "mac" }.into(), "".into());
    body.insert(
        if version_code {
            "versionCode"
        } else {
            "version"
        }
        .into(),
        "17".into(),
    );
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    body.insert(
        "t".into(),
        if version_code {
            at.as_secs().to_string()
        } else {
            at.as_millis().to_string()
        },
    );
    body
}
fn sign_form(body: &mut BTreeMap<String, String>, filter_empty: bool) {
    let sign = hash(format!(
        "{}{LAOHU_SECRET}",
        body.values().cloned().collect::<String>()
    ));
    body.insert("sign".into(), sign);
    if filter_empty {
        body.retain(|_, v| !v.is_empty());
    }
}
fn form(fields: &[(&str, &str)]) -> BTreeMap<String, String> {
    fields
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sms_wire_profiles_signatures_encryption_and_filtering() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let mut paths = Vec::new();
            for index in 0..4 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap().to_string();
                paths.push(path.clone());
                let mut headers = BTreeMap::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    let (key, value) = line.trim().split_once(':').unwrap();
                    headers.insert(key.to_ascii_lowercase(), value.trim().to_string());
                }
                let mut bytes = vec![0; headers["content-length"].parse::<usize>().unwrap()];
                reader.read_exact(&mut bytes).unwrap();
                if index < 3 {
                    assert_eq!(
                        headers["content-type"],
                        "application/x-www-form-urlencoded;charset=UTF-8"
                    );
                    assert_eq!(headers["user-agent"], "LaohuSDK/4.327.0 (android os 14;mobile  manufacturer Google; model Pixel 6)");
                    let mut body: BTreeMap<String, String> = String::from_utf8(bytes)
                        .unwrap()
                        .split('&')
                        .map(|field| {
                            let (key, value) = field.split_once('=').unwrap();
                            let decode = |text: &str| {
                                let mut output = Vec::new();
                                let mut bytes = text.bytes();
                                while let Some(byte) = bytes.next() {
                                    output.push(match byte {
                                        b'+' => b' ',
                                        b'%' => {
                                            let hex =
                                                [bytes.next().unwrap(), bytes.next().unwrap()];
                                            u8::from_str_radix(
                                                std::str::from_utf8(&hex).unwrap(),
                                                16,
                                            )
                                            .unwrap()
                                        }
                                        other => other,
                                    });
                                }
                                String::from_utf8(output).unwrap()
                            };
                            (decode(key), decode(value))
                        })
                        .collect();
                    let sign = body.remove("sign").unwrap();
                    assert_eq!(
                        sign,
                        hash(format!(
                            "{}{LAOHU_SECRET}",
                            body.values().cloned().collect::<String>()
                        ))
                    );
                    assert_eq!(body["deviceId"], "device");
                    if index < 2 {
                        assert_eq!(body["versionCode"], "17");
                        assert!(!body.contains_key("version"));
                        for key in ["adm", "idfa", "imei", "mac"] {
                            assert!(!body.contains_key(key));
                        }
                        assert!(body.values().all(|v| !v.is_empty()));
                        assert!(body["t"].parse::<u64>().unwrap().abs_diff(now()) <= 5);
                        assert_eq!(body["cellphone"], "13800138000");
                        if index == 0 {
                            assert_eq!(body["areaCodeId"], "1");
                            assert_eq!(body["type"], "16");
                            assert!(!body.contains_key("captcha"));
                        } else {
                            assert_eq!(body["captcha"], "123456");
                            assert!(!body.contains_key("type"));
                            assert!(!body.contains_key("areaCodeId"));
                        }
                    } else {
                        assert_eq!(body["version"], "17");
                        assert!(!body.contains_key("versionCode"));
                        assert!(!body.contains_key("imei"));
                        for key in ["adm", "idfa", "mac"] {
                            assert_eq!(body[key], "");
                        }
                        assert!(body["t"].parse::<u64>().unwrap().abs_diff(now() * 1000) < 6000);
                        assert_eq!(body["cellphone"], encrypt("13800138000"));
                        assert_eq!(body["captcha"], encrypt("123456"));
                        assert_eq!(body["areaCodeId"], "1");
                        assert_eq!(body["type"], "16");
                    }
                }
                let response = match index {
                    0 | 1 => r#"{"code":0}"#,
                    2 => r#"{"code":0,"result":{"token":"secret","userId":"user"}}"#,
                    _ => {
                        r#"{"code":0,"data":{"accessToken":"access","refreshToken":"refresh","uid":"uid"}}"#
                    }
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                )
                .unwrap();
            }
            paths
        });
        let client = Client::new(&base, &base);
        client.send_captcha("13800138000", "device").unwrap();
        client.login_sms("13800138000", "123456", "device").unwrap();
        assert_eq!(
            worker.join().unwrap(),
            [
                "/m/newApi/sendPhoneCaptchaWithOutLogin",
                "/m/newApi/checkPhoneCaptchaWithOutLogin",
                "/openApi/sms/new/login",
                "/usercenter/api/login",
            ]
        );
    }
    #[test]
    fn signature_vector_and_encryption() {
        assert_eq!(
            ds(1770000000, "AbCd1234"),
            "1770000000,AbCd1234,98d886c4c3b3f68edbd335e706e66370"
        );
        assert_eq!(encrypt("secret-password"), "ULobCwCc+A8F/uF8S8Tvzg==");
    }
    #[test]
    fn classifies_authentication_and_verification_without_leaking_body() {
        let e = check(&json!({"code":7,"message":"人机验证 private-token"})).unwrap_err();
        assert_eq!(e.kind, FailureKind::Verification);
        assert!(!e.message.contains("private-token"));
        assert_eq!(
            check(&json!({"code":1,"msg":"密码错误"})).unwrap_err().kind,
            FailureKind::Auth
        );
        assert!(check(&json!({})).is_err());
    }
}
