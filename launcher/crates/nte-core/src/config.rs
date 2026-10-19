use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::Result;

/// Minimal INI reader tolerant of UTF-8 BOM, CRLF, comments and duplicate keys.
#[derive(Debug, Default, Clone)]
pub struct Ini {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl Ini {
    pub fn parse(text: &str) -> Self {
        let mut ini = Ini::default();
        let mut section = String::from("General");
        ini.sections.entry(section.clone()).or_default();
        for raw in text.trim_start_matches('\u{feff}').lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = rest.trim().to_string();
                ini.sections.entry(section.clone()).or_default();
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                ini.sections
                    .entry(section.clone())
                    .or_default()
                    .insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        ini
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(Self::parse(&text))
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(section)?.get(key).map(String::as_str)
    }

    pub fn get_int(&self, section: &str, key: &str) -> Option<i64> {
        self.get(section, key)?.parse().ok()
    }

    pub fn section(&self, section: &str) -> Option<&BTreeMap<String, String>> {
        self.sections.get(section)
    }
}

/// The subset of the shipped Config/Config.ini that matters for a native launcher.
#[derive(Debug, Clone)]
pub struct LauncherConfig {
    pub root: PathBuf,
    pub launcher: String,
    pub patcher: String,
    pub client: String,
    pub game_id: i64,
    pub launch_type: i64,
    pub version: String,
    pub build: String,
    pub version_url: String,
    pub backup_version_url: String,
    pub branch: String,
    pub patcher_ignored_sections: Vec<String>,
    pub mutex_name: String,
}

impl LauncherConfig {
    pub fn load(root: &Path) -> Result<Self> {
        let ini = Ini::load(&root.join("Config").join("Config.ini"))?;
        let version = ini
            .section("VERSION")
            .cloned()
            .unwrap_or_default();
        let ignored = version
            .get("PATCHER_IGNORED_SECTION")
            .cloned()
            .unwrap_or_default();
        Ok(Self {
            root: root.to_path_buf(),
            launcher: ini.get("General", "Launcher").unwrap_or("NTELauncher.exe").to_string(),
            patcher: ini.get("General", "Patcher").unwrap_or("NTEUpdate.exe").to_string(),
            client: ini.get("General", "Client").unwrap_or("NTEGame.exe").to_string(),
            game_id: ini.get_int("General", "GameID").unwrap_or(0),
            launch_type: ini.get_int("General", "LaunchType").unwrap_or(0),
            version: version.get("Version").cloned().unwrap_or_default(),
            build: version.get("Build").cloned().unwrap_or_default(),
            version_url: version
                .get("VersionInfoFileURL")
                .cloned()
                .unwrap_or_else(|| crate::VERSION_URL.to_string()),
            backup_version_url: version
                .get("BackupVersionURL")
                .cloned()
                .unwrap_or_else(|| crate::BACKUP_VERSION_URL.to_string()),
            branch: version
                .get("Branch")
                .cloned()
                .unwrap_or_else(|| "publish_ob".to_string()),
            patcher_ignored_sections: ignored
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            mutex_name: version
                .get("MutexName")
                .cloned()
                .unwrap_or_default(),
        })
    }

    /// Path layout of the installed launcher.
    pub fn launcher_dir(&self) -> &Path { &self.root }
    pub fn user_config(&self) -> PathBuf { self.root.join("UserData").join("Config").join("Config.ini") }
    pub fn game_dir(&self) -> PathBuf { self.root.join("..").join("Client").join("WindowsNoEditor") }
}
