//! 运行环境操作与启动/资源任务共用互斥边界.
use std::path::{Path, PathBuf};

use nte_core::environment::{self, RuntimeEnvironment};
use tauri::{AppHandle, Manager};

use crate::{
    bootstrap_impl, data_dir, detect_launcher_root, detect_proton, effective_install_root,
    expand_path, persist_settings, session_alive, AppState, Bootstrap, Settings,
};

pub fn selected(settings: &Settings) -> Result<&RuntimeEnvironment, String> {
    let id = settings
        .active_environment
        .as_ref()
        .ok_or("请先在设置中创建并选择 Proton 运行环境")?;
    settings
        .runtime_environments
        .iter()
        .find(|environment| &environment.id == id)
        .ok_or_else(|| "所选运行环境不存在, 请重新选择环境".into())
}

pub fn prefix(settings: &Settings) -> Result<PathBuf, String> {
    let environment = selected(settings)?;
    let launcher = detect_launcher_root(settings);
    let install = effective_install_root(settings, launcher.as_deref());
    if !environment.matches(&install, settings.distribution) {
        return Err("运行环境与当前游戏地区或安装目录不匹配, 请切换环境".into());
    }
    environment
        .prefix(&data_dir())
        .map_err(|error| format!("{error:#}"))
}

pub fn verify_proton(settings: &Settings) -> Result<(), String> {
    selected(settings)?
        .proton
        .verify()
        .map_err(|error| format!("{error:#}"))
}

