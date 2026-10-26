//! 运行环境操作与启动/资源任务共用互斥边界.
use std::path::{Path, PathBuf};

use nte_core::environment::{self, ProtonPin, RuntimeEnvironment};
use tauri::{AppHandle, Manager};

use crate::{
    bootstrap_impl, data_dir, default_prefix, detect_launcher_root, detect_proton,
    effective_install_root, expand_path, persist_settings, session_alive, AppState, Bootstrap,
    Settings,
};

pub fn selected(settings: &Settings) -> Result<Option<&RuntimeEnvironment>, String> {
    settings
        .active_environment
        .as_ref()
        .map(|id| {
            settings
                .runtime_environments
                .iter()
                .find(|environment| &environment.id == id)
                .ok_or_else(|| "所选运行环境不存在, 请切回原始环境".into())
        })
        .transpose()
}

pub fn prefix(settings: &Settings) -> Result<PathBuf, String> {
    if let Some(environment) = selected(settings)? {
        let launcher = detect_launcher_root(settings);
        let install = effective_install_root(settings, launcher.as_deref());
        if !environment.matches(&install, settings.distribution) {
            return Err("运行环境与当前游戏地区或安装目录不匹配, 请切换环境".into());
        }
        environment
            .prefix(&data_dir())
            .map_err(|error| format!("{error:#}"))
    } else {
        Ok(settings
            .prefix
            .as_deref()
            .map(expand_path)
            .unwrap_or_else(default_prefix))
    }
}

pub fn verify_proton(settings: &Settings) -> Result<(), String> {
    let pin = selected(settings)?
        .map(|profile| &profile.proton)
        .or(settings.original_proton_pin.as_ref());
    if let Some(pin) = pin {
        pin.verify().map_err(|error| format!("{error:#}"))?;
    }
    Ok(())
}

pub fn log_dir(settings: &Settings) -> Result<PathBuf, String> {
    match selected(settings)? {
        Some(environment) => {
            environment
                .directory(&data_dir())
                .map_err(|error| error.to_string())?;
            Ok(data_dir().join("logs/environments").join(&environment.id))
        }
        None => Ok(data_dir().join("logs")),
    }
}

struct Task<'a>(&'a AppState);

impl<'a> Task<'a> {
    fn begin(
        state: &'a AppState,
        downloads: &crate::download_commands::DownloadManager,
    ) -> Result<Self, String> {
        let downloads = downloads.inner.lock().map_err(|error| error.to_string())?;
        if crate::download_commands::busy(&downloads.status.phase) {
            return Err("游戏资源任务正在进行, 暂时无法操作运行环境".into());
        }
        let mut game = state.game.lock().map_err(|error| error.to_string())?;
        if game.environment_busy
            || game.starting
            || game
                .session
                .as_ref()
                .is_some_and(|session| session.closing || session_alive(session))
        {
            return Err("游戏或运行环境任务正在进行, 请结束后重试".into());
        }
        game.environment_busy = true;
        Ok(Self(state))
    }
}

impl Drop for Task<'_> {
    fn drop(&mut self) {
        if let Ok(mut game) = self.0.game.lock() {
            game.environment_busy = false;
        }
    }
}

fn validate_selection(settings: &Settings) -> Result<(), String> {
    prefix(settings)?;
    verify_proton(settings)?;
    if detect_proton(settings).is_none() {
        return Err("未找到所选环境的 Proton".into());
    }
    Ok(())
}

