//! 游玩记录命令与海报文件写入.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use nte_core::playtime::{PlaytimeHistory, PlaytimeStore};
use tauri::State;

const MAX_POSTER_BYTES: usize = 20 * 1024 * 1024;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// 托管在 Tauri 状态里的游玩记录. 监控线程和命令通过同一把锁串行写入.
pub struct Playtime {
    inner: Mutex<PlaytimeStore>,
}

impl Playtime {
    pub fn load(data_dir: &Path) -> Self {
        let mut store = PlaytimeStore::load(data_dir.join("playtime.json"));
        if let Err(error) = store.recover() {
            eprintln!("游玩记录恢复失败: {error:#}");
        }
        Self {
            inner: Mutex::new(store),
        }
    }

    pub fn start(&self, started_at: i64, mode: &str, distribution: &str) -> Result<(), String> {
        self.lock()?
            .start(started_at, mode, distribution)
            .map_err(|error| format!("{error:#}"))
    }

    pub fn touch(&self, now: i64) -> Result<(), String> {
        self.lock()?
            .touch(now)
            .map_err(|error| format!("{error:#}"))
    }

    pub fn finish(&self, now: i64) -> Result<(), String> {
        self.lock()?
            .finish(now)
            .map_err(|error| format!("{error:#}"))
    }

    pub fn history(&self) -> Result<PlaytimeHistory, String> {
        Ok(self.lock()?.history())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, PlaytimeStore>, String> {
        self.inner
            .lock()
            .map_err(|_| "游玩记录状态不可用".to_string())
    }
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[tauri::command]
pub fn playtime_history(playtime: State<'_, Playtime>) -> Result<PlaytimeHistory, String> {
    playtime.history()
}

#[tauri::command]
pub async fn playtime_export_poster(destination: String, data: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = decode_poster(&data)?;
        let path = destination_path(&destination)?;
        write_png(&path, &bytes)?;
        Ok(path.display().to_string())
    })
    .await
    .map_err(|error| format!("保存海报任务失败: {error}"))?
}

fn decode_poster(data: &str) -> Result<Vec<u8>, String> {
    if data.len() > MAX_POSTER_BYTES * 2 {
        return Err("海报数据超过大小上限".into());
    }
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| "海报数据格式无效".to_string())?;
    if bytes.len() > MAX_POSTER_BYTES {
        return Err("海报数据超过大小上限".into());
    }
    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err("海报数据不是 PNG 图片".into());
    }
    Ok(bytes)
}

fn destination_path(destination: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(destination);
    if !path.is_absolute() {
        return Err("海报保存路径必须是绝对路径".into());
    }
    if path.is_dir() {
        return Err("海报保存路径不能是文件夹".into());
    }
    let path = if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        path
    } else {
        path.with_extension("png")
    };
    if path.is_dir() {
        return Err("海报保存路径不能是文件夹".into());
    }
    let parent = path.parent().ok_or("海报保存文件夹不存在")?;
    if !parent.is_dir() {
        return Err("海报保存文件夹不存在".into());
    }
    Ok(path)
}

fn write_png(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "poster.png".into());
    let mut temp = None;
    for _ in 0..64 {
        let candidate = path.with_file_name(format!(
            ".{name}.oddity-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temp = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("创建海报临时文件失败: {error}")),
        }
    }
    let (temp_path, mut file) = temp.ok_or_else(|| "无法创建海报临时文件".to_string())?;
    let result = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("写入海报失败: {error}"))
        .and_then(|_| {
            fs::rename(&temp_path, path).map_err(|error| format!("保存海报失败: {error}"))
        });
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::TempDir;
    use tauri::Manager;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\noddity";

    fn encoded() -> String {
        STANDARD.encode(PNG)
    }

    #[test]
    fn records_sessions_through_the_managed_state() {
        let dir = TempDir::new("playtime-command");
        let playtime = Playtime::load(dir.path());
        playtime.start(1_000_000, "background", "cn").unwrap();
        playtime.touch(1_100_000).unwrap();
        playtime.finish(1_160_000).unwrap();
        let history = playtime.history().unwrap();
        assert_eq!(history.sessions.len(), 1);
        assert_eq!(history.sessions[0].ended_at, 1_160_000);
        assert_eq!(history.active, None);

        let reloaded = Playtime::load(dir.path());
        assert_eq!(reloaded.history().unwrap(), history);
    }

    #[test]
    fn recovers_an_active_session_while_loading() {
        let dir = TempDir::new("playtime-command-recover");
        let playtime = Playtime::load(dir.path());
        playtime.start(1_000_000, "official", "global").unwrap();
        playtime.touch(1_120_000).unwrap();
        drop(playtime);
        let recovered = Playtime::load(dir.path());
        let history = recovered.history().unwrap();
        assert_eq!(history.active, None);
        assert_eq!(history.sessions[0].ended_at, 1_120_000);
        assert_eq!(history.sessions[0].mode, "official");
    }

    #[test]
    fn exposes_history_through_the_command() {
        let dir = TempDir::new("playtime-command-history");
        let playtime = Playtime::load(dir.path());
        playtime.start(1_000_000, "background", "cn").unwrap();
        playtime.finish(1_120_000).unwrap();
        let app = tauri::test::mock_app();
        app.manage(playtime);
        let history = playtime_history(app.state()).unwrap();
        assert_eq!(history.sessions.len(), 1);
        assert!(now_millis() > 1_600_000_000_000);
    }

    #[test]
    fn saves_a_png_at_the_requested_path() {
        let dir = TempDir::new("playtime-poster");
        let destination = dir.join("poster");
        let path = destination_path(destination.to_str().unwrap()).unwrap();
        write_png(&path, &decode_poster(&encoded()).unwrap()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), PNG);
        assert_eq!(path, destination.with_extension("png"));
    }

    #[test]
    fn saves_a_png_through_the_async_command() {
        let dir = TempDir::new("playtime-poster-command");
        let destination = dir.join("poster.png");
        let path = tauri::async_runtime::block_on(playtime_export_poster(
            destination.display().to_string(),
            encoded(),
        ))
        .unwrap();
        assert_eq!(fs::read(path).unwrap(), PNG);
    }

    #[test]
    fn reports_write_failures_without_leaving_a_temporary_file() {
        let dir = TempDir::new("playtime-poster-failure");
        let missing = dir.join("missing/poster.png");
        assert!(write_png(&missing, PNG).is_err());
        let occupied = dir.join("occupied.png");
        fs::create_dir(&occupied).unwrap();
        assert!(write_png(&occupied, PNG).is_err());
    }

    #[test]
    fn rejects_an_existing_directory_as_the_poster_path() {
        let dir = TempDir::new("playtime-poster-dir");
        let occupied = dir.join("poster.png");
        fs::create_dir(&occupied).unwrap();
        let error = destination_path(occupied.to_str().unwrap()).unwrap_err();
        assert!(error.contains("文件夹"));
    }

    #[test]
    fn rejects_invalid_poster_data_and_paths() {
        assert!(decode_poster("not base64").is_err());
        assert!(decode_poster(&STANDARD.encode(b"not a png")).is_err());
        assert!(decode_poster(&"A".repeat(MAX_POSTER_BYTES * 2 + 1)).is_err());
        assert!(destination_path("relative.png").is_err());
        let dir = TempDir::new("playtime-poster-missing");
        assert!(destination_path(dir.join("missing/poster.png").to_str().unwrap()).is_err());
    }
}
