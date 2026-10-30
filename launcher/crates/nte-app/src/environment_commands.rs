//! 运行环境操作与启动/资源任务共用互斥边界.
use std::path::{Path, PathBuf};

use nte_core::distribution::Distribution;
use nte_core::environment::{self, RuntimeEnvironment};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::{
    bootstrap_impl, data_dir, detect_launcher_root, detect_proton, effective_install_root,
    expand_path, persist_settings, session_alive, AppState, Bootstrap, Settings,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub distribution: Distribution,
    pub install_root: PathBuf,
    pub installed: bool,
}

fn installation(settings: &Settings, distribution: Distribution) -> Installation {
    let mut target = settings.clone();
    target.distribution = distribution;
    let launcher = detect_launcher_root(&target);
    Installation {
        distribution,
        install_root: effective_install_root(&target, launcher.as_deref()),
        installed: launcher
            .as_deref()
            .and_then(crate::game_exe_for)
            .is_some_and(|game| game.is_file()),
    }
}

pub fn installations(settings: &Settings) -> Vec<Installation> {
    [Distribution::Cn, Distribution::Global]
        .into_iter()
        .map(|distribution| installation(settings, distribution))
        .collect()
}

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
    distribution: Distribution,
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
            distribution,
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
    distribution: Distribution,
) -> Result<Bootstrap, String> {
    let _task = Task::begin(state, downloads)?;
    let snapshot = state
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
    let target = installation(&snapshot, distribution);
    if !target.installed {
        return Err(format!(
            "尚未安装{}, 请先安装游戏",
            if distribution == Distribution::Cn {
                "国服"
            } else {
                "国际服"
            }
        ));
    }
    let source = if let Some(id) = source_id {
        let source = snapshot
            .runtime_environments
            .iter()
            .find(|entry| entry.id == id)
            .ok_or("源运行环境不存在")?;
        source
            .proton
            .verify()
            .map_err(|error| format!("{error:#}"))?;
        Some((
            source
                .prefix(&data_dir())
                .map_err(|error| format!("{error:#}"))?,
            source.proton.path.clone(),
        ))
    } else {
        None
    };
    let install = target.install_root;
    let _install_lock = nte_core::install_lock::InstallLock::acquire(&install)
        .map_err(|error| error.to_string())?;
    let created = environment::create(
        &data_dir(),
        source
            .as_ref()
            .map(|(prefix, proton)| (prefix.as_path(), proton.as_path())),
        &expand_path(proton_path),
        &install,
        distribution,
        name,
    )
    .map_err(|error| format!("创建运行环境失败: {error:#}"))?;
    let mut settings = state.settings.lock().map_err(|error| error.to_string())?;
    let mut next = settings.clone();
    // 首个环境创建后即可使用, 后续副本不改变当前选择.
    if next.active_environment.is_none() {
        next.distribution = distribution;
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
            next.distribution = environment.distribution;
            next.active_environment = Some(id);
            validate_selection(&next)?;
        }
        "keep" => {
            let mut candidate = next.clone();
            candidate.distribution = environment.distribution;
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

/// 外部启动时选择并持久化地区与运行环境.
#[derive(Debug, Clone)]
pub struct LaunchSelection {
    pub settings: Settings,
    pub changed: bool,
}

fn resolve_launch_target(
    settings: &Settings,
    region: Option<Distribution>,
    environment: Option<&str>,
) -> Result<(Distribution, String), String> {
    if let Some(value) = environment {
        let found = if let Some(entry) = settings
            .runtime_environments
            .iter()
            .find(|entry| entry.id == value)
        {
            entry
        } else {
            let mut matches = settings
                .runtime_environments
                .iter()
                .filter(|entry| entry.name == value);
            let first = matches
                .next()
                .ok_or_else(|| format!("运行环境不存在: {value}"))?;
            if matches.next().is_some() {
                return Err(format!("运行环境名称重复: {value}. 请改用环境 ID."));
            }
            first
        };
        if let Some(region) = region {
            if found.distribution != region {
                return Err(format!(
                    "运行环境「{}」属于{}, 与 --region {} 不一致.",
                    found.name,
                    found.distribution.label(),
                    region.as_str()
                ));
            }
        }
        let mut target = settings.clone();
        target.distribution = found.distribution;
        let launcher = crate::detect_launcher_root(&target).ok_or_else(|| {
            format!(
                "尚未找到{}安装, 请先在 Oddity 中完成安装.",
                found.distribution.label()
            )
        })?;
        let install = crate::effective_install_root(&target, Some(&launcher));
        if !found.matches(&install, found.distribution) {
            return Err(format!(
                "运行环境「{}」与当前游戏安装目录不匹配, 请重新选择环境.",
                found.name
            ));
        }
        return Ok((found.distribution, found.id.clone()));
    }

    let region = region.ok_or("缺少游戏地区")?;
    let mut target = settings.clone();
    target.distribution = region;
    let launcher = crate::detect_launcher_root(&target)
        .ok_or_else(|| format!("尚未找到{}安装, 请先在 Oddity 中完成安装.", region.label()))?;
    let install = crate::effective_install_root(&target, Some(&launcher));
    let candidates: Vec<&RuntimeEnvironment> = settings
        .runtime_environments
        .iter()
        .filter(|entry| entry.matches(&install, region))
        .collect();
    if candidates.is_empty() {
        return Err(format!(
            "尚未为{}创建运行环境, 请先在 Oddity 中创建.",
            region.label()
        ));
    }
    let active = settings.active_environment.as_deref();
    let stable = settings.stable_environment.as_deref();
    let chosen = candidates
        .iter()
        .find(|entry| active == Some(entry.id.as_str()))
        .or_else(|| {
            candidates
                .iter()
                .find(|entry| stable == Some(entry.id.as_str()))
        })
        .copied()
        .or_else(|| (candidates.len() == 1).then(|| candidates[0]));
    if let Some(chosen) = chosen {
        return Ok((region, chosen.id.clone()));
    }
    let names = candidates
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "{}有多个运行环境 ({}), 请使用 --environment 指定.",
        region.label(),
        names
    ))
}

