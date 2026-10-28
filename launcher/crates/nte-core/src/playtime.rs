//! 游玩记录的本地持久化.
//!
//! 只保存原始时间戳和启动方式, 日历聚合留给界面按本地时区完成.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths::TemporaryFile;
use crate::{Error, Result};

/// 过短的会话通常是启动失败或进程探测抖动, 不计入游玩记录.
pub const MIN_SESSION_MS: i64 = 60_000;

const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaytimeSession {
    pub started_at: i64,
    pub ended_at: i64,
    pub mode: String,
    pub distribution: String,
}

impl PlaytimeSession {
    pub fn duration_ms(&self) -> i64 {
        (self.ended_at - self.started_at).max(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivePlaytime {
    pub started_at: i64,
    pub last_seen_at: i64,
    pub mode: String,
    pub distribution: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaytimeHistory {
    pub sessions: Vec<PlaytimeSession>,
    pub active: Option<ActivePlaytime>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PlaytimeData {
    version: u32,
    sessions: Vec<PlaytimeSession>,
    active: Option<ActivePlaytime>,
}

impl Default for PlaytimeData {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            sessions: Vec::new(),
            active: None,
        }
    }
}

/// 游玩记录文件. 所有写操作先更新内存, 再用同目录临时文件原子替换.
pub struct PlaytimeStore {
    path: PathBuf,
    data: PlaytimeData,
}

impl PlaytimeStore {
    /// 读取记录. 文件损坏时保留原文件并返回空记录, 避免阻断启动.
    pub fn load(path: PathBuf) -> Self {
        let mut data = match read_data(&path) {
            Ok(Some(data)) => data,
            Ok(None) => PlaytimeData::default(),
            Err(error) => {
                eprintln!("读取游玩记录失败: {error:#}");
                backup_corrupt(&path);
                PlaytimeData::default()
            }
        };
        data.normalize();
        Self { path, data }
    }

    pub fn history(&self) -> PlaytimeHistory {
        PlaytimeHistory {
            sessions: self.data.sessions.clone(),
            active: self.data.active.clone(),
        }
    }

    /// 新一轮游戏开始. 上一次未结束的会话按最后一次心跳收束.
    pub fn start(&mut self, started_at: i64, mode: &str, distribution: &str) -> Result<()> {
        if started_at <= 0 {
            return Err(Error::Parse("游玩记录开始时间无效".into()));
        }
        if let Some(active) = self.data.active.take() {
            self.push_session(
                active.started_at,
                active.last_seen_at,
                &active.mode,
                &active.distribution,
            );
        }
        self.data.active = Some(ActivePlaytime {
            started_at,
            last_seen_at: started_at,
            mode: mode.to_string(),
            distribution: distribution.to_string(),
        });
        self.save()
    }

    /// 游戏运行期间更新心跳. 时间回拨时保持原值, 避免会话被截短.
    pub fn touch(&mut self, now: i64) -> Result<()> {
        let Some(active) = self.data.active.as_mut() else {
            return Ok(());
        };
        if now <= active.last_seen_at {
            return Ok(());
        }
        active.last_seen_at = now;
        self.save()
    }

    /// 游戏退出. 以调用时间和最后一次心跳中较晚者作为结束时间.
    pub fn finish(&mut self, now: i64) -> Result<()> {
        let Some(active) = self.data.active.take() else {
            return Ok(());
        };
        self.push_session(
            active.started_at,
            now.max(active.last_seen_at),
            &active.mode,
            &active.distribution,
        );
        self.save()
    }

    /// 启动时收束上次异常退出留下的活动会话.
    pub fn recover(&mut self) -> Result<()> {
        let Some(active) = self.data.active.take() else {
            return Ok(());
        };
        self.push_session(
            active.started_at,
            active.last_seen_at,
            &active.mode,
            &active.distribution,
        );
        self.save()
    }

    fn push_session(&mut self, started_at: i64, ended_at: i64, mode: &str, distribution: &str) {
        if ended_at - started_at < MIN_SESSION_MS {
            return;
        }
        self.data.sessions.push(PlaytimeSession {
            started_at,
            ended_at,
            mode: mode.to_string(),
            distribution: distribution.to_string(),
        });
        self.data.sessions.sort_by_key(|session| session.started_at);
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = TemporaryFile::new(&self.path)?;
        serde_json::to_writer_pretty(&temp.file, &self.data)
            .map_err(|error| Error::Parse(format!("写入游玩记录失败: {error}")))?;
        temp.file
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        temp.commit(&self.path, &|| false)
    }
}

impl PlaytimeData {
    fn normalize(&mut self) {
        self.version = FORMAT_VERSION;
        self.sessions
            .retain(|session| session.ended_at >= session.started_at && session.started_at > 0);
        self.sessions.sort_by_key(|session| session.started_at);
        if let Some(active) = self.active.as_mut() {
            if active.started_at <= 0 {
                self.active = None;
            } else if active.last_seen_at < active.started_at {
                active.last_seen_at = active.started_at;
            }
        }
    }
}

fn read_data(path: &Path) -> Result<Option<PlaytimeData>> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(Error::Parse("游玩记录文件超过读取上限".into()));
    }
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| Error::Parse(format!("游玩记录格式无效: {error}")))
}

