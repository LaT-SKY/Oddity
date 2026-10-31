use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nte_core::logs::{LogPreview, PreparedLogs};
use serde::Serialize;

static NEXT_EXPORT: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub struct LogExports(Arc<Mutex<Option<(String, PreparedLogs)>>>);

#[derive(Serialize)]
pub struct Preview {
    id: String,
    #[serde(flatten)]
    summary: LogPreview,
}

pub async fn prepare_log_export(state: &LogExports) -> Result<Preview, String> {
    let pending = state.0.clone();
    crate::service::spawn_blocking(move || {
        let logs = PreparedLogs::prepare(&super::data_dir().join("logs"), None)
            .map_err(|error| format!("{error:#}"))?;
        let id = NEXT_EXPORT.fetch_add(1, Ordering::Relaxed).to_string();
        let preview = Preview {
            id: id.clone(),
            summary: logs.preview.clone(),
        };
        *pending.lock().map_err(|_| "无法访问待导出日志")? = Some((id, logs));
        Ok(preview)
    })
    .await
    .map_err(|_| "日志检查任务失败")?
}

pub async fn save_log_export(
    state: &LogExports,
    id: String,
    destination: String,
) -> Result<String, String> {
    let pending = state.0.clone();
    crate::service::spawn_blocking(move || {
        let mut pending = pending.lock().map_err(|_| "无法访问待导出日志")?;
        let (_, logs) = pending
            .as_mut()
            .filter(|(current, _)| *current == id)
            .ok_or("日志预览已失效, 请重新检查")?;
        let path = logs
            .save(Path::new(&destination))
            .map_err(|error| format!("{error:#}"))?;
        *pending = None;
        Ok(path.display().to_string())
    })
    .await
    .map_err(|_| "日志保存任务失败")?
}

pub fn discard_log_export(state: &LogExports, id: String) -> Result<(), String> {
    let mut pending = state.0.lock().map_err(|_| "无法访问待导出日志")?;
    if pending.as_ref().is_some_and(|(current, _)| *current == id) {
        *pending = None;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{EnvGuard, TempDir};
    use std::fs;
    use std::io::Read;

    #[test]
    fn commands_bind_preview_save_retry_and_discard_to_the_same_snapshot() {
        let mut env = EnvGuard::new();
        let dir = TempDir::new("log-commands");
        env.set("XDG_DATA_HOME", dir.path());
        dir.write("oddity/logs/test.log", "token=command-secret\n");
        let mut app = crate::service::test_service();
        std::sync::Arc::get_mut(&mut app).unwrap().exports = LogExports::default();
        crate::service::block_on(async {
            let first = prepare_log_export(&app.exports).await.unwrap();
            assert_eq!(first.summary.file_count, 1);
            assert_eq!(first.summary.redactions.credentials, 1);
            let second = prepare_log_export(&app.exports).await.unwrap();
            assert_ne!(first.id, second.id);
            // 旧预览不能保存或释放新快照.
            discard_log_export(&app.exports, first.id.clone()).unwrap();
            assert!(
                save_log_export(&app.exports, first.id, dir.path().display().to_string())
                    .await
                    .is_err()
            );
            assert!(save_log_export(
                &app.exports,
                second.id.clone(),
                dir.join("missing").display().to_string()
            )
            .await
            .is_err());
            dir.write("oddity/logs/test.log", "unreviewed\n");
            let output = save_log_export(
                &app.exports,
                second.id.clone(),
                dir.path().display().to_string(),
            )
            .await
            .unwrap();
            let mut archive = zip::ZipArchive::new(fs::File::open(output).unwrap()).unwrap();
            let mut text = String::new();
            archive
                .by_name("test.log")
                .unwrap()
                .read_to_string(&mut text)
                .unwrap();
            assert!(text.contains("[凭据已移除]"));
            assert!(!text.contains("command-secret"));
            assert!(!text.contains("unreviewed"));
            assert!(
                save_log_export(&app.exports, second.id, dir.path().display().to_string())
                    .await
                    .is_err()
            );
            let third = prepare_log_export(&app.exports).await.unwrap();
            discard_log_export(&app.exports, third.id.clone()).unwrap();
            assert!(
                save_log_export(&app.exports, third.id, dir.path().display().to_string())
                    .await
                    .is_err()
            );
        });
        assert!(app.exports.0.lock().unwrap().is_none());
    }
}
