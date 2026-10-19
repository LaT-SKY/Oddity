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
        let version = ini.section("VERSION").cloned().unwrap_or_default();
        let ignored = version
            .get("PATCHER_IGNORED_SECTION")
            .cloned()
            .unwrap_or_default();
        Ok(Self {
            root: root.to_path_buf(),
            launcher: ini
                .get("General", "Launcher")
                .unwrap_or("NTELauncher.exe")
                .to_string(),
            patcher: ini
                .get("General", "Patcher")
                .unwrap_or("NTEUpdate.exe")
                .to_string(),
            client: ini
                .get("General", "Client")
                .unwrap_or("NTEGame.exe")
                .to_string(),
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
            mutex_name: version.get("MutexName").cloned().unwrap_or_default(),
        })
    }

    /// Path layout of the installed launcher.
    pub fn launcher_dir(&self) -> &Path {
        &self.root
    }
    pub fn user_config(&self) -> PathBuf {
        self.root.join("UserData").join("Config").join("Config.ini")
    }
    pub fn game_dir(&self) -> PathBuf {
        self.root.join("..").join("Client").join("WindowsNoEditor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn ini_handles_bom_comments_duplicates_and_bad_lines() {
        let text = "\u{feff}top=1\r\n; 注释\r\n# 另一种注释\r\n[Main]\r\nKey = Value \r\nKey=Second\r\nempty=\r\nno_separator\r\n[Other]\r\nn=42\r\nnotnum=abc\r\n";
        let ini = Ini::parse(text);
        assert_eq!(ini.get("General", "top"), Some("1"));
        // 重复键取最后一次出现的值
        assert_eq!(ini.get("Main", "Key"), Some("Second"));
        assert_eq!(ini.get("Main", "empty"), Some(""));
        assert_eq!(ini.get("Main", "no_separator"), None);
        assert_eq!(ini.get_int("Other", "n"), Some(42));
        assert_eq!(ini.get_int("Other", "notnum"), None);
        assert_eq!(ini.get_int("Other", "missing"), None);
        assert_eq!(ini.get("Missing", "n"), None);
        assert_eq!(ini.section("Other").map(|section| section.len()), Some(2));
        assert!(ini.section("Nope").is_none());
    }

    #[test]
    fn ini_load_reads_files_and_reports_missing_ones() {
        let dir = TempDir::new("ini");
        let path = dir.write("Config/Config.ini", "[VERSION]\nVersion=1.2.3\n");
        let ini = Ini::load(&path).expect("load ini");
        assert_eq!(ini.get("VERSION", "Version"), Some("1.2.3"));
        assert!(Ini::load(&dir.join("nope.ini")).is_err());
    }

    #[test]
    fn launcher_config_reads_the_ini_values() {
        let dir = TempDir::new("cfg");
        dir.write(
            "Config/Config.ini",
            r#"[General]
Launcher=MyLauncher.exe
Patcher=MyPatcher.exe
Client=MyGame.exe
GameID=1289
LaunchType=3

[VERSION]
Version=1.4.16
Build=20240101
VersionInfoFileURL=https://cdn.test/Version.ini
BackupVersionURL=https://cdn.test/Backup.ini
Branch=publish_test
MutexName=nte-mutex
PATCHER_IGNORED_SECTION= A , ,B 
"#,
        );
        let cfg = LauncherConfig::load(dir.path()).expect("load config");
        assert_eq!(cfg.launcher, "MyLauncher.exe");
        assert_eq!(cfg.patcher, "MyPatcher.exe");
        assert_eq!(cfg.client, "MyGame.exe");
        assert_eq!(cfg.game_id, 1289);
        assert_eq!(cfg.launch_type, 3);
        assert_eq!(cfg.version, "1.4.16");
        assert_eq!(cfg.build, "20240101");
        assert_eq!(cfg.version_url, "https://cdn.test/Version.ini");
        assert_eq!(cfg.backup_version_url, "https://cdn.test/Backup.ini");
        assert_eq!(cfg.branch, "publish_test");
        assert_eq!(cfg.mutex_name, "nte-mutex");
        // 空白项被丢掉，只留下真正的段名
        assert_eq!(cfg.patcher_ignored_sections, vec!["A", "B"]);
        assert_eq!(cfg.launcher_dir(), dir.path());
        assert_eq!(
            cfg.user_config(),
            dir.path()
                .join("UserData")
                .join("Config")
                .join("Config.ini")
        );
        assert_eq!(
            cfg.game_dir(),
            dir.path().join("..").join("Client").join("WindowsNoEditor")
        );
    }

    #[test]
    fn launcher_config_falls_back_to_defaults() {
        let dir = TempDir::new("cfg-default");
        dir.write("Config/Config.ini", "[General]\nGameID=not-a-number\n");
        let cfg = LauncherConfig::load(dir.path()).expect("load config");
        assert_eq!(cfg.launcher, "NTELauncher.exe");
        assert_eq!(cfg.patcher, "NTEUpdate.exe");
        assert_eq!(cfg.client, "NTEGame.exe");
        assert_eq!(cfg.game_id, 0);
        assert_eq!(cfg.launch_type, 0);
        assert_eq!(cfg.version, "");
        assert_eq!(cfg.build, "");
        assert_eq!(cfg.version_url, crate::VERSION_URL);
        assert_eq!(cfg.backup_version_url, crate::BACKUP_VERSION_URL);
        assert_eq!(cfg.branch, "publish_ob");
        assert_eq!(cfg.mutex_name, "");
        assert!(cfg.patcher_ignored_sections.is_empty());
    }

    #[test]
    fn launcher_config_requires_config_ini() {
        let dir = TempDir::new("cfg-missing");
        assert!(LauncherConfig::load(dir.path()).is_err());
    }
}
