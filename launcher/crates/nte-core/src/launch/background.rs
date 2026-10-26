use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use crate::{Error, Result};

const CN_LAUNCHER_MD5: &str = "81c779899c4a53b47651c51afa5dea1c";
const GLOBAL_LAUNCHER_MD5: &str = "c55d49312fbcc7d41ead802faa25281b";
const READY_MARKER: &[u8] = b"[GameClientAgent::_setLocalResChecked] 0->1";
const LOG_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickProfile {
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
}

impl ClickProfile {
    pub fn argument(self) -> String {
        format!("{},{},{},{}", self.width, self.height, self.x, self.y)
    }
}

pub fn validated_click_profile(launcher: &Path) -> Result<ClickProfile> {
    profile_for_md5(&crate::update::md5_file(launcher)?)
}

fn profile_for_md5(checksum: &str) -> Result<ClickProfile> {
    if ![CN_LAUNCHER_MD5, GLOBAL_LAUNCHER_MD5].contains(&checksum) {
        return Err(Error::Unsupported(
            "当前官方启动器版本尚未验证后台启动, 请使用官方启动器窗口启动游戏.".into(),
        ));
    }
    Ok(ClickProfile {
        width: 1280,
        height: 720,
        x: 1120,
        y: 636,
    })
}

#[derive(Debug, Default)]
struct LogTail {
    offset: u64,
    bytes: Vec<u8>,
}

impl LogTail {
    fn read(path: &Path) -> io::Result<Self> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        let offset = file.metadata()?.len().saturating_sub(LOG_LIMIT as u64);
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::new();
        file.take(LOG_LIMIT as u64).read_to_end(&mut bytes)?;
        Ok(Self { offset, bytes })
    }

    fn matches_at(&self, absolute_offset: u64, bytes: &[u8]) -> bool {
        let Some(relative) = absolute_offset.checked_sub(self.offset) else {
            return false;
        };
        let Ok(relative) = usize::try_from(relative) else {
            return false;
        };
        self.bytes
            .get(relative..relative.saturating_add(bytes.len()))
            == Some(bytes)
    }
}

/// 保存启动前的日志片段, 不让旧的资源检查记录触发本轮启动.
pub struct FreshLog {
    before: LogTail,
}

impl std::fmt::Debug for FreshLog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FreshLog")
            .field("offset", &self.before.offset)
            .field("bytes", &self.before.bytes.len())
            .finish()
    }
}

impl FreshLog {
    pub fn capture(path: &Path) -> io::Result<Self> {
        Ok(Self {
            before: LogTail::read(path)?,
        })
    }