fn backup_corrupt(path: &Path) {
    if !path.exists() {
        return;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "playtime.json".into());
    let backup = path.with_file_name(format!("{name}.corrupt-{stamp}"));
    match fs::rename(path, &backup) {
        Ok(()) => eprintln!("游玩记录已备份至 {}", backup.display()),
        Err(error) => eprintln!("备份损坏的游玩记录失败: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn started(path: &Path) -> PlaytimeStore {
        let mut store = PlaytimeStore::load(path.to_path_buf());
        store.start(1_000_000, "background", "cn").unwrap();
        store
    }

    #[test]
    fn records_a_finished_session_and_reloads_it() {
        let dir = TempDir::new("playtime-roundtrip");
        let path = dir.join("playtime.json");
        let mut store = started(&path);
        store.touch(1_030_000).unwrap();
        store.finish(1_120_000).unwrap();

        let history = store.history();
        assert_eq!(history.active, None);
        assert_eq!(history.sessions.len(), 1);
        assert_eq!(history.sessions[0].started_at, 1_000_000);
        assert_eq!(history.sessions[0].ended_at, 1_120_000);
        assert_eq!(history.sessions[0].duration_ms(), 120_000);
        assert_eq!(history.sessions[0].mode, "background");
        assert_eq!(history.sessions[0].distribution, "cn");

        let reloaded = PlaytimeStore::load(path.clone());
        assert_eq!(reloaded.history(), history);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn drops_sessions_shorter_than_one_minute() {
        let dir = TempDir::new("playtime-short");
        let path = dir.join("playtime.json");
        let mut store = started(&path);
        store.finish(1_030_000).unwrap();
        assert!(store.history().sessions.is_empty());
        assert_eq!(store.history().active, None);
    }

    #[test]
    fn recovers_an_interrupted_session_at_the_last_heartbeat() {
        let dir = TempDir::new("playtime-recover");
        let path = dir.join("playtime.json");
        let mut store = started(&path);
        store.touch(1_150_000).unwrap();
        drop(store);

        let mut recovered = PlaytimeStore::load(path.clone());
        recovered.recover().unwrap();
        let history = recovered.history();
        assert_eq!(history.active, None);
        assert_eq!(history.sessions[0].ended_at, 1_150_000);
        assert_eq!(PlaytimeStore::load(path).history(), history);
    }

    #[test]
    fn starts_a_new_session_after_finishing_a_stale_one() {
        let dir = TempDir::new("playtime-stale");
        let path = dir.join("playtime.json");
        let mut store = started(&path);
        store.touch(1_100_000).unwrap();
        store.start(2_000_000, "official", "global").unwrap();
        let history = store.history();
        assert_eq!(history.sessions.len(), 1);
        assert_eq!(history.sessions[0].ended_at, 1_100_000);
        assert_eq!(history.active.unwrap().mode, "official");
    }

    #[test]
    fn keeps_the_latest_heartbeat_when_the_clock_moves_back() {
        let dir = TempDir::new("playtime-clock");
        let path = dir.join("playtime.json");
        let mut store = started(&path);
        store.touch(1_100_000).unwrap();
        store.touch(1_050_000).unwrap();
        assert_eq!(store.history().active.unwrap().last_seen_at, 1_100_000);
        store.finish(1_060_000).unwrap();
        assert_eq!(store.history().sessions[0].ended_at, 1_100_000);
    }

    #[test]
    fn finish_without_an_active_session_is_a_noop() {
        let dir = TempDir::new("playtime-idle");
        let path = dir.join("playtime.json");
        let mut store = PlaytimeStore::load(path.clone());
        store.finish(1_000_000).unwrap();
        store.recover().unwrap();
        assert_eq!(store.history(), PlaytimeHistory::default());
        assert!(!path.exists());
    }

    #[test]
    fn rejects_an_invalid_start_timestamp() {
        let dir = TempDir::new("playtime-invalid");
        let mut store = PlaytimeStore::load(dir.join("playtime.json"));
        assert!(store.start(0, "background", "cn").is_err());
    }

    #[test]
    fn preserves_a_corrupt_file_and_starts_empty() {
        let dir = TempDir::new("playtime-corrupt");
        let path = dir.join("playtime.json");
        fs::write(&path, b"not json").unwrap();
        let store = PlaytimeStore::load(path.clone());
        assert!(store.history().sessions.is_empty());
        assert_eq!(store.history().active, None);
        let backup = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| name.starts_with("playtime.json.corrupt-"));
        assert!(backup.is_some());
        assert!(!path.exists());
    }

    #[test]
    fn normalizes_invalid_stored_sessions() {
        let dir = TempDir::new("playtime-normalize");
        let path = dir.join("playtime.json");
        fs::write(
            &path,
            br#"{
                "version": 0,
                "sessions": [
                    {"startedAt": 30, "endedAt": 10, "mode": "x", "distribution": "cn"},
                    {"startedAt": 0, "endedAt": 100, "mode": "x", "distribution": "cn"},
                    {"startedAt": 200, "endedAt": 300, "mode": "x", "distribution": "cn"}
                ],
                "active": {"startedAt": 50, "lastSeenAt": 40, "mode": "x", "distribution": "cn"}
            }"#,
        )
        .unwrap();
        let mut store = PlaytimeStore::load(path);
        let history = store.history();
        assert_eq!(history.sessions.len(), 1);
        assert_eq!(history.sessions[0].started_at, 200);
        assert_eq!(history.active.as_ref().unwrap().last_seen_at, 50);
        store.recover().unwrap();
        assert_eq!(store.history().sessions.len(), 1);
    }
}
