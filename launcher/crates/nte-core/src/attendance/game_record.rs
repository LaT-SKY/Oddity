//! 塔吉多角色面板协议参考 NTEUID, 来源和许可见 THIRD_PARTY.md。
use super::{id, now, Failure, FailureKind, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRecord {
    pub role_id: String,
    pub role_name: String,
    pub server_name: String,
    pub stamina: Option<u32>,
    pub stamina_max: Option<u32>,
    pub city_stamina: Option<u32>,
    pub city_stamina_max: Option<u32>,
    pub activity: Option<u32>,
    pub weekly_remaining: Option<u32>,
    pub updated_at: u64,
}

fn text(value: &Value) -> String {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| s.chars().count() <= 128 && !s.chars().any(char::is_control))
        .unwrap_or_default()
        .to_string()
}
fn number(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|n| n.try_into().ok())
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
}
impl GameRecord {
    pub(super) fn parse(data: &Value, expected_role: &str) -> Result<Self> {
        let role_id = id(&data["roleid"])
            .filter(|s| s == expected_role)
            .ok_or_else(|| {
                Failure::new(
                    FailureKind::Protocol,
                    "角色面板身份不匹配, 请在塔吉多检查绑定角色",
                )
            })?;
        Ok(Self {
            role_id,
            role_name: text(&data["rolename"]),
            server_name: text(&data["servername"]),
            stamina: number(&data["staminaValue"]),
            stamina_max: number(&data["staminaMaxValue"]),
            city_stamina: number(&data["citystaminaValue"]),
            city_stamina_max: number(&data["citystaminaMaxValue"]),
            activity: number(&data["dayvalue"]),
            weekly_remaining: number(&data["weekcopiesremainCnt"]),
            updated_at: now(),
        })
    }
}
