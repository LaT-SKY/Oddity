//! 环境快照的本地持久化.
//!
//! 写操作先写同目录临时文件, 成功后才替换内存和正式文件.
use std::collections::HashSet;
use std::fs;
use std::io::{BufWriter, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths::TemporaryFile;
use crate::{Error, Result};

use super::{Snapshot, MAX_ATTEMPTS, MAX_DETAIL_CHARS, MAX_LABEL_CHARS};

const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Starting,
    Running,
    Success,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureReason {
    Spawn,
    EarlyExit,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub reason: FailureReason,
    pub exit_code: Option<i32>,
    pub detail: Option<String>,
}

impl Failure {
    pub fn spawn(detail: impl Into<String>) -> Self {
        Self {
            reason: FailureReason::Spawn,
            exit_code: None,
            detail: detail_text(detail),
        }
    }

    pub fn early_exit(exit_code: Option<i32>) -> Self {
        Self {
            reason: FailureReason::EarlyExit,
            exit_code,
            detail: None,
        }
    }

    pub fn interrupted() -> Self {
        Self {
            reason: FailureReason::Interrupted,
            exit_code: None,
            detail: None,
        }
    }
}

fn detail_text(detail: impl Into<String>) -> Option<String> {
    let text: String = detail
        .into()
        .chars()
        .filter(|character| !character.is_control() || *character == '\n' || *character == '\t')
        .take(MAX_DETAIL_CHARS)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub id: String,
    pub captured_at: u64,
    pub status: Status,
    pub running_at: Option<u64>,
    pub ended_at: Option<u64>,
    pub duration_ms: Option<u64>,
    pub failure: Option<Failure>,
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub saved_at: u64,
    pub captured_at: u64,
    pub label: String,
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Data {
    version: u32,
    baseline_id: Option<String>,
    reference: Option<Reference>,
    attempts: Vec<Attempt>,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            baseline_id: None,
            reference: None,
            attempts: Vec::new(),
        }
    }
}

/// 环境快照文件. 所有写入都通过同一把锁串行化.
pub struct SnapshotStore {
    path: PathBuf,
    data: Data,
}

impl SnapshotStore {
    /// 读取记录. 文件损坏时保留原文件并从空记录开始, 避免阻断启动.
    pub fn load(path: PathBuf) -> Self {
        let mut data = match read_data(&path) {
            Ok(Some(data)) => data,
            Ok(None) => Data::default(),
            Err(error) => {
                eprintln!("读取环境快照失败: {error:#}");
                backup_corrupt(&path);
                Data::default()
            }
        };
        data.normalize_loaded();
        Self { path, data }
    }

    pub fn attempts(&self) -> &[Attempt] {
        &self.data.attempts
    }

    pub fn baseline(&self) -> Option<&Attempt> {
        self.data
            .baseline_id
            .as_ref()
            .and_then(|id| self.data.attempts.iter().find(|attempt| &attempt.id == id))
    }

    pub fn latest(&self) -> Option<&Attempt> {
        self.data.attempts.last()
    }

    pub fn reference(&self) -> Option<&Reference> {
        self.data.reference.as_ref()
    }

    pub fn find(&self, id: &str) -> Option<&Attempt> {
        self.data.attempts.iter().find(|attempt| attempt.id == id)
    }

    /// 新一次启动尝试; 返回本次记录的 ID.
    pub fn begin(&mut self, snapshot: Snapshot, now: u64) -> Result<String> {
        if now == 0 {
            return Err(Error::Parse("环境快照时间无效".into()));
        }
        let mut next = self.data.clone();
        let id = next.next_id(now);
        next.attempts.push(Attempt {
            id: id.clone(),
            captured_at: now,
            status: Status::Starting,
            running_at: None,
            ended_at: None,
            duration_ms: None,
            failure: None,
            snapshot,
        });
        self.commit(next)?;
        Ok(id)
    }

    /// 首次检测到游戏进程.
    pub fn mark_running(&mut self, id: &str, now: u64) -> Result<()> {
        let mut next = self.data.clone();
        let Some(attempt) = next.attempts.iter_mut().find(|attempt| attempt.id == id) else {
            return Ok(());
        };
        if attempt.status != Status::Starting {
            return Ok(());
        }
        attempt.status = Status::Running;
        attempt.running_at = Some(now.max(attempt.captured_at));
        self.commit(next)
    }

