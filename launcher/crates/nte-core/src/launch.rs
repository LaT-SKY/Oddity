use std::path::{Path, PathBuf};
use std::process::Command;

/// UE map and extra args observed in NTEGame.log when the launcher starts the game.
pub const GAME_ARGS: [&str; 2] = [
    "/Game/LoginAndCreate/Map/Updater/Updater_P",
    "-SAVEWINPOS=1",
];

#[derive(Debug, Clone)]
pub struct ProtonLaunch {
    pub runner: PathBuf,
    pub prefix: PathBuf,
    pub exe: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl ProtonLaunch {
    pub fn umu(prefix: &Path, exe: &Path) -> Self {
        Self {
            runner: PathBuf::from("umu-run"),
            prefix: prefix.to_path_buf(),
            exe: exe.to_path_buf(),
            args: GAME_ARGS.iter().map(|s| s.to_string()).collect(),
            env: vec![
                ("WINEPREFIX".into(), prefix.display().to_string()),
                ("PROTON_ENABLE_WAYLAND".into(), "1".into()),
            ],
        }
    }

    pub fn command(&self) -> Command {
        let mut cmd = Command::new(&self.runner);
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        cmd.arg(&self.exe).args(&self.args);
        cmd
    }

    /// The shipped launcher writes GameLauncher.config as one line holding the
    /// path of WPGameClientSDK_64.dll before starting HTGame.exe.
    pub fn write_game_launcher_config(game_bin_dir: &Path, sdk_dll: &Path) -> std::io::Result<()> {
        std::fs::write(
            game_bin_dir.join("GameLauncher.config"),
            sdk_dll.display().to_string(),
        )
    }
}
