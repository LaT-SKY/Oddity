//! 启动器设置及用户目录, 供主程序和应急入口共用.
use crate::distribution::Distribution;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub distribution: Distribution,
    pub install_root: Option<String>,
    pub global_install_root: Option<String>,
    pub runtime_environments: Vec<crate::environment::RuntimeEnvironment>,
    pub active_environment: Option<String>,
    pub stable_environment: Option<String>,
    pub launch_mode: String,
    pub game_mode: crate::game_mode::Options,
    pub gamepad: crate::gamepad::Options,
    pub language: String,
    pub reduce_motion: bool,
    pub stack_bulletins: bool,
    pub theme: String,
    pub hero_media: String,
    pub minimize_to_tray_on_close: bool,
    pub show_launcher_after_game_exit: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            distribution: Distribution::Cn,
            install_root: None,
            global_install_root: None,
            runtime_environments: Vec::new(),
            active_environment: None,
            stable_environment: None,
            launch_mode: "background".to_string(),
            game_mode: crate::game_mode::Options::default(),
            gamepad: crate::gamepad::Options::default(),
            language: "zh-CN".to_string(),
            reduce_motion: false,
            stack_bulletins: false,
            theme: "light".to_string(),
            hero_media: "video".to_string(),
            minimize_to_tray_on_close: true,
            show_launcher_after_game_exit: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserPaths {
    pub config_base: PathBuf,
    pub data_base: PathBuf,
}

impl UserPaths {
    pub fn discover() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        Self {
            config_base: std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config")),
            data_base: std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share")),
        }
    }
    pub fn config_dir(&self) -> PathBuf {
        self.config_base.join("oddity")
    }
    pub fn data_dir(&self) -> PathBuf {
        self.data_base.join("oddity")
    }
    pub fn settings(&self) -> PathBuf {
        self.config_dir().join("settings.json")
    }
    pub fn logs(&self) -> PathBuf {
        self.data_dir().join("logs")
    }
    pub fn instance_lock(&self) -> PathBuf {
        self.config_base.join("oddity-instance.lock")
    }
}