    /// 游戏连续运行满阈值后写入成功和自动基准.
    pub fn mark_success(
        &mut self,
        id: &str,
        now: u64,
        game_version: Option<String>,
        success_after_ms: u64,
    ) -> Result<()> {
        let mut next = self.data.clone();
        let Some(attempt) = next.attempts.iter_mut().find(|attempt| attempt.id == id) else {
            return Ok(());
        };
        if attempt.status == Status::Success {
            return Ok(());
        }
        if !matches!(attempt.status, Status::Starting | Status::Running) {
            return Ok(());
        }
        let start = attempt.running_at.unwrap_or(attempt.captured_at);
        if now < start.saturating_add(success_after_ms) {
            let minutes = (success_after_ms / 60_000).max(1);
            return Err(Error::Parse(format!("游戏运行时间不足 {minutes} 分钟")));
        }
        attempt.status = Status::Success;
        attempt.running_at.get_or_insert(start);
        attempt.failure = None;
        if let Some(version) = super::text(game_version) {
            attempt.snapshot.game.version = Some(version);
        }
        next.baseline_id = Some(id.to_string());
        self.commit(next)
    }

    /// 游戏退出或启动器结束. 成功状态不会被降级.
    pub fn finish(&mut self, id: &str, now: u64, failure: Option<Failure>) -> Result<()> {
        let mut next = self.data.clone();
        let Some(attempt) = next.attempts.iter_mut().find(|attempt| attempt.id == id) else {
            return Ok(());
        };
        if attempt.ended_at.is_some() {
            return Ok(());
        }
        if attempt.status == Status::Interrupted {
            return Ok(());
        }
        let start = attempt.running_at.unwrap_or(attempt.captured_at);
        let end = now.max(start);
        attempt.ended_at = Some(end);
        attempt.duration_ms = Some(end.saturating_sub(start));
        if attempt.status != Status::Success {
            attempt.status = Status::Failed;
            attempt.failure = Some(failure.unwrap_or_else(|| Failure::early_exit(None)));
        }
        self.commit(next)
    }

    /// 保存或替换参考快照. 参考快照不参与裁剪.
    pub fn save_reference(
        &mut self,
        snapshot: Snapshot,
        captured_at: u64,
        now: u64,
        label: &str,
    ) -> Result<Reference> {
        if captured_at == 0 || now == 0 {
            return Err(Error::Parse("参考快照时间无效".into()));
        }
        let reference = Reference {
            saved_at: now,
            captured_at,
            label: normalize_label(label),
            snapshot,
        };
        let mut next = self.data.clone();
        next.reference = Some(reference.clone());
        self.commit(next)?;
        Ok(reference)
    }

    pub fn delete_reference(&mut self) -> Result<()> {
        if self.data.reference.is_none() {
            return Ok(());
        }
        let mut next = self.data.clone();
        next.reference = None;
        self.commit(next)
    }

    /// 清除尝试和自动基准, 保留参考快照.
    pub fn clear_history(&mut self) -> Result<()> {
        if self.data.attempts.is_empty() && self.data.baseline_id.is_none() {
            return Ok(());
        }
        let mut next = self.data.clone();
        next.attempts.clear();
        next.baseline_id = None;
        self.commit(next)
    }

    fn commit(&mut self, mut data: Data) -> Result<()> {
        data.normalize_data();
        save_data(&self.path, &data)?;
        self.data = data;
        Ok(())
    }
}

impl Data {
    fn next_id(&self, now: u64) -> String {
        let base = format!("{now:x}-{:x}", std::process::id());
        if !self.attempts.iter().any(|attempt| attempt.id == base) {
            return base;
        }
        let mut index = 1u64;
        loop {
            let candidate = format!("{base}-{index:x}");
            if !self.attempts.iter().any(|attempt| attempt.id == candidate) {
                return candidate;
            }
            index += 1;
        }
    }

    fn normalize_loaded(&mut self) {
        for attempt in &mut self.attempts {
            if matches!(attempt.status, Status::Starting | Status::Running) {
                attempt.status = Status::Interrupted;
                attempt.failure = Some(Failure::interrupted());
            }
        }
        self.normalize_data();
    }

