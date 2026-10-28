use std::io::Write;
use std::process::{Command, Stdio};

use nte_core::game_mode::gpu::{Config, Info};
use tauri::State;

#[tauri::command]
pub async fn game_mode_gpu_info() -> Result<Info, String> {
    tauri::async_runtime::spawn_blocking(nte_core::game_mode::gpu::info)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn game_mode_gpu_apply(
    state: State<'_, crate::AppState>,
    config: Config,
) -> Result<Info, String> {
    apply_configuration(
        state,
        "game-mode-gpu-apply",
        serde_json::to_vec(&config).map_err(|error| error.to_string())?,
    )
    .await?;
    game_mode_gpu_info().await
}

#[tauri::command]
pub async fn game_mode_tuning_info() -> Result<nte_core::game_mode::tuning::Config, String> {
    tauri::async_runtime::spawn_blocking(nte_core::game_mode::tuning::info)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn game_mode_tuning_apply(
    state: State<'_, crate::AppState>,
    config: nte_core::game_mode::tuning::Config,
) -> Result<nte_core::game_mode::tuning::Config, String> {
    apply_configuration(
        state,
        "game-mode-tuning-apply",
        serde_json::to_vec(&config).map_err(|error| error.to_string())?,
    )
    .await?;
    game_mode_tuning_info().await
}

async fn apply_configuration(
    state: State<'_, crate::AppState>,
    operation: &'static str,
    payload: Vec<u8>,
) -> Result<(), String> {
    if !state
        .settings
        .lock()
        .map_err(|error| error.to_string())?
        .game_mode
        .performance
    {
        return Err("请先启用 GameMode".into());
    }
    {
        let mut game = state.game.lock().map_err(|error| error.to_string())?;
        if game.starting
            || game.environment_busy
            || game.session.as_ref().is_some_and(crate::session_alive)
        {
            return Err("请先结束游戏和运行环境任务".into());
        }
        game.environment_busy = true;
    }
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        nte_core::game_mode::ensure_idle().map_err(|error| error.to_string())?;
        let helper = nte_core::recovery::sibling("nte-cli").map_err(|error| error.to_string())?;
        if !helper.is_file() {
            return Err("未找到配套的 nte-cli".into());
        }
        let mut child = Command::new("pkexec")
            .arg(helper)
            .arg(operation)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("无法获取管理员授权: {error}"))?;
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| "无法传入 GameMode 配置".to_string())?;
        let written = input.write_all(&payload);
        drop(input);
        let output = child
            .wait_with_output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(if matches!(output.status.code(), Some(126 | 127)) {
                "管理员授权已取消或被拒绝".into()
            } else {
                format!(
                    "应用 GameMode 配置失败: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )
            });
        }
        written.map_err(|error| error.to_string())?;
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    if let Ok(mut game) = state.game.lock() {
        game.environment_busy = false;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tauri::Manager;

    #[test]
    fn gpu_configuration_blocks_busy_sessions_and_clears_failed_authorization_tasks() {
        let mut env = crate::tests::EnvGuard::new();
        env.set(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent/oddity-gpu-test-bus",
        );
        let app = tauri::test::mock_app();
        app.manage(crate::AppState {
            settings: Mutex::new(crate::Settings {
                game_mode: nte_core::game_mode::Options {
                    performance: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            game: Mutex::new(crate::GameTracker {
                environment_busy: true,
                ..Default::default()
            }),
        });
        let busy =
            tauri::async_runtime::block_on(game_mode_gpu_apply(app.state(), Config::default()));
        assert!(busy.unwrap_err().contains("请先结束"));
        let tuning_busy = tauri::async_runtime::block_on(game_mode_tuning_apply(
            app.state(),
            nte_core::game_mode::tuning::Config::default(),
        ));
        assert!(tuning_busy.unwrap_err().contains("请先结束"));
        app.state::<crate::AppState>()
            .game
            .lock()
            .unwrap()
            .environment_busy = false;
        let failed =
            tauri::async_runtime::block_on(game_mode_gpu_apply(app.state(), Config::default()));
        assert!(failed.is_err());
        let tuning_failed = tauri::async_runtime::block_on(game_mode_tuning_apply(
            app.state(),
            nte_core::game_mode::tuning::Config::default(),
        ));
        assert!(tuning_failed.is_err());
        assert!(
            !app.state::<crate::AppState>()
                .game
                .lock()
                .unwrap()
                .environment_busy
        );
        app.state::<crate::AppState>()
            .settings
            .lock()
            .unwrap()
            .game_mode
            .performance = false;
        let disabled = tauri::async_runtime::block_on(game_mode_tuning_apply(
            app.state(),
            nte_core::game_mode::tuning::Config::default(),
        ));
        assert_eq!(disabled.unwrap_err(), "请先启用 GameMode");
        let disabled =
            tauri::async_runtime::block_on(game_mode_gpu_apply(app.state(), Config::default()));
        assert_eq!(disabled.unwrap_err(), "请先启用 GameMode");
    }
}
