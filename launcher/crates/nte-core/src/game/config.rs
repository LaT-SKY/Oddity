use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{distribution::Distribution, Error, Result};

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
            fs::read_to_string(config_dir.join("PatcherConfig.json")).map_err(Error::Io)?;
        let pc: PatcherConfigJson = serde_json::from_str(&patcher_json)
            .map_err(|e| Error::Parse(format!("PatcherConfig.json: {e}")))?;
        let branch = pc
            .branch_name
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| Error::Parse("PatcherConfig.json has no branchName".into()))?;
        let app_id = pc
            .app_id
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| Error::Parse("PatcherConfig.json has no appId".into()))?;
        if game_id > 0 && app_id != game_id.to_string() {
            return Err(Error::Integrity(
                "PatcherConfig.json appId 与游戏地区不一致".into(),
            ));
        }
        let distribution = app_id.parse().ok().and_then(Distribution::from_game_id);
        let mut cdns: Vec<String> = pc
            .game_res_url
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect();
        if cdns.is_empty() {
            cdns = pc
                .game_update_asset_url
                .into_iter()
                .filter(|s| !s.trim().is_empty())
                .collect();
        }
        if cdns.is_empty() {
            let region =
                distribution.ok_or_else(|| Error::Parse("未知 appId 缺少资源 CDN 配置".into()))?;
            cdns = region
                .resource_cdns()
                .into_iter()
                .map(str::to_string)
                .collect();
        }
        let update_branch = pc
            .update_branch_name
            .filter(|s| !s.trim().is_empty())
            .or_else(|| distribution.map(|region| region.update_branch().to_string()))
            .ok_or_else(|| Error::Parse("PatcherConfig.json has no updateBranchName".into()))?;

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
            update_branch,
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
        return Err(Error::Parse(format!(
            "no PatcherConfig.json for GameID={game_id}"
        )));
    }
    let root = launcher_root.join("ResFilesM");
    let entries = fs::read_dir(&root).map_err(Error::Io)?;
    let mut candidates = entries
        .flatten()
        .map(|entry| entry.path().join("PatcherConfig"))
        .filter(|path| path.join("PatcherConfig.json").is_file());
    let first = candidates.next().ok_or_else(|| {
        Error::Parse(format!(
            "no ResFilesM/*/PatcherConfig/PatcherConfig.json under {}",
            launcher_root.display()
        ))
    })?;
    if candidates.next().is_some() {
        return Err(Error::Integrity(
            "多个资源配置目录且 GameID 未指定, 无法确定游戏地区".into(),
        ));
    }
    Ok(first)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    const PATCHER_DIR: &str = "install/NTELauncher/ResFilesM/1289/PatcherConfig";

    fn launcher_root(dir: &TempDir) -> PathBuf {
        dir.join("install/NTELauncher")
    }

    #[test]
    fn loads_the_full_patcher_configuration() {
        let dir = TempDir::new("gamecfg");
        dir.write(
            format!("{PATCHER_DIR}/PatcherConfig.json"),
            r#"{"gameResUrl":["https://a.test/cdn/","https://b.test/cdn"],"branchName":"publish_ob","updateBranchName":"publish_up","appId":"1289"}"#,
        );
        dir.write(
            format!("{PATCHER_DIR}/PatcherPath.json"),
            r#"{"check_dir_list":["Client/Content/Paks"],"ResRoot":"X","Patcher":"Y"}"#,
        );
        dir.write(
            format!("{PATCHER_DIR}/PatcherCheckConfig.json"),
            r#"[{"type":"sig","maxSize":100},{"type":".utoc","maxSize":200},{"type":"","maxSize":10},{"type":"bin","maxSize":0}]"#,
        );
        dir.write(
            "install/NTELauncher/UserData/Config/Config.ini",
            "[Game]\nVersion_=1.4.16\n",
        );

        let root = launcher_root(&dir);
        let cfg = GameUpdateConfig::load(&root, 1289).expect("load config");
        assert_eq!(cfg.config_dir, dir.join(PATCHER_DIR));
        assert_eq!(cfg.install_root, dir.join("install"));
        assert_eq!(
            cfg.patcher_dir,
            root.join("UserData").join("Patcher").join("PatcherSDK")
        );
        assert_eq!(cfg.cdns, vec!["https://a.test/cdn/", "https://b.test/cdn"]);
        assert_eq!(cfg.branch, "publish_ob");
        assert_eq!(cfg.update_branch, "publish_up");
        assert_eq!(cfg.app_id, "1289");
        assert_eq!(cfg.check_dirs, vec!["Client/Content/Paks"]);
        assert_eq!(cfg.max_threads, 5);
        // 空类型和 0 上限的规则被丢弃，缺少点号的扩展名补上点号
        assert_eq!(cfg.rules.len(), 2);
        assert_eq!(cfg.rules[0].extension, ".sig");
        assert_eq!(cfg.rules[0].max_size, 100);
        assert_eq!(cfg.rules[1].extension, ".utoc");
        assert_eq!(cfg.game_dir(), dir.join("install/Client/WindowsNoEditor"));
        assert_eq!(cfg.local_version().as_deref(), Some("1.4.16"));

        assert!(cfg.force_hash("Client/Content/Paks/a.sig", 100));
        assert!(cfg.force_hash("Client/Content/Paks/A.SIG", 100));
        assert!(!cfg.force_hash("Client/Content/Paks/a.sig", 101));
        assert!(!cfg.force_hash("Client/Content/Paks/a.pak", 10));
    }

    #[test]
    fn falls_back_to_the_update_asset_url_and_default_cdn() {
        let dir = TempDir::new("gamecfg-cdn");
        dir.write(
            format!("{PATCHER_DIR}/PatcherConfig.json"),
            r#"{"gameUpdateAssetUrl":["https://assets.test/res"],"branchName":"publish_ob","appId":"1289"}"#,
        );
        let cfg = GameUpdateConfig::load(&launcher_root(&dir), 0).expect("load config");
        assert_eq!(cfg.cdns, vec!["https://assets.test/res"]);
        // 没有 PatcherPath.json 时 check_dir_list 为空
        assert!(cfg.check_dirs.is_empty());
        // 没有 PatcherCheckConfig.json 时使用默认规则
        assert_eq!(cfg.rules.len(), 2);
        assert_eq!(cfg.rules[0].extension, ".sig");
        assert_eq!(cfg.rules[1].extension, ".utoc");
        assert_eq!(cfg.update_branch, "publish_Updata");
        assert!(cfg.local_version().is_none());

        dir.write(
            format!("{PATCHER_DIR}/PatcherConfig.json"),
            r#"{"branchName":"publish_ob","appId":"1289"}"#,
        );
        let cfg = GameUpdateConfig::load(&launcher_root(&dir), 0).expect("load config");
        assert_eq!(cfg.cdns, Distribution::Cn.resource_cdns());
    }

    #[test]
    fn global_resources_use_global_cdns_and_update_branch() {
        let dir = TempDir::new("gamecfg-global");
        let path = "install/NTEGlobal/ResFilesM/3000001/PatcherConfig/PatcherConfig.json";
        dir.write(path, r#"{"branchName":"PC_140","appId":"3000001"}"#);
        let root = dir.join("install/NTEGlobal");
        let cfg = GameUpdateConfig::load(&root, 3000001).unwrap();
        assert_eq!(cfg.cdns, Distribution::Global.resource_cdns());
        assert_eq!(cfg.update_branch, "Publish_Update");
        assert_eq!(cfg.branch, "PC_140");
        dir.write(path, r#"{"branchName":"PC_140","appId":"1289"}"#);
        assert!(matches!(
            GameUpdateConfig::load(&root, 3000001),
            Err(Error::Integrity(_))
        ));
    }

    #[test]
    fn rejects_incomplete_patcher_configs() {
        let dir = TempDir::new("gamecfg-bad");
        let root = launcher_root(&dir);

        let error = GameUpdateConfig::load(&root, 0).expect_err("no ResFilesM");
        assert!(matches!(error, Error::Io(_)), "{error:?}");

        std::fs::create_dir_all(root.join("ResFilesM")).expect("create ResFilesM");
        let error = GameUpdateConfig::load(&root, 0).expect_err("no config dir");
        assert!(error.to_string().contains("no ResFilesM"), "{error}");

        dir.write(format!("{PATCHER_DIR}/PatcherConfig.json"), "{ not json");
        let error = GameUpdateConfig::load(&root, 1289).expect_err("bad json");
        assert!(error.to_string().contains("PatcherConfig.json"), "{error}");

        dir.write(
            format!("{PATCHER_DIR}/PatcherConfig.json"),
            r#"{"branchName":"","appId":"1289"}"#,
        );
        let error = GameUpdateConfig::load(&root, 1289).expect_err("no branch");
        assert!(error.to_string().contains("no branchName"), "{error}");

        dir.write(
            format!("{PATCHER_DIR}/PatcherConfig.json"),
            r#"{"branchName":"publish_ob"}"#,
        );
        let error = GameUpdateConfig::load(&root, 1289).expect_err("no app id");
        assert!(error.to_string().contains("no appId"), "{error}");

        dir.write(
            format!("{PATCHER_DIR}/PatcherConfig.json"),
            r#"{"branchName":"publish_ob","appId":"1289"}"#,
        );
        dir.write(format!("{PATCHER_DIR}/PatcherCheckConfig.json"), "[{}]");
        let error = GameUpdateConfig::load(&root, 1289).expect_err("bad rules");
        assert!(
            error.to_string().contains("PatcherCheckConfig.json"),
            "{error}"
        );
    }

    #[test]
    fn rejects_other_game_ids_and_scans_when_id_is_unknown() {
        let dir = TempDir::new("gamecfg-scan");
        // 已知 GameID 不得扫描其他区服的资源配置.
        dir.write(
            "install/NTELauncher/ResFilesM/4321/PatcherConfig/PatcherConfig.json",
            r#"{"branchName":"custom","updateBranchName":"custom_update","appId":"4321","gameResUrl":["https://custom.test"]}"#,
        );
        assert!(GameUpdateConfig::load(&launcher_root(&dir), 1289).is_err());
        let cfg = GameUpdateConfig::load(&launcher_root(&dir), 0).expect("load config");
        assert_eq!(cfg.app_id, "4321");
        assert!(cfg.config_dir.ends_with("ResFilesM/4321/PatcherConfig"));
    }

    #[test]
    fn patcher_path_json_is_ignored_when_broken() {
        let dir = TempDir::new("gamecfg-path");
        dir.write(
            format!("{PATCHER_DIR}/PatcherConfig.json"),
            r#"{"branchName":"publish_ob","appId":"1289"}"#,
        );
        dir.write(format!("{PATCHER_DIR}/PatcherPath.json"), "not json");
        let cfg = GameUpdateConfig::load(&launcher_root(&dir), 1289).expect("load config");
        assert!(cfg.check_dirs.is_empty());
    }
}