    fn normalize_data(&mut self) {
        self.version = FORMAT_VERSION;
        self.attempts
            .retain(|attempt| !attempt.id.is_empty() && attempt.captured_at > 0);
        let mut seen = HashSet::new();
        self.attempts
            .retain(|attempt| seen.insert(attempt.id.clone()));
        self.attempts.sort_by(|left, right| {
            left.captured_at
                .cmp(&right.captured_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        for attempt in &mut self.attempts {
            if let Some(failure) = &mut attempt.failure {
                failure.detail = failure.detail.take().and_then(detail_text);
            }
            if let Some(ended_at) = attempt.ended_at {
                let start = attempt.running_at.unwrap_or(attempt.captured_at);
                let ended_at = ended_at.max(start);
                attempt.ended_at = Some(ended_at);
                attempt.duration_ms = Some(ended_at.saturating_sub(start));
            }
        }
        if self
            .reference
            .as_ref()
            .is_some_and(|reference| reference.captured_at == 0)
        {
            self.reference = None;
        } else if let Some(reference) = &mut self.reference {
            reference.label = normalize_label(&reference.label);
        }
        if self.baseline_id.as_ref().is_some_and(|id| {
            !self
                .attempts
                .iter()
                .any(|attempt| &attempt.id == id && attempt.status == Status::Success)
        }) {
            self.baseline_id = None;
        }
        if self.baseline_id.is_none() {
            self.baseline_id = self
                .attempts
                .iter()
                .rev()
                .find(|attempt| attempt.status == Status::Success)
                .map(|attempt| attempt.id.clone());
        }
        self.prune();
    }

    fn prune(&mut self) {
        if self.attempts.len() <= MAX_ATTEMPTS {
            return;
        }
        let start = self.attempts.len() - MAX_ATTEMPTS;
        let mut kept = self.attempts.split_off(start);
        if let Some(id) = self.baseline_id.clone() {
            if !kept.iter().any(|attempt| attempt.id == id) {
                if let Some(index) = self.attempts.iter().position(|attempt| attempt.id == id) {
                    kept.insert(0, self.attempts.remove(index));
                }
            }
        }
        self.attempts = kept;
    }
}

pub fn normalize_label(label: &str) -> String {
    let text: String = label
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    let text = text.trim();
    let text = if text.is_empty() {
        "参考快照"
    } else {
        text
    };
    text.chars().take(MAX_LABEL_CHARS).collect()
}

fn read_data(path: &Path) -> Result<Option<Data>> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(Error::Parse("环境快照文件超过读取上限".into()));
    }
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| Error::Parse(format!("环境快照格式无效: {error}")))
}

