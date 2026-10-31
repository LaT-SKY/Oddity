//! 命令参数在此反序列化一次, 业务层只接收具体类型.
use crate::service::ServiceHandle;
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
pub struct Request {
    pub id: u64,
    pub command: String,
    #[serde(default)]
    pub args: Value,
}
pub async fn dispatch(app: ServiceHandle, command: String, args: Value) -> Result<Value, Value> {
    match command.as_str() {
        "attendance_status" => serde_json::to_value(
            crate::attendance_commands::attendance_status()
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "attendance_refresh_game_records" => serde_json::to_value(
            crate::attendance_commands::attendance_refresh_game_records(&app.state)
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "attendance_login" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                phone: String,
                password: String,
                remember_password: bool,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::attendance_commands::attendance_login(
                    &app.state,
                    args.phone,
                    args.password,
                    args.remember_password,
                )
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "attendance_send_code" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                phone: String,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::attendance_commands::attendance_send_code(&app.state, args.phone)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "attendance_login_sms" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                phone: String,
                code: String,
                device_id: String,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::attendance_commands::attendance_login_sms(
                    &app.state,
                    args.phone,
                    args.code,
                    args.device_id,
                )
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "attendance_configure" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                preferences: nte_core::attendance::Preferences,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::attendance_commands::attendance_configure(args.preferences)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "attendance_run" => serde_json::to_value(
            crate::attendance_commands::attendance_run(&app.state)
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "attendance_logout" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                account_id: String,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::attendance_commands::attendance_logout(args.account_id)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "attendance_keyring_check" => serde_json::to_value(
            crate::attendance_commands::attendance_keyring_check()
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "game_download_status" => serde_json::to_value(
            crate::download_commands::game_download_status(&app.downloads),
        )
        .map_err(|e| json!(e.to_string())),
        "game_download_start" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                mode: String,
                full: bool,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::download_commands::game_download_start(app.clone(), args.mode, args.full)
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "game_download_cancel" => serde_json::to_value(
            crate::download_commands::game_download_cancel(&app.downloads),
        )
        .map_err(|e| json!(e.to_string())),
        "environment_create" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                name: String,
                proton_path: String,
                source_id: Option<String>,
                distribution: nte_core::distribution::Distribution,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::environment_commands::environment_create(
                    app.clone(),
                    args.name,
                    args.proton_path,
                    args.source_id,
                    args.distribution,
                )
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "environment_action" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                action: String,
                id: Option<String>,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::environment_commands::environment_action(app.clone(), args.action, args.id)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "browse_files" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                path: Option<String>,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::file_commands::browse_files(args.path)
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "game_mode_gpu_info" => serde_json::to_value(
            crate::game_mode_commands::game_mode_gpu_info()
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "game_mode_gpu_apply" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                config: nte_core::game_mode::gpu::Config,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::game_mode_commands::game_mode_gpu_apply(&app.state, args.config)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "game_mode_tuning_info" => serde_json::to_value(
            crate::game_mode_commands::game_mode_tuning_info()
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "game_mode_tuning_apply" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                config: nte_core::game_mode::tuning::Config,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::game_mode_commands::game_mode_tuning_apply(&app.state, args.config)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "gamepad_poll" => serde_json::to_value(
            crate::gamepad_commands::gamepad_poll(app.clone())
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "gamepad_feedback" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                device_id: u32,
                effect: crate::gamepad_commands::Feedback,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::gamepad_commands::gamepad_feedback(app.clone(), args.device_id, args.effect)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "gamepad_profile_validate" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                profile: nte_core::gamepad::Profile,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::gamepad_commands::gamepad_profile_validate(args.profile)
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "gamepad_profile_load" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                path: String,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::gamepad_commands::gamepad_profile_load(args.path)
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "gamepad_profile_save" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                path: String,
                profile: nte_core::gamepad::Profile,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::gamepad_commands::gamepad_profile_save(args.path, args.profile)
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "gamepad_release_idle" => serde_json::to_value(
            crate::gamepad_commands::gamepad_release_idle(app.clone())
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "prepare_log_export" => serde_json::to_value(
            crate::log_commands::prepare_log_export(&app.exports)
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "save_log_export" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                id: String,
                destination: String,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::log_commands::save_log_export(&app.exports, args.id, args.destination)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "discard_log_export" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                id: String,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::log_commands::discard_log_export(&app.exports, args.id)
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "bootstrap" => {
            serde_json::to_value(crate::bootstrap(&app.state)).map_err(|e| json!(e.to_string()))
        }
        "launcher_background" => serde_json::to_value(crate::launcher_background(&app.state))
            .map_err(|e| json!(e.to_string())),
        "check_update" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                full: bool,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::check_update(&app.state, args.full)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "fetch_bulletins" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                distribution: nte_core::distribution::Distribution,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::fetch_bulletins(args.distribution)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "save_settings" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                settings: crate::Settings,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::save_settings(&app.state, &app.downloads, args.settings)
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "open_url" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                url: String,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::open_url(args.url)
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "open_directory" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                path: String,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::open_directory(args.path)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "launch_official" => serde_json::to_value(
            crate::launch_official(app.clone(), &app.state)
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "game_mode_status" => serde_json::to_value(
            crate::game_mode_status(&app.state)
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "game_status" => {
            serde_json::to_value(crate::game_status(&app.state)).map_err(|e| json!(e.to_string()))
        }
        "take_launch_error" => serde_json::to_value(crate::take_launch_error(&app.state))
            .map_err(|e| json!(e.to_string())),
        "cancel_launch" => serde_json::to_value(
            crate::cancel_launch(app.clone(), &app.state)
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "close_game" => serde_json::to_value(
            crate::close_game(app.clone(), &app.state)
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "playtime_history" => serde_json::to_value(
            crate::playtime_commands::playtime_history(&app.playtime)
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "playtime_export_poster" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                destination: String,
                data: String,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::playtime_commands::playtime_export_poster(args.destination, args.data)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "environment_snapshot_overview" => serde_json::to_value(
            crate::snapshot_commands::environment_snapshot_overview(&app.snapshots)
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "environment_snapshot_compare" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                target: String,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::snapshot_commands::environment_snapshot_compare(app.clone(), args.target)
                    .await
                    .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "environment_snapshot_save_reference" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                label: Option<String>,
                attempt_id: Option<String>,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            serde_json::to_value(
                crate::snapshot_commands::environment_snapshot_save_reference(
                    app.clone(),
                    args.label,
                    args.attempt_id,
                )
                .await
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
            )
            .map_err(|e| json!(e.to_string()))
        }
        "environment_snapshot_delete_reference" => serde_json::to_value(
            crate::snapshot_commands::environment_snapshot_delete_reference(&app.snapshots)
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "environment_snapshot_clear_history" => serde_json::to_value(
            crate::snapshot_commands::environment_snapshot_clear_history(&app.snapshots)
                .map_err(|e| serde_json::to_value(e).unwrap_or(Value::Null))?,
        )
        .map_err(|e| json!(e.to_string())),
        "host_state" => {
            #[derive(Deserialize)]
            struct Args {
                focused: bool,
                visible: bool,
            }
            let state: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            app.focused
                .store(state.focused, std::sync::atomic::Ordering::Release);
            app.visible
                .store(state.visible, std::sync::atomic::Ordering::Release);
            if !state.focused || !state.visible {
                app.input_task(crate::gamepad_commands::stop)
                    .map_err(|e| json!(e))?;
            }
            Ok(Value::Null)
        }
        "external_launch" => {
            #[derive(Deserialize)]
            struct Args {
                argv: Vec<String>,
                cold: bool,
            }
            let args: Args = serde_json::from_value(args).map_err(|e| json!(e.to_string()))?;
            let argv = args
                .argv
                .into_iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>();
            let request = crate::cli::parse(&argv).map_err(|e| json!(e))?;
            if let Some(request) = request {
                crate::handle_external_launch(
                    app,
                    request,
                    if args.cold {
                        crate::LaunchOrigin::ColdStart
                    } else {
                        crate::LaunchOrigin::Forwarded
                    },
                )
                .await;
            }
            Ok(Value::Null)
        }
        _ => Err(json!("无法识别的宿主命令")),
    }
}