    pub fn is_ready(&self, path: &Path) -> io::Result<bool> {
        let current = LogTail::read(path)?;
        let mut offset = 0;
        for line in current.bytes.split_inclusive(|byte| *byte == b'\n') {
            // 尾部读取可能从半行开始, 尚未写完的记录也留到下次检查.
            let complete = line.last() == Some(&b'\n') && (offset != 0 || current.offset == 0);
            if complete
                && line.trim_ascii_end().ends_with(READY_MARKER)
                && !self.before.matches_at(current.offset + offset as u64, line)
            {
                return Ok(true);
            }
            offset += line.len();
        }
        Ok(false)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SessionStage {
    #[default]
    Starting,
    HelperStarted,
    NativeStarted,
    OfficialStartClickSent,
    GameStarted,
    GameExited,
    NativeExited,
    HelperExited,
    SessionExited,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SessionStatus {
    pub stage: SessionStage,
    pub game_started: bool,
    pub game_exited: bool,
    pub exit_code: Option<u32>,
    pub cleanup_incomplete: bool,
    failed: bool,
    finished: bool,
}

impl SessionStatus {
    pub fn parse(text: &str) -> Self {
        let mut status = Self::default();
        for line in text.split_inclusive('\n') {
            if !line.ends_with('\n') {
                continue;
            }
            let event = line.split_whitespace().next().unwrap_or("");
            match event {
                "HELPER_STARTED" => status.stage = SessionStage::HelperStarted,
                "NATIVE_STARTED" => status.stage = SessionStage::NativeStarted,
                "OFFICIAL_START_CLICK_SENT" => status.stage = SessionStage::OfficialStartClickSent,
                "GAME_STARTED" => {
                    status.stage = SessionStage::GameStarted;
                    status.game_started = true;
                }
                "GAME_EXITED" => {
                    status.stage = SessionStage::GameExited;
                    status.game_exited = true;
                }
                "NATIVE_EXITED" => status.stage = SessionStage::NativeExited,
                "HELPER_EXITED" => status.stage = SessionStage::HelperExited,
                "SESSION_EXITED" => {
                    status.stage = SessionStage::SessionExited;
                    status.exit_code = line
                        .split_whitespace()
                        .find_map(|field| field.strip_prefix("code="))
                        .and_then(|value| value.parse().ok());
                    status.finished = status.exit_code.is_some();
                    status.failed |= !matches!(status.exit_code, Some(0));
                }
                "SESSION_CLEANUP_INCOMPLETE" => status.cleanup_incomplete = true,
                "ERR" => status.failed = true,
                _ => {}
            }
        }
        status
    }

    pub fn finished(&self) -> bool {
        self.finished && !self.cleanup_incomplete
    }

    pub fn error(&self) -> Option<&'static str> {
        if self.cleanup_incomplete {
            Some("官方启动器后台进程尚未完成清理.")
        } else if self.failed {
            Some("官方启动器后台会话失败, 请检查启动诊断记录.")
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    const READY: &str = "[12:00:00] [GameClientAgent::_setLocalResChecked] 0->1\n";

    #[test]
    fn click_profile_requires_a_verified_binary() {
        assert_eq!(
            profile_for_md5(CN_LAUNCHER_MD5).unwrap().argument(),
            "1280,720,1120,636"
        );
        assert_eq!(
            profile_for_md5(GLOBAL_LAUNCHER_MD5).unwrap().argument(),
            "1280,720,1120,636"
        );
        let dir = TempDir::new("background-profile");
        let path = dir.join("NTEGame.exe");
        std::fs::write(&path, b"unknown launcher build").unwrap();
        assert!(matches!(
            validated_click_profile(&path),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn old_marker_and_unrelated_appends_are_not_fresh() {
        let dir = TempDir::new("background-old-log");
        let path = dir.join("NTEGame.log");
        std::fs::write(&path, READY).unwrap();
        let snapshot = FreshLog::capture(&path).unwrap();
        assert!(!snapshot.is_ready(&path).unwrap());
        std::fs::write(&path, format!("{READY}unrelated line\n")).unwrap();
        assert!(!snapshot.is_ready(&path).unwrap());
        std::fs::write(&path, format!("{READY}unrelated line\n{READY}")).unwrap();
        assert!(snapshot.is_ready(&path).unwrap());
    }

    #[test]
    fn truncation_rotation_and_same_size_rewrite_are_fresh() {
        for mode in ["truncate", "rotate", "same-size"] {
            let dir = TempDir::new(mode);
            let path = dir.join("NTEGame.log");
            let old = if mode == "same-size" {
                READY.replace("0->1", "0->0")
            } else {
                format!("old log padding\n{READY}old trailer\n")
            };
            std::fs::write(&path, old).unwrap();
            let snapshot = FreshLog::capture(&path).unwrap();
            if mode == "rotate" {
                std::fs::rename(&path, dir.join("NTEGame.log.1")).unwrap();
            }
            std::fs::write(&path, READY).unwrap();
            assert!(snapshot.is_ready(&path).unwrap(), "{mode}");
        }
    }

    #[test]
    fn missing_and_partial_logs_wait_for_a_complete_record() {
        let dir = TempDir::new("background-partial-log");
        let path = dir.join("NTEGame.log");
        let snapshot = FreshLog::capture(&path).unwrap();
        assert!(!snapshot.is_ready(&path).unwrap());
        std::fs::write(&path, READY.trim_end()).unwrap();
        assert!(!snapshot.is_ready(&path).unwrap());
        std::fs::write(&path, READY.replace("0->1", "0->10")).unwrap();
        assert!(!snapshot.is_ready(&path).unwrap());
        std::fs::write(&path, READY).unwrap();
        assert!(snapshot.is_ready(&path).unwrap());
    }

    #[test]
    fn log_reads_are_bounded_and_retained_old_tail_does_not_trigger() {
        let dir = TempDir::new("background-bounded-log");
        let path = dir.join("NTEGame.log");
        let mut content = vec![b'x'; LOG_LIMIT + 256];
        content.extend_from_slice(b"\n");
        content.extend_from_slice(READY.as_bytes());
        std::fs::write(&path, &content).unwrap();
        let snapshot = FreshLog::capture(&path).unwrap();
        assert!(snapshot.before.bytes.len() <= LOG_LIMIT);
        content.extend_from_slice(b"unrelated\n");
        std::fs::write(&path, &content).unwrap();
        assert!(!snapshot.is_ready(&path).unwrap());
        content.extend_from_slice(READY.as_bytes());
        std::fs::write(&path, &content).unwrap();
        assert!(snapshot.is_ready(&path).unwrap());
    }

    #[test]
    fn status_tracks_game_and_clean_session_completion() {
        let status = SessionStatus::parse("HELPER_STARTED windows_pid=4\nNATIVE_STARTED windows_pid=8\nOFFICIAL_START_CLICK_SENT\nGAME_STARTED windows_pid=12\nGAME_EXITED code=0\nNATIVE_EXITED code=0\nHELPER_EXITED code=0\nSESSION_EXITED code=0\n");
        assert!(status.game_started && status.game_exited && status.finished());
        assert_eq!(status.stage, SessionStage::SessionExited);
        assert_eq!(status.error(), None);
    }

    #[test]
    fn status_does_not_echo_errors_or_accept_partial_completion() {
        assert!(!SessionStatus::parse("SESSION_EXITED code=").finished());
        assert!(!SessionStatus::parse("SESSION_EXITED code=0").finished());
        let failed = SessionStatus::parse("ERR diagnostic details\nSESSION_EXITED code=6\n");
        assert!(failed.finished());
        assert_eq!(failed.exit_code, Some(6));
        assert!(failed
            .error()
            .is_some_and(|error| !error.contains("diagnostic details")));
        let incomplete = SessionStatus::parse("SESSION_CLEANUP_INCOMPLETE\n");
        assert!(!incomplete.finished());
        assert!(incomplete.error().is_some());
    }
}