fn save_data(path: &Path, data: &Data) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = TemporaryFile::new(path)?;
    let mut writer = BufWriter::new(&temp.file);
    serde_json::to_writer_pretty(&mut writer, data)
        .map_err(|error| Error::Parse(format!("写入环境快照失败: {error}")))?;
    writer.flush()?;
    drop(writer);
    temp.file
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    temp.commit(path, &|| false)
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
        .unwrap_or_else(|| "environment-snapshots.json".into());
    let backup = path.with_file_name(format!("{name}.corrupt-{stamp}"));
    match fs::rename(path, &backup) {
        Ok(()) => eprintln!("环境快照已备份至 {}", backup.display()),
        Err(error) => eprintln!("备份损坏的环境快照失败: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::distribution::Distribution;
    use crate::snapshots::SUCCESS_AFTER_MS;
    use crate::snapshots::{Game, Gamescope, Graphics, Launcher, Runtime, System};
    use crate::testing::TempDir;

    fn snapshot(tag: &str) -> Snapshot {
        Snapshot {
            launcher: Launcher {
                version: tag.into(),
                package: "系统安装".into(),
            },
            system: System {
                os: Some("Arch Linux".into()),
                kernel: Some("6.17.1".into()),
                session: Some("wayland".into()),
                desktop: Some("KDE".into()),
            },
            graphics: Graphics {
                gpus: Vec::new(),
                opengl: None,
            },
            game: Game {
                distribution: Distribution::Cn,
                install_root: Some("/game".into()),
                launcher_root: Some("/game/NTELauncher".into()),
                version: Some(tag.into()),
                launcher_version: Some("1.0".into()),
            },
            runtime: Runtime {
                environment_id: Some("env".into()),
                environment_name: Some("稳定环境".into()),
                proton_name: Some("GE-Proton9-20".into()),
                proton_identity: Some("pin".into()),
                proton_ok: true,
                prefix: Some("/prefix".into()),
                prefix_ok: true,
                launch_mode: "background".into(),
                gamescope: Gamescope::default(),
                umu: Some("/usr/bin/umu-run".into()),
            },
        }
    }

    fn store(tag: &str) -> (TempDir, PathBuf, SnapshotStore) {
        let dir = TempDir::new(tag);
        let path = dir.join("environment-snapshots.json");
        let store = SnapshotStore::load(path.clone());
        (dir, path, store)
    }

    #[test]
    fn records_a_success_and_reloads_it() {
        let (_dir, path, mut store) = store("snapshot-roundtrip");
        let id = store.begin(snapshot("a"), 1_000_000).unwrap();
        store.mark_running(&id, 1_005_000).unwrap();
        store
            .mark_success(&id, 1_065_000, Some("1.0.8".into()), SUCCESS_AFTER_MS)
            .unwrap();
        store.finish(&id, 1_100_000, None).unwrap();

        let attempt = store.baseline().unwrap();
        assert_eq!(attempt.status, Status::Success);
        assert_eq!(attempt.snapshot.game.version.as_deref(), Some("1.0.8"));
        assert_eq!(attempt.duration_ms, Some(95_000));

        let reloaded = SnapshotStore::load(path.clone());
        assert_eq!(reloaded.baseline().unwrap().id, id);
        assert_eq!(reloaded.baseline().unwrap().status, Status::Success);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn success_requires_the_running_threshold() {
        let (_dir, _path, mut store) = store("snapshot-threshold");
        let id = store.begin(snapshot("a"), 1_000_000).unwrap();
        store.mark_running(&id, 1_001_000).unwrap();
        assert!(store
            .mark_success(&id, 1_030_000, None, SUCCESS_AFTER_MS)
            .is_err());
        assert_eq!(store.latest().unwrap().status, Status::Running);
        store
            .mark_success(&id, 1_061_000, None, SUCCESS_AFTER_MS)
            .unwrap();
        assert_eq!(store.baseline().unwrap().status, Status::Success);
    }

    #[test]
    fn success_respects_a_configured_threshold() {
        let (_dir, _path, mut store) = store("snapshot-custom-threshold");
        let id = store.begin(snapshot("a"), 1_000_000).unwrap();
        store.mark_running(&id, 1_000_000).unwrap();
        assert!(store
            .mark_success(&id, 1_299_999, None, 5 * 60_000)
            .is_err());
        store
            .mark_success(&id, 1_300_000, None, 5 * 60_000)
            .unwrap();
        assert_eq!(store.baseline().unwrap().status, Status::Success);
    }

    #[test]
    fn keeps_the_baseline_when_pruning_attempts() {
        let (_dir, _path, mut store) = store("snapshot-prune");
        let id = store.begin(snapshot("a"), 1_000_000).unwrap();
        store.mark_running(&id, 1_001_000).unwrap();
        store
            .mark_success(&id, 1_061_000, None, SUCCESS_AFTER_MS)
            .unwrap();
        for index in 0..(MAX_ATTEMPTS + 10) {
            store
                .begin(
                    snapshot(&format!("{index}")),
                    2_000_000 + u64::try_from(index).unwrap() * 1_000,
                )
                .unwrap();
        }
        assert!(store.attempts().len() <= MAX_ATTEMPTS + 1);
        assert!(store.attempts().iter().any(|attempt| attempt.id == id));
        assert_eq!(store.baseline().unwrap().id, id);
    }

    #[test]
    fn reference_survives_success_and_history_cleanup() {
        let (_dir, _path, mut store) = store("snapshot-reference");
        let reference = store
            .save_reference(snapshot("ref"), 1_000_000, 1_000_100, "  更新前\n ")
            .unwrap();
        assert_eq!(reference.label, "更新前");

        let id = store.begin(snapshot("a"), 2_000_000).unwrap();
        store.mark_running(&id, 2_001_000).unwrap();
        store
            .mark_success(&id, 2_061_000, None, SUCCESS_AFTER_MS)
            .unwrap();
        assert_eq!(store.reference().unwrap().label, "更新前");

        store.clear_history().unwrap();
        assert!(store.attempts().is_empty());
        assert!(store.baseline().is_none());
        assert!(store.reference().is_some());

        store.delete_reference().unwrap();
        assert!(store.reference().is_none());
    }

    #[test]
    fn replacing_a_reference_keeps_only_the_latest() {
        let (_dir, _path, mut store) = store("snapshot-reference-replace");
        store
            .save_reference(snapshot("first"), 1_000_000, 1_000_100, "一")
            .unwrap();
        store
            .save_reference(snapshot("second"), 2_000_000, 2_000_100, "二")
            .unwrap();
        assert_eq!(store.reference().unwrap().label, "二");
        assert_eq!(
            store.reference().unwrap().snapshot.game.version.as_deref(),
            Some("second")
        );
    }

    #[test]
    fn marks_stale_attempts_as_interrupted() {
        let (_dir, path, mut store) = store("snapshot-interrupted");
        let id = store.begin(snapshot("a"), 1_000_000).unwrap();
        store.mark_running(&id, 1_001_000).unwrap();
        drop(store);

        let reloaded = SnapshotStore::load(path);
        let attempt = reloaded.latest().unwrap();
        assert_eq!(attempt.status, Status::Interrupted);
        assert_eq!(
            attempt.failure.as_ref().unwrap().reason,
            FailureReason::Interrupted
        );
        assert!(reloaded.baseline().is_none());
    }

    #[test]
    fn failed_save_does_not_expose_unpersisted_state() {
        let dir = TempDir::new("snapshot-save-failure");
        let parent = dir.join("blocked");
        fs::write(&parent, b"file").unwrap();
        let mut store = SnapshotStore::load(parent.join("environment-snapshots.json"));
        assert!(store.begin(snapshot("a"), 1_000_000).is_err());
        assert!(store.attempts().is_empty());
        assert!(store.baseline().is_none());
    }

    #[test]
    fn preserves_a_corrupt_file_and_starts_empty() {
        let dir = TempDir::new("snapshot-corrupt");
        let path = dir.join("environment-snapshots.json");
        fs::write(&path, b"not json").unwrap();
        let store = SnapshotStore::load(path.clone());
        assert!(store.attempts().is_empty());
        assert!(store.baseline().is_none());
        let backup = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| name.starts_with("environment-snapshots.json.corrupt-"));
        assert!(backup.is_some());
        assert!(!path.exists());
    }

    #[test]
    fn normalizes_old_and_invalid_records() {
        let dir = TempDir::new("snapshot-normalize");
        let path = dir.join("environment-snapshots.json");
        fs::write(
            &path,
            br#"{
                "version": 0,
                "baselineId": "missing",
                "reference": {"savedAt": 10, "capturedAt": 10, "label": "", "snapshot": {
                    "launcher": {"version": "a", "package": "p"},
                    "system": {"os": null, "kernel": null, "session": null, "desktop": null},
                    "graphics": {"gpus": [], "opengl": null},
                    "game": {"distribution": "cn", "installRoot": null, "launcherRoot": null, "version": null, "launcherVersion": null},
                    "runtime": {"environmentId": null, "environmentName": null, "protonName": null, "protonIdentity": null, "protonOk": false, "prefix": null, "prefixOk": false, "launchMode": "background", "gamescope": {"enabled": false, "width": 0, "height": 0, "fps": 0}, "umu": null}
                }},
                "attempts": [
                    {"id": "one", "capturedAt": 20, "status": "running", "snapshot": {
                        "launcher": {"version": "b", "package": "p"},
                        "system": {"os": null, "kernel": null, "session": null, "desktop": null},
                        "graphics": {"gpus": [], "opengl": null},
                        "game": {"distribution": "cn", "installRoot": null, "launcherRoot": null, "version": null, "launcherVersion": null},
                        "runtime": {"environmentId": null, "environmentName": null, "protonName": null, "protonIdentity": null, "protonOk": false, "prefix": null, "prefixOk": false, "launchMode": "background", "gamescope": {"enabled": false, "width": 0, "height": 0, "fps": 0}, "umu": null}
                    }}
                ]
            }"#,
        )
        .unwrap();
        let store = SnapshotStore::load(path);
        assert_eq!(store.attempts().len(), 1);
        assert_eq!(store.latest().unwrap().id, "one");
        assert_eq!(store.latest().unwrap().status, Status::Interrupted);
        assert_eq!(store.reference().unwrap().label, "参考快照");
        assert!(store.baseline().is_none());
    }
}
