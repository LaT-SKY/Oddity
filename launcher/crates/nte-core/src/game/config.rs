use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct CheckRule {
    pub extension: String,
    pub max_size: u64,
}

#[derive(Debug, Clone)]
pub struct GameUpdateConfig {
    pub launcher_root: PathBuf,
    pub install_root: PathBuf,
    pub config_dir: PathBuf,
    pub patcher_dir: PathBuf,
    pub branch: String,
    pub update_branch: String,
    pub app_id: String,
    pub cdns: Vec<String>,
    pub rules: Vec<CheckRule>,
    pub check_dirs: Vec<String>,
    pub max_threads: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatcherConfigJson {
    #[serde(default)]
    game_res_url: Vec<String>,
    #[serde(default)]
    game_update_asset_url: Vec<String>,
    branch_name: Option<String>,
    update_branch_name: Option<String>,
    app_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
struct PatcherPathJson {
    #[serde(rename = "ResRoot", default)]
    res_root: Option<String>,
    #[serde(rename = "Patcher", default)]
    patcher: Option<String>,
    #[serde(rename = "check_dir_list", default)]
    check_dir_list: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawCheckRule {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "maxSize")]
    max_size: u64,
}

impl GameUpdateConfig {
    pub fn load(launcher_root: &Path, game_id: i64) -> Result<Self> {
        let config_dir = find_config_dir(launcher_root, game_id)?;
        let patcher_json =
            fs::read_to_string(config_dir.join("PatcherConfig.json")).map_err(|e| Error::Io(e))?;
        let pc: PatcherConfigJson = serde_json::from_str(&patcher_json)
            .map_err(|e| Error::Parse(format!("PatcherConfig.json: {e}")))?;
        let mut cdns = pc.game_res_url;
        if cdns.is_empty() {
            cdns = pc.game_update_asset_url;
        }
        if cdns.is_empty() {
            cdns.push("https://yhcdn1.wmupd.com/clientRes".to_string());
        }
        let branch = pc
            .branch_name
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::Parse("PatcherConfig.json has no branchName".into()))?;
        let app_id = pc
            .app_id
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::Parse("PatcherConfig.json has no appId".into()))?;

        let path_json = fs::read_to_string(config_dir.join("PatcherPath.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<PatcherPathJson>(&s).ok())
            .unwrap_or_default();

        let rules = match fs::read_to_string(config_dir.join("PatcherCheckConfig.json")) {
            Ok(s) => serde_json::from_str::<Vec<RawCheckRule>>(&s)
                .map_err(|e| Error::Parse(format!("PatcherCheckConfig.json: {e}")))?
                .into_iter()
                .filter(|r| !r.kind.is_empty() && r.max_size > 0)
                .map(|r| CheckRule {
                    extension: normalize_extension(&r.kind),
                    max_size: r.max_size,
                })
                .collect(),
            Err(_) => default_rules(),
        };

        let install_root = launcher_root
            .parent()
            .ok_or_else(|| Error::Parse("launcher root has no parent install dir".into()))?
            .to_path_buf();
        let patcher_dir = launcher_root
            .join("UserData")
            .join("Patcher")
            .join("PatcherSDK");

        Ok(Self {
            launcher_root: launcher_root.to_path_buf(),
            install_root,
            config_dir,
            patcher_dir,
            branch,
            update_branch: pc
                .update_branch_name
                .unwrap_or_else(|| "publish_Updata".into()),
            app_id,
            cdns,
            rules,
            check_dirs: path_json.check_dir_list,
            max_threads: 5,
        })
    }

    pub fn game_dir(&self) -> PathBuf {
        self.install_root.join("Client").join("WindowsNoEditor")
    }

    pub fn local_version(&self) -> Option<String> {
        let ini = crate::config::Ini::load(
            &self
                .launcher_root
                .join("UserData")
                .join("Config")
                .join("Config.ini"),
        )
        .ok()?;
        ini.get("Game", "Version_").map(|s| s.to_string())
    }

    pub fn force_hash(&self, path: &str, size: u64) -> bool {
        let lower = path.to_ascii_lowercase();
        self.rules
            .iter()
            .any(|r| lower.ends_with(&r.extension) && size <= r.max_size)
    }
}

fn find_config_dir(launcher_root: &Path, game_id: i64) -> Result<PathBuf> {
    if game_id > 0 {
        let p = launcher_root
            .join("ResFilesM")
            .join(game_id.to_string())
            .join("PatcherConfig");
        if p.join("PatcherConfig.json").is_file() {
            return Ok(p);
        }
    }
    let root = launcher_root.join("ResFilesM");
    let entries = fs::read_dir(&root).map_err(|e| Error::Io(e))?;
    for entry in entries.flatten() {
        let p = entry.path().join("PatcherConfig");
        if p.join("PatcherConfig.json").is_file() {
            return Ok(p);
        }
    }
    Err(Error::Parse(format!(
        "no ResFilesM/*/PatcherConfig/PatcherConfig.json under {}",
        launcher_root.display()
    )))
}

fn normalize_extension(value: &str) -> String {
    let mut s = value.trim().to_ascii_lowercase();
    if !s.starts_with('.') {
        s.insert(0, '.');
    }
    s
}

fn default_rules() -> Vec<CheckRule> {
    vec![
        CheckRule {
            extension: ".sig".into(),
            max_size: 5 * 1024 * 1024,
        },
        CheckRule {
            extension: ".utoc".into(),
            max_size: 20 * 1024 * 1024,
        },
    ]
}