pub fn validate_settings_change(
    current: &Settings,
    next: &Settings,
    busy: bool,
) -> Result<(), String> {
    if current.runtime_environments != next.runtime_environments
        || current.active_environment != next.active_environment
        || current.stable_environment != next.stable_environment
        || current.original_proton_pin != next.original_proton_pin
    {
        return Err("运行环境已变化, 请重新检测后重试".into());
    }
    if busy
        && (crate::download_paths_changed(current, next)
            || current.prefix != next.prefix
            || current.proton_path != next.proton_path
            || current.launch_mode != next.launch_mode)
    {
        return Err("游戏或运行环境任务正在进行, 暂时无法修改运行路径或启动方式".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn environment_create(
    app: AppHandle,
    name: String,
    proton_path: String,
    source_id: Option<String>,
) -> Result<Bootstrap, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let downloads = app.state::<crate::download_commands::DownloadManager>();
        create_impl(
            &state,
            &downloads,
            source_id.as_deref(),
            &name,
            &proton_path,
        )
    })
    .await
    .map_err(|error| format!("创建运行环境任务失败: {error}"))?
}

fn create_impl(
    state: &AppState,
    downloads: &crate::download_commands::DownloadManager,
    source_id: Option<&str>,
    name: &str,
    proton_path: &str,
) -> Result<Bootstrap, String> {
    let _task = Task::begin(state, downloads)?;
    let mut snapshot = state
        .settings
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    snapshot.active_environment = source_id.map(str::to_owned);
    validate_selection(&snapshot)?;
    if snapshot
        .runtime_environments
        .iter()
        .any(|entry| entry.name == name.trim())
    {
        return Err("运行环境名称已存在".into());
    }
    let source = prefix(&snapshot)?;
    let source_proton = detect_proton(&snapshot).ok_or("源环境 Proton 不可用")?;
    let launcher = detect_launcher_root(&snapshot).ok_or("未找到游戏安装目录")?;
    let install = effective_install_root(&snapshot, Some(&launcher));
    let _install_lock = nte_core::install_lock::InstallLock::acquire(&install)
        .map_err(|error| error.to_string())?;
    let original_pin = if snapshot.original_proton_pin.is_none() {
        let mut original = snapshot.clone();
        original.active_environment = None;
        let proton = detect_proton(&original).ok_or("原始环境 Proton 不可用")?;
        Some(ProtonPin::capture(&proton.root).map_err(|error| format!("{error:#}"))?)
    } else {
        snapshot.original_proton_pin.clone()
    };
    let created = environment::create(
        &data_dir(),
        &source,
        &source_proton.root,
        &expand_path(proton_path),
        &install,
        snapshot.distribution,
        name,
    )
    .map_err(|error| format!("创建运行环境失败: {error:#}"))?;
    let mut settings = state.settings.lock().map_err(|error| error.to_string())?;
    let mut next = settings.clone();
    if next.original_proton_pin.is_none() {
        next.original_proton_pin = original_pin;
        next.proton_path = next
            .original_proton_pin
            .as_ref()
            .map(|pin| pin.path.display().to_string());
        if next.prefix.is_none() {
            next.prefix = Some(default_prefix().display().to_string());
        }
    }
    next.runtime_environments.push(created.clone());
    if let Err(error) = persist_settings(&next) {
        let cleanup = environment::discard(&data_dir(), &created);
        return Err(format!(
            "{error}{}",
            cleanup
                .err()
                .map(|error| format!("; 副本清理失败: {error:#}"))
                .unwrap_or_default()
        ));
    }
    *settings = next.clone();
    Ok(bootstrap_impl(&next))
}

#[tauri::command]
pub async fn environment_action(
    app: AppHandle,
    action: String,
    id: Option<String>,
) -> Result<Bootstrap, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let downloads = app.state::<crate::download_commands::DownloadManager>();
        action_impl(&state, &downloads, &action, id)
    })
    .await
    .map_err(|error| format!("运行环境任务失败: {error}"))?
}

pub fn proton_path(settings: &Settings) -> Option<&Path> {
    match selected(settings) {
        Ok(Some(entry)) => Some(entry.proton.path.as_path()),
        Ok(None) => settings
            .original_proton_pin
            .as_ref()
            .map(|pin| pin.path.as_path()),
        Err(_) => None,
    }
}