pub fn log_dir(settings: &Settings) -> Result<PathBuf, String> {
    let environment = selected(settings)?;
    environment
        .directory(&data_dir())
        .map_err(|error| error.to_string())?;
    Ok(data_dir().join("logs/environments").join(&environment.id))
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
    {
        return Err("运行环境已变化, 请重新检测后重试".into());
    }
    if busy
        && (crate::download_paths_changed(current, next) || current.launch_mode != next.launch_mode)
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
    if snapshot
        .runtime_environments
        .iter()
        .any(|entry| entry.name == name.trim())
    {
        return Err("运行环境名称已存在".into());
    }
    let source = if let Some(id) = source_id {
        snapshot.active_environment = Some(id.into());
        validate_selection(&snapshot)?;
        Some((prefix(&snapshot)?, selected(&snapshot)?.proton.path.clone()))
    } else {
        None
    };
    let launcher = detect_launcher_root(&snapshot);
    let install = effective_install_root(&snapshot, launcher.as_deref());
    let _install_lock = nte_core::install_lock::InstallLock::acquire(&install)
        .map_err(|error| error.to_string())?;
    let created = environment::create(
        &data_dir(),
        source
            .as_ref()
            .map(|(prefix, proton)| (prefix.as_path(), proton.as_path())),
        &expand_path(proton_path),
        &install,
        snapshot.distribution,
        name,
    )
    .map_err(|error| format!("创建运行环境失败: {error:#}"))?;
    let mut settings = state.settings.lock().map_err(|error| error.to_string())?;
    let mut next = settings.clone();
    // 首个环境创建后即可使用, 后续副本不改变当前选择.
    if next.active_environment.is_none() {
        next.active_environment = Some(created.id.clone());
        next.stable_environment = Some(created.id.clone());
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
    selected(settings)
        .ok()
        .map(|entry| entry.proton.path.as_path())
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
    let id = id.ok_or("请选择运行环境")?;
    let environment = next
        .runtime_environments
        .iter()
        .find(|entry| entry.id == id)
        .cloned()
        .ok_or("运行环境不存在")?;
    match action {
        "switch" => {
            next.active_environment = Some(id);
            validate_selection(&next)?;
        }
        "keep" => {
            let mut candidate = next.clone();
            candidate.active_environment = Some(id.clone());
            validate_selection(&candidate)?;
            next.stable_environment = Some(id);
        }
        "discard" => {
            let entry = environment;
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
            directory.write_text(
                "install/NTELauncher/Config/Config.ini",
                "[VERSION]\nVersion=1.0\n",
            );
            directory.write("install/NTELauncher/NTEGame.exe", "MZ");
            directory.write(
                "install/Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe",
                "MZ",
            );
            fake_proton(directory.path(), "Proton-A");
            fake_proton(directory.path(), "Proton-B");
            let state = AppState {
                settings: Mutex::new(Settings {
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

        fn create(
            &self,
            name: &str,
            source: Option<&str>,
            proton: &str,
        ) -> Result<Bootstrap, String> {
            create_impl(
                &self.state,
                &self.downloads,
                source,
                name,
                &self.directory.join(proton).display().to_string(),
            )
        }

        fn first(&self) -> RuntimeEnvironment {
            self.create("A", None, "Proton-A")
                .unwrap()
                .settings
                .runtime_environments[0]
                .clone()
        }

        fn action(&self, action: &str, id: Option<&str>) -> Result<Bootstrap, String> {
            action_impl(&self.state, &self.downloads, action, id.map(str::to_string))
        }
    }

    #[test]
    fn first_environment_is_required_and_automatically_selected_and_kept() {
        let fixture = Fixture::new();
        let original = fixture.state.settings.lock().unwrap().clone();
        assert!(prefix(&original).unwrap_err().contains("请先"));
        assert!(verify_proton(&original).is_err());
        assert!(log_dir(&original).is_err());
        assert!(detect_proton(&original).is_none());
        let a = fixture.first();
        let settings = crate::load_settings();
        assert_eq!(settings.active_environment.as_deref(), Some(a.id.as_str()));
        assert_eq!(settings.stable_environment, settings.active_environment);
        assert!(prefix(&settings)
            .unwrap()
            .ends_with(format!("{}/prefix", a.id)));
        assert!(log_dir(&settings).unwrap().ends_with(&a.id));
        assert!(fixture.action("switch", None).is_err());
        assert!(fixture.action("keep", None).is_err());
        assert!(fixture
            .action("discard", Some(&a.id))
            .unwrap_err()
            .contains("当前环境"));
        assert!(fixture
            .create("A", None, "Proton-B")
            .unwrap_err()
            .contains("名称已存在"));
        assert!(fixture.action("switch", Some("missing")).is_err());
        assert!(fixture.action("unsupported", Some(&a.id)).is_err());
    }

    #[test]
    fn selected_source_can_be_cloned_while_another_environment_is_active() {
        let fixture = Fixture::new();
        let a = fixture.first();
        let relative = "drive_c/settings.ini";
        std::fs::write(a.prefix(&data_dir()).unwrap().join(relative), "A settings").unwrap();
        let created = fixture.create("B", Some(&a.id), "Proton-B").unwrap();
        let b = created
            .settings
            .runtime_environments
            .last()
            .unwrap()
            .clone();
        assert_eq!(
            created.settings.active_environment.as_deref(),
            Some(a.id.as_str())
        );
        fixture.action("switch", Some(&b.id)).unwrap();
        std::fs::write(b.prefix(&data_dir()).unwrap().join(relative), "B settings").unwrap();
        let copied = fixture.create("A 副本", Some(&a.id), "Proton-B").unwrap();
        let copy = copied.settings.runtime_environments.last().unwrap();
        assert_eq!(
            std::fs::read_to_string(copy.prefix(&data_dir()).unwrap().join(relative)).unwrap(),
            "A settings"
        );
        assert_eq!(
            copied.settings.active_environment.as_deref(),
            Some(b.id.as_str())
        );
        assert_eq!(
            copied.settings.stable_environment.as_deref(),
            Some(a.id.as_str())
        );
        assert!(fixture
            .create("无效源", Some("missing"), "Proton-A")
            .unwrap_err()
            .contains("不存在"));
        fixture.action("keep", Some(&b.id)).unwrap();
        let deleted = fixture.action("discard", Some(&a.id)).unwrap();
        assert_eq!(deleted.settings.runtime_environments.len(), 2);
        assert!(a.prefix(&data_dir()).is_err());
        assert_eq!(
            std::fs::read_to_string(b.prefix(&data_dir()).unwrap().join(relative)).unwrap(),
            "B settings"
        );
        fixture.action("switch", Some(&copy.id)).unwrap();
        assert!(fixture
            .action("discard", Some(&b.id))
            .unwrap_err()
            .contains("稳定环境"));
    }

    #[test]
    fn changed_proton_and_installation_cannot_be_selected() {
        let fixture = Fixture::new();
        let a = fixture.first();
        let created = fixture.create("B", Some(&a.id), "Proton-B").unwrap();
        let b = created.settings.runtime_environments.last().unwrap();
        fixture.directory.write_text("Proton-B/proton", "updated");
        assert!(fixture
            .action("switch", Some(&b.id))
            .unwrap_err()
            .contains("Proton 已变化"));
        assert!(fixture.action("keep", Some(&b.id)).is_err());
        assert_eq!(
            crate::load_settings().active_environment.as_deref(),
            Some(a.id.as_str())
        );
        let mut settings = created.settings.clone();
        settings.distribution = nte_core::distribution::Distribution::Global;
        assert!(prefix(&settings).unwrap_err().contains("不匹配"));
        settings.active_environment = Some("missing".into());
        assert!(selected(&settings).is_err());
        assert!(verify_proton(&settings).is_err());
        assert!(detect_proton(&settings).is_none());
    }

    #[test]
    fn fresh_environment_can_be_created_for_a_new_installation() {
        let fixture = Fixture::new();
        fixture.state.settings.lock().unwrap().install_root =
            Some(fixture.directory.join("new-install").display().to_string());
        let created = fixture.create("新安装", None, "Proton-A").unwrap();
        let profile = &created.settings.runtime_environments[0];
        assert_eq!(profile.install_root, fixture.directory.join("new-install"));
        assert!(profile
            .prefix(&data_dir())
            .unwrap()
            .join("drive_c")
            .is_dir());
        assert!(created.error.unwrap().contains("尚未安装启动器"));
    }

    #[test]
    fn failed_persistence_cleans_environment_and_does_not_replace_memory() {
        let fixture = Fixture::new();
        fixture.directory.write("config", "blocked");
        assert!(fixture.create("A", None, "Proton-A").is_err());
        let settings = fixture.state.settings.lock().unwrap();
        assert!(settings.runtime_environments.is_empty());
        assert!(settings.active_environment.is_none());
        assert_eq!(
            std::fs::read_dir(data_dir().join("environments"))
                .unwrap()
                .count(),
            0
        );
        assert!(!fixture.state.game.lock().unwrap().environment_busy);
    }

    #[test]
    fn failed_persistence_keeps_selection_and_restores_deleted_directory() {
        let fixture = Fixture::new();
        let a = fixture.first();
        let created = fixture.create("B", Some(&a.id), "Proton-B").unwrap();
        let b = created.settings.runtime_environments.last().unwrap();
        std::fs::remove_file(crate::settings_path()).unwrap();
        std::fs::create_dir(crate::settings_path()).unwrap();
        assert!(fixture.action("switch", Some(&b.id)).is_err());
        assert!(fixture.action("discard", Some(&b.id)).is_err());
        assert!(b.prefix(&data_dir()).is_ok());
        assert_eq!(
            fixture
                .state
                .settings
                .lock()
                .unwrap()
                .active_environment
                .as_deref(),
            Some(a.id.as_str())
        );
    }

    #[test]
    fn environment_tasks_block_overlaps_and_runtime_settings_changes() {
        let fixture = Fixture::new();
        {
            let task = Task::begin(&fixture.state, &fixture.downloads).unwrap();
            assert!(fixture.create("A", None, "Proton-A").is_err());
            assert!(fixture.action("switch", None).is_err());
            drop(task);
        }
        fixture.state.game.lock().unwrap().starting = true;
        assert!(fixture.create("A", None, "Proton-A").is_err());
        fixture.state.game.lock().unwrap().starting = false;
        for mode in ["check", "apply"] {
            {
                let mut downloads = fixture.downloads.inner.lock().unwrap();
                downloads.status.phase = "checking".into();
                downloads.status.mode = mode.into();
            }
            assert!(fixture
                .create("A", None, "Proton-A")
                .unwrap_err()
                .contains("资源任务正在进行"));
            assert!(fixture.action("switch", None).is_err());
        }
        let original = Settings::default();
        let mut next = original.clone();
        next.install_root = Some("/other".into());
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
        use std::io::BufRead;
        use std::process::{Command, Stdio};
        let fixture = Fixture::new();
        let a = fixture.first();
        let mut child = Command::new("/bin/sh")
            .args(["-c", "echo ready; read -r release"])
            .env("WINEPREFIX", a.prefix(&data_dir()).unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        // 子进程确认 exec 完成后才扫描 /proc, 避免读到继承自测试进程的旧环境变量.
        let mut ready = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        let result = fixture.create("B", Some(&a.id), "Proton-B");
        drop(child.stdin.take());
        child.wait().unwrap();
        assert_eq!(ready.trim(), "ready");
        assert!(result.unwrap_err().contains("仍被进程"));
        fixture.create("B", Some(&a.id), "Proton-B").unwrap();
    }
}
