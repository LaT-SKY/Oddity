use std::path::{Path, PathBuf};
use std::process::Command;

pub mod background;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn umu_launch_carries_prefix_and_game_args() {
        let launch = ProtonLaunch::umu(Path::new("/prefix"), Path::new("/tmp/NTEGame.exe"));
        assert_eq!(launch.runner, PathBuf::from("umu-run"));
        assert_eq!(launch.prefix, PathBuf::from("/prefix"));
        assert_eq!(launch.exe, PathBuf::from("/tmp/NTEGame.exe"));
        assert_eq!(launch.args, GAME_ARGS.to_vec());
        assert!(launch
            .env
            .contains(&("WINEPREFIX".to_string(), "/prefix".to_string())));
        assert!(launch
            .env
            .contains(&("PROTON_ENABLE_WAYLAND".to_string(), "1".to_string())));

        let cmd = launch.command();
        assert_eq!(cmd.get_program().to_string_lossy(), "umu-run");
        let args: Vec<String> = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            args,
            vec![
                "/tmp/NTEGame.exe".to_string(),
                GAME_ARGS[0].to_string(),
                GAME_ARGS[1].to_string()
            ]
        );
        let envs: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().to_string(),
                    value.map(|value| value.to_string_lossy().to_string()),
                )
            })
            .collect();
        assert!(envs.contains(&("WINEPREFIX".to_string(), Some("/prefix".to_string()))));
        assert!(envs.contains(&("PROTON_ENABLE_WAYLAND".to_string(), Some("1".to_string()))));
    }

    #[test]
    fn writes_a_single_line_game_launcher_config() {
        let dir = TempDir::new("launch");
        let bin_dir = dir.join("Binaries/Win64");
        std::fs::create_dir_all(&bin_dir).expect("create bin dir");
        ProtonLaunch::write_game_launcher_config(
            &bin_dir,
            Path::new("Z:\\game\\WPGameClientSDK_64.dll"),
        )
        .expect("write config");
        assert_eq!(
            dir.read_text("Binaries/Win64/GameLauncher.config"),
            "Z:\\game\\WPGameClientSDK_64.dll"
        );
    }
}