fn action_impl(
    state: &AppState,
    downloads: &crate::download_commands::DownloadManager,
    action: &str,
    id: Option<String>,
) -> Result<Bootstrap, String> {
    let _task = Task::begin(state, downloads)?;
    let mut settings = state.settings.lock().map_err(|error| error.to_string())?;
    let mut next = settings.clone();
    let environment = id
        .as_ref()
        .map(|id| {
            next.runtime_environments
                .iter()
                .find(|entry| &entry.id == id)
                .cloned()
                .ok_or_else(|| "运行环境不存在".to_string())
        })
        .transpose()?;
    match action {
        "switch" => {
            next.active_environment = id;
            // 原始环境即使尚未配置完毕也允许切回, 方便修复错误配置.
            if next.active_environment.is_some() {
                validate_selection(&next)?;
            }
        }
        "keep" => {
            if id.is_some() {
                let mut candidate = next.clone();
                candidate.active_environment = id.clone();
                validate_selection(&candidate)?;
            }
            next.stable_environment = id;
        }
        "discard" => {
            let entry = environment.ok_or("不能删除原始环境")?;
            if next.active_environment.as_ref() == Some(&entry.id)
                || next.stable_environment.as_ref() == Some(&entry.id)
            {
                return Err("不能删除当前环境或稳定环境, 请先切换并选择其他稳定环境".into());
            }
            // 设置提交失败时恢复目录, 不把删除到一半的 prefix 重新登记.
            let pending = environment::prepare_discard(&data_dir(), &entry)
                .map_err(|error| format!("{error:#}"))?;
            next.runtime_environments
                .retain(|profile| profile.id != entry.id);
            persist_settings(&next)?;
            *settings = next.clone();
            pending.commit().map_err(|error| format!("{error:#}"))?;
            return Ok(bootstrap_impl(&next));
        }
        _ => return Err("不支持的运行环境操作".into()),
    }
    persist_settings(&next)?;
    *settings = next.clone();
    Ok(bootstrap_impl(&next))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{fake_proton, EnvGuard, TempDir};
    use crate::{download_commands::DownloadManager, GameTracker};
    use std::sync::Mutex;

    struct Fixture {
        directory: TempDir,
        state: AppState,
        downloads: DownloadManager,
        _env: EnvGuard,
    }

    impl Fixture {
        fn new() -> Self {
            let mut env = EnvGuard::new();
            let directory = TempDir::new("environment-commands");
            env.set("HOME", directory.join("home"));
            env.set("XDG_CONFIG_HOME", directory.join("config"));
            env.set("XDG_DATA_HOME", directory.join("data"));
            env.remove("NTE_ROOT");
            env.remove("NTE_INSTALL");
            env.remove("WINEPREFIX");
            env.remove("PROTONPATH");
            directory.write_text(
                "install/NTELauncher/Config/Config.ini",
                "[VERSION]\nVersion=1.0\n",
            );
            directory.write("install/NTELauncher/NTEGame.exe", "MZ");
            directory.write(
                "install/Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe",
                "MZ",
            );
            directory.write("source/drive_c/users/player/config.ini", "original");
            let proton = fake_proton(directory.path(), "Proton-A");
            fake_proton(directory.path(), "Proton-B");
            let state = AppState {
                settings: Mutex::new(Settings {
                    prefix: Some(directory.join("source").display().to_string()),
                    proton_path: Some(proton.display().to_string()),
                    install_root: Some(directory.join("install").display().to_string()),
                    ..Default::default()
                }),
                game: Mutex::new(GameTracker::default()),
            };
            Self {
                directory,
                state,
                downloads: DownloadManager::default(),
                _env: env,
            }
        }

        fn create(&self) -> Result<Bootstrap, String> {
            create_impl(
                &self.state,
                &self.downloads,
                None,
                "新版测试",
                &self.directory.join("Proton-B").display().to_string(),
            )
        }

        fn action(&self, action: &str, id: Option<&str>) -> Result<Bootstrap, String> {
            action_impl(&self.state, &self.downloads, action, id.map(str::to_string))
        }
    }

    #[test]
    fn create_switch_keep_and_discard_preserve_original_environment() {
        let fixture = Fixture::new();
        let created = fixture.create().unwrap();
        assert!(created.settings.active_environment.is_none());
        assert!(created.settings.original_proton_pin.is_some());
        assert_eq!(created.settings.runtime_environments.len(), 1);
        let id = &created.settings.runtime_environments[0].id;
        assert!(fixture.create().unwrap_err().contains("名称已存在"));
        let selected = fixture.action("switch", Some(id)).unwrap();
        assert!(selected.prefix.contains(id));
        assert!(selected.proton_path.ends_with("Proton-B"));
        assert!(fixture
            .action("discard", Some(id))
            .unwrap_err()
            .contains("当前环境"));
        fixture.action("keep", Some(id)).unwrap();
        fixture.action("switch", None).unwrap();
        assert!(fixture
            .action("discard", Some(id))
            .unwrap_err()
            .contains("稳定环境"));
        fixture.action("keep", None).unwrap();
        let deleted = fixture.action("discard", Some(id)).unwrap();
        assert!(deleted.settings.runtime_environments.is_empty());
        assert_eq!(
            fixture
                .directory
                .read_text("source/drive_c/users/player/config.ini"),
            "original"
        );
        assert!(fixture.action("discard", None).is_err());
        assert!(fixture.action("switch", Some("missing")).is_err());
        assert!(fixture.action("unsupported", None).is_err());
        assert!(crate::load_settings().runtime_environments.is_empty());
        assert!(!fixture.state.game.lock().unwrap().environment_busy);
    }

    #[test]
    fn creation_uses_the_requested_source_without_switching_the_active_environment() {
        let fixture = Fixture::new();
        let created = fixture.create().unwrap();
        let b = created.settings.runtime_environments[0].clone();
        fixture.action("switch", Some(&b.id)).unwrap();
        let relative = "drive_c/users/player/config.ini";
        std::fs::write(b.prefix(&data_dir()).unwrap().join(relative), "B settings").unwrap();
        let a_copy = create_impl(
            &fixture.state,
            &fixture.downloads,
            None,
            "稳定 A 的副本",
            &fixture.directory.join("Proton-A").display().to_string(),
        )
        .unwrap();
        let a = a_copy.settings.runtime_environments.last().unwrap();
        assert_eq!(
            std::fs::read_to_string(a.prefix(&data_dir()).unwrap().join(relative)).unwrap(),
            "original"
        );
        assert_eq!(
            a_copy.settings.active_environment.as_deref(),
            Some(b.id.as_str())
        );
        assert!(a_copy.settings.stable_environment.is_none());
        let b_copy = create_impl(
            &fixture.state,
            &fixture.downloads,
            Some(&b.id),
            "B 的副本",
            &fixture.directory.join("Proton-A").display().to_string(),
        )
        .unwrap();
        let copied = b_copy.settings.runtime_environments.last().unwrap();
        assert_eq!(
            std::fs::read_to_string(copied.prefix(&data_dir()).unwrap().join(relative)).unwrap(),
            "B settings"
        );
        assert_eq!(
            crate::load_settings().active_environment.as_deref(),
            Some(b.id.as_str())
        );
        assert!(create_impl(
            &fixture.state,
            &fixture.downloads,
            Some("missing"),
            "无效源",
            &fixture.directory.join("Proton-A").display().to_string()
        )
        .unwrap_err()
        .contains("不存在"));
        assert_eq!(
            fixture
                .state
                .settings
                .lock()
                .unwrap()
                .runtime_environments
                .len(),
            3
        );
    }

    #[test]
    fn changed_proton_and_installation_cannot_be_selected() {
        let fixture = Fixture::new();
        let created = fixture.create().unwrap();
        let id = &created.settings.runtime_environments[0].id;
        fixture.directory.write_text("Proton-B/proton", "updated");
        assert!(fixture
            .action("switch", Some(id))
            .unwrap_err()
            .contains("Proton 已变化"));
        assert!(fixture
            .state
            .settings
            .lock()
            .unwrap()
            .active_environment
            .is_none());
        let mut settings = created.settings.clone();
        settings.active_environment = Some(id.clone());
        settings.distribution = nte_core::distribution::Distribution::Global;
        assert!(prefix(&settings).unwrap_err().contains("不匹配"));
        settings.active_environment = Some("missing".into());
        assert!(selected(&settings).is_err());
        assert!(verify_proton(&settings).is_err());
        assert!(detect_proton(&settings).is_none());
    }

    #[test]
    fn failed_persistence_cleans_clone_and_does_not_replace_memory() {
        let fixture = Fixture::new();
        fixture.directory.write("config", "blocked");
        assert!(fixture.create().is_err());
        let settings = fixture.state.settings.lock().unwrap();
        assert!(settings.runtime_environments.is_empty());
        assert!(settings.original_proton_pin.is_none());
        assert_eq!(
            std::fs::read_dir(data_dir().join("environments"))
                .unwrap()
                .count(),
            0
        );
        assert!(!fixture.state.game.lock().unwrap().environment_busy);
    }

    #[test]
    fn failed_selection_persistence_keeps_existing_prefix_and_selection() {
        let fixture = Fixture::new();
        let created = fixture.create().unwrap();
        let profile = &created.settings.runtime_environments[0];
        std::fs::remove_file(crate::settings_path()).unwrap();
        std::fs::create_dir(crate::settings_path()).unwrap();
        assert!(fixture.action("switch", Some(&profile.id)).is_err());
        assert!(fixture.action("discard", Some(&profile.id)).is_err());
        assert!(profile.prefix(&data_dir()).is_ok());
        assert!(fixture
            .state
            .settings
            .lock()
            .unwrap()
            .active_environment
            .is_none());
    }

    #[test]
    fn environment_tasks_block_overlaps_and_runtime_settings_changes() {
        let fixture = Fixture::new();
        {
            let task = Task::begin(&fixture.state, &fixture.downloads).unwrap();
            assert!(fixture.create().is_err());
            assert!(fixture.action("switch", None).is_err());
            drop(task);
        }
        fixture.state.game.lock().unwrap().starting = true;
        assert!(fixture.create().is_err());
        fixture.state.game.lock().unwrap().starting = false;
        for mode in ["check", "apply"] {
            {
                let mut downloads = fixture.downloads.inner.lock().unwrap();
                downloads.status.phase = "checking".into();
                downloads.status.mode = mode.into();
            }
            assert!(fixture.create().unwrap_err().contains("资源任务正在进行"));
            assert!(fixture.action("switch", None).is_err());
        }
        fixture.downloads.inner.lock().unwrap().status.phase = "idle".into();
        let original = Settings::default();
        let mut next = original.clone();
        next.prefix = Some("/other".into());
        assert!(validate_settings_change(&original, &next, true).is_err());
        assert!(validate_settings_change(&original, &next, false).is_ok());
        next = original.clone();
        next.theme = "dark".into();
        assert!(validate_settings_change(&original, &next, true).is_ok());
        next.active_environment = Some("foreign".into());
        assert!(validate_settings_change(&original, &next, false).is_err());
    }

    #[test]
    fn external_prefix_process_blocks_copy_until_it_exits() {
        let fixture = Fixture::new();
        let mut child = std::process::Command::new("/usr/bin/sleep")
            .arg("5")
            .env("WINEPREFIX", fixture.directory.join("source"))
            .spawn()
            .unwrap();
        let result = fixture.create();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(result.unwrap_err().contains("仍被进程"));
        fixture.create().unwrap();
    }
}
