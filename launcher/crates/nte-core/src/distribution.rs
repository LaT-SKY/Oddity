use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Distribution {
    #[default]
    Cn,
    Global,
}

impl FromStr for Distribution {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cn" => Ok(Self::Cn),
            "global" => Ok(Self::Global),
            _ => Err(format!("无法识别游戏地区: {value}. 可选值: cn, global.")),
        }
    }
}

impl Distribution {
    pub fn from_game_id(game_id: i64) -> Option<Self> {
        match game_id {
            1289 => Some(Self::Cn),
            3000001 => Some(Self::Global),
            _ => None,
        }
    }

    pub fn game_id(self) -> i64 {
        match self {
            Self::Cn => 1289,
            Self::Global => 3000001,
        }
    }

    pub fn launcher_exe(self) -> &'static str {
        match self {
            Self::Cn => "NTELauncher.exe",
            Self::Global => "NTEGlobalLauncher.exe",
        }
    }

    pub fn patcher_exe(self) -> &'static str {
        match self {
            Self::Cn => "NTEUpdate.exe",
            Self::Global => "NTEGlobalUpdate.exe",
        }
    }

    pub fn launcher_branch(self) -> &'static str {
        match self {
            Self::Cn => "publish_ob",
            Self::Global => "publish_PC",
        }
    }

    pub fn update_branch(self) -> &'static str {
        match self {
            Self::Cn => "publish_Updata",
            Self::Global => "Publish_Update",
        }
    }

    pub fn resource_cdns(self) -> [&'static str; 2] {
        match self {
            Self::Cn => [
                "https://yhcdn1.wmupd.com/clientRes",
                "https://yhcdn2.wmupd.com/clientRes",
            ],
            Self::Global => [
                "https://ntecdn1.perfectworld.com/clientRes",
                "https://ntecdn2.perfectworld.com/clientRes",
            ],
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cn => "cn",
            Self::Global => "global",
        }
    }

    pub fn launcher_dir(self) -> &'static str {
        match self {
            Self::Cn => "NTELauncher",
            Self::Global => "NTEGlobal",
        }
    }

    pub fn client_exe(self) -> &'static str {
        match self {
            Self::Cn => "NTEGame.exe",
            Self::Global => "NTEGlobalGame.exe",
        }
    }

    pub fn version_urls(self) -> [&'static str; 2] {
        match self {
            Self::Cn => [
                "https://yhcdn1.wmupd.com/hd/publish_ob/launcher/Version.ini",
                "https://yhcdn2.wmupd.com/hd/publish_ob/launcher/Version.ini",
            ],
            Self::Global => [
                "https://ntecdn1.perfectworld.com/hd/publish_PC/launcher/Version.ini",
                "https://ntecdn2.perfectworld.com/hd/publish_PC/launcher/Version.ini",
            ],
        }
    }

    pub fn default_install_name(self) -> &'static str {
        match self {
            Self::Cn => "Neverness To Everness",
            Self::Global => "Neverness To Everness Global",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cn => "国服",
            Self::Global => "国际服",
        }
    }

    pub fn game_name(self) -> &'static str {
        match self {
            Self::Cn => "异环",
            Self::Global => "NTE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_region_names_case_insensitively() {
        assert_eq!("cn".parse::<Distribution>().unwrap(), Distribution::Cn);
        assert_eq!(
            " Global ".parse::<Distribution>().unwrap(),
            Distribution::Global
        );
        let error = "eu".parse::<Distribution>().unwrap_err();
        assert!(error.contains("cn, global"));
    }

    #[test]
    fn labels_match_the_two_distributions() {
        assert_eq!(Distribution::Cn.label(), "国服");
        assert_eq!(Distribution::Global.label(), "国际服");
        assert_eq!(Distribution::Cn.game_name(), "异环");
        assert_eq!(Distribution::Global.game_name(), "NTE");
    }
}
