use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Distribution {
    #[default]
    Cn,
    Global,
}

impl Distribution {
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
}