/// 解析请求地区与环境, 必要时切换并持久化. 没有指定时沿用当前设置.
pub fn select_for_launch(
    state: &AppState,
    downloads: &crate::download_commands::DownloadManager,
    region: Option<Distribution>,
    environment: Option<&str>,
) -> Result<LaunchSelection, String> {
    if region.is_none() && environment.is_none() {
        let settings = state
            .settings
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        return Ok(LaunchSelection {
            settings,
            changed: false,
        });
    }

    let current = state
        .settings
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let (distribution, environment_id) = resolve_launch_target(&current, region, environment)?;
    if current.distribution == distribution
        && current.active_environment.as_deref() == Some(environment_id.as_str())
    {
        return Ok(LaunchSelection {
            settings: current,
            changed: false,
        });
    }

    let task = Task::begin(state, downloads)?;
    let mut settings = state.settings.lock().map_err(|error| error.to_string())?;
    let snapshot = settings.clone();
    let (distribution, environment_id) = resolve_launch_target(&snapshot, region, environment)?;
    if snapshot.distribution == distribution
        && snapshot.active_environment.as_deref() == Some(environment_id.as_str())
    {
        drop(settings);
        drop(task);
        return Ok(LaunchSelection {
            settings: snapshot,
            changed: false,
        });
    }

    let mut next = snapshot;
    next.distribution = distribution;
    next.active_environment = Some(environment_id);
    validate_selection(&next)?;
    persist_settings(&next)?;
    *settings = next.clone();
    drop(settings);
    drop(task);
    Ok(LaunchSelection {
        settings: next,
        changed: true,
    })
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
                    global_install_root: Some(directory.join("global").display().to_string()),
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
                Distribution::Cn,
            )
        }

        fn install_global(&self) {
            self.directory.write_text(
                "global/NTEGlobal/Config/Config.ini",
                "[General]\nGameID=3000001\n[VERSION]\nVersion=1.0\n",
            );
            self.directory
                .write("global/NTEGlobal/NTEGlobalGame.exe", "MZ");
            self.directory.write(
                "global/Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe",
                "MZ",
            );
        }

        fn create_global(&self, name: &str, source: Option<&str>) -> Result<Bootstrap, String> {
            create_impl(
                &self.state,
                &self.downloads,
                source,
                name,
                &self.directory.join("Proton-B").display().to_string(),
                Distribution::Global,
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
    fn missing_game_cannot_be_used_to_create_an_environment() {
        let fixture = Fixture::new();
        let availability = installations(&fixture.state.settings.lock().unwrap());
        assert!(availability[0].installed);
        assert!(!availability[1].installed);
        assert!(fixture
            .create_global("国际服", None)
            .unwrap_err()
            .contains("尚未安装国际服"));
        std::fs::remove_file(
            fixture
                .directory
                .join("install/Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe"),
        )
        .unwrap();
        assert!(fixture
            .create("国服", None, "Proton-A")
            .unwrap_err()
            .contains("尚未安装国服"));
        assert!(!installations(&fixture.state.settings.lock().unwrap())[0].installed);
        assert!(fixture
            .state
            .settings
            .lock()
            .unwrap()
            .runtime_environments
            .is_empty());
        assert!(!data_dir().join("environments").exists());
    }

    #[test]
    fn copies_bind_the_selected_region_and_switch_with_its_installation() {
        let fixture = Fixture::new();
        fixture.install_global();
        let a = fixture.first();
        let source = a.prefix(&data_dir()).unwrap();
        std::fs::write(source.join("user.reg"), "CN settings").unwrap();
        let relative = "drive_c/Program Files (x86)/Neverness To Everness";
        std::fs::create_dir_all(source.join(relative).parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&a.install_root, source.join(relative)).unwrap();
        let created = fixture.create_global("国际服副本", Some(&a.id)).unwrap();
        let global = created.settings.runtime_environments.last().unwrap();
        assert_eq!(global.distribution, Distribution::Global);
        assert_eq!(global.install_root, fixture.directory.join("global"));
        let copied = global.prefix(&data_dir()).unwrap();
        assert_eq!(
            std::fs::read_to_string(copied.join("user.reg")).unwrap(),
            "CN settings"
        );
        assert_eq!(
            std::fs::read_link(copied.join(relative)).unwrap(),
            global.install_root
        );
        assert_eq!(created.settings.distribution, Distribution::Cn);
        assert_eq!(
            created.settings.active_environment.as_deref(),
            Some(a.id.as_str())
        );
        let kept = fixture.action("keep", Some(&global.id)).unwrap();
        assert_eq!(kept.settings.distribution, Distribution::Cn);
        let switched = fixture.action("switch", Some(&global.id)).unwrap();
        assert_eq!(switched.settings.distribution, Distribution::Global);
        assert_eq!(
            switched.install_root,
            global.install_root.display().to_string()
        );
        assert!(switched.game_exists);
        assert!(switched
            .runtime_installations
            .iter()
            .all(|entry| entry.installed));
        let back = fixture
            .create("国服副本", Some(&global.id), "Proton-A")
            .unwrap();
        let cn = back.settings.runtime_environments.last().unwrap();
        assert_eq!(cn.distribution, Distribution::Cn);
        assert_eq!(cn.install_root, a.install_root);
        assert_eq!(back.settings.distribution, Distribution::Global);
        let switched = fixture.action("switch", Some(&cn.id)).unwrap();
        assert_eq!(switched.settings.distribution, Distribution::Cn);
    }

    #[test]
    fn first_environment_activates_the_selected_installed_region() {
        let fixture = Fixture::new();
        fixture.install_global();
        let created = fixture.create_global("国际服", None).unwrap();
        let global = &created.settings.runtime_environments[0];
        assert_eq!(created.settings.distribution, Distribution::Global);
        assert_eq!(
            created.settings.active_environment.as_deref(),
            Some(global.id.as_str())
        );
        assert_eq!(
            created.settings.active_environment,
            created.settings.stable_environment
        );
        assert!(created.game_exists);
        assert!(created.ok);
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

    #[test]
    fn launch_selection_switches_region_and_environment() {
        let fixture = Fixture::new();
        fixture.install_global();
        let a = fixture.first();
        let created = fixture.create_global("国际服", None).unwrap();
        let global = created
            .settings
            .runtime_environments
            .last()
            .unwrap()
            .clone();
        assert_eq!(created.settings.distribution, Distribution::Cn);

        let selection = select_for_launch(
            &fixture.state,
            &fixture.downloads,
            Some(Distribution::Global),
            None,
        )
        .unwrap();
        assert!(selection.changed);
        assert_eq!(selection.settings.distribution, Distribution::Global);
        assert_eq!(
            selection.settings.active_environment.as_deref(),
            Some(global.id.as_str())
        );
        assert_eq!(crate::load_settings().distribution, Distribution::Global);
        assert_eq!(
            crate::load_settings().active_environment.as_deref(),
            Some(global.id.as_str())
        );

        let selection = select_for_launch(
            &fixture.state,
            &fixture.downloads,
            Some(Distribution::Cn),
            Some(&a.id),
        )
        .unwrap();
        assert!(selection.changed);
        assert_eq!(selection.settings.distribution, Distribution::Cn);
        assert_eq!(
            selection.settings.active_environment.as_deref(),
            Some(a.id.as_str())
        );

        let selection =
            select_for_launch(&fixture.state, &fixture.downloads, None, Some("A")).unwrap();
        assert!(!selection.changed);
        assert_eq!(
            selection.settings.active_environment.as_deref(),
            Some(a.id.as_str())
        );
    }

    #[test]
    fn launch_selection_validates_region_environment_and_install() {
        let fixture = Fixture::new();
        let a = fixture.first();
        let error = select_for_launch(
            &fixture.state,
            &fixture.downloads,
            Some(Distribution::Global),
            None,
        )
        .unwrap_err();
        assert!(error.contains("尚未找到国际服安装"));
        assert!(
            select_for_launch(&fixture.state, &fixture.downloads, None, Some("missing"),).is_err()
        );
        let error = select_for_launch(
            &fixture.state,
            &fixture.downloads,
            Some(Distribution::Global),
            Some("A"),
        )
        .unwrap_err();
        assert!(error.contains("不一致"));
        let selection = select_for_launch(
            &fixture.state,
            &fixture.downloads,
            Some(Distribution::Cn),
            None,
        )
        .unwrap();
        assert!(!selection.changed);
        assert_eq!(
            selection.settings.active_environment.as_deref(),
            Some(a.id.as_str())
        );
    }

    #[test]
    fn launch_selection_requires_environment_when_region_is_ambiguous() {
        let fixture = Fixture::new();
        fixture.install_global();
        let a = fixture.first();
        let b = fixture
            .create("B", Some(&a.id), "Proton-B")
            .unwrap()
            .settings
            .runtime_environments
            .last()
            .unwrap()
            .clone();
        let global = fixture
            .create_global("国际服", None)
            .unwrap()
            .settings
            .runtime_environments
            .last()
            .unwrap()
            .clone();
        fixture.action("switch", Some(&global.id)).unwrap();
        fixture.action("keep", Some(&global.id)).unwrap();
        let error = select_for_launch(
            &fixture.state,
            &fixture.downloads,
            Some(Distribution::Cn),
            None,
        )
        .unwrap_err();
        assert!(error.contains("多个运行环境"));
        assert!(error.contains('A') && error.contains('B'));
        let selection = select_for_launch(
            &fixture.state,
            &fixture.downloads,
            Some(Distribution::Cn),
            Some("B"),
        )
        .unwrap();
        assert_eq!(
            selection.settings.active_environment.as_deref(),
            Some(b.id.as_str())
        );
    }

    #[test]
    fn launch_selection_keeps_current_settings_without_arguments() {
        let fixture = Fixture::new();
        fixture.first();
        let before = crate::load_settings();
        let selection = select_for_launch(&fixture.state, &fixture.downloads, None, None).unwrap();
        assert!(!selection.changed);
        assert_eq!(selection.settings.distribution, before.distribution);
        assert_eq!(
            selection.settings.active_environment,
            before.active_environment
        );
    }

    #[test]
    fn launch_selection_honors_busy_state() {
        let fixture = Fixture::new();
        fixture.install_global();
        let _a = fixture.first();
        fixture.create_global("国际服", None).unwrap();
        fixture.state.game.lock().unwrap().starting = true;
        let error = select_for_launch(
            &fixture.state,
            &fixture.downloads,
            Some(Distribution::Global),
            None,
        )
        .unwrap_err();
        assert!(error.contains("正在进行"));
        assert_eq!(
            fixture.state.settings.lock().unwrap().distribution,
            Distribution::Cn
        );
    }
}
