use std::path::Path;
use std::process::Command;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowMode {
    #[default]
    Fullscreen,
    Borderless,
    Windowed,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scaling {
    #[default]
    Linear,
    Nearest,
    Fsr,
    Nis,
    Integer,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub enabled: bool,
    pub window_mode: WindowMode,
    pub game_width: u32,
    pub game_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub fps: u32,
    pub scaling: Scaling,
}

impl Options {
    pub fn validate(&self) -> Result<()> {
        for (width, height) in [
            (self.game_width, self.game_height),
            (self.output_width, self.output_height),
        ] {
            if width > 16384 || height > 16384 {
                bail!("分辨率范围为 0-16384, 0 表示使用默认值");
            }
        }
        if self.fps > 1000 {
            bail!("帧率限制范围为 0-1000, 0 表示不限制");
        }
        Ok(())
    }

    /// 参数直接传给进程, 不经过 shell. 可复用于不同的游戏运行器.
    pub fn command(&self, runner: &Path, overlay: bool) -> Result<Command> {
        self.validate()?;
        if !self.enabled {
            return Ok(Command::new(runner));
        }
        let executable =
            super::executable("gamescope").ok_or_else(|| anyhow::anyhow!("未找到 Gamescope"))?;
        let mut command = Command::new(executable);
        match self.window_mode {
            WindowMode::Fullscreen => {
                command.arg("-f");
            }
            WindowMode::Borderless => {
                command.arg("-b");
            }
            WindowMode::Windowed => {}
        }
        for (flag, value) in [
            ("-w", self.game_width),
            ("-h", self.game_height),
            ("-W", self.output_width),
            ("-H", self.output_height),
            ("-r", self.fps),
        ] {
            if value != 0 {
                command.arg(flag).arg(value.to_string());
            }
        }
        match self.scaling {
            Scaling::Linear => {
                command.args(["-F", "linear"]);
            }
            Scaling::Nearest => {
                command.args(["-F", "nearest"]);
            }
            Scaling::Fsr => {
                command.args(["-F", "fsr"]);
            }
            Scaling::Nis => {
                command.args(["-F", "nis"]);
            }
            Scaling::Integer => {
                command.args(["-S", "integer"]);
            }
        }
        if overlay {
            if super::executable("mangoapp").is_none() {
                bail!("Gamescope 性能浮层需要 mangoapp");
            }
            command.arg("--mangoapp").env_remove("MANGOHUD");
        }
        command.arg("--").arg(runner);
        Ok(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{EnvGuard, TempDir};
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn gamescope_wraps_the_runner_and_uses_its_own_overlay() {
        let mut env = EnvGuard::new();
        let dir = TempDir::new("gamescope-command");
        for name in ["gamescope", "mangoapp"] {
            let file = dir.write(name, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n");
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        env.set("PATH", dir.path());
        let options = Options {
            enabled: true,
            game_width: 1280,
            game_height: 720,
            output_width: 1920,
            output_height: 1080,
            fps: 60,
            scaling: Scaling::Fsr,
            ..Default::default()
        };
        let mut command = options
            .command(Path::new("/runner with spaces/umu-run"), true)
            .unwrap();
        command.arg("game with spaces.exe");
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            [
                "-f",
                "-w",
                "1280",
                "-h",
                "720",
                "-W",
                "1920",
                "-H",
                "1080",
                "-r",
                "60",
                "-F",
                "fsr",
                "--mangoapp",
                "--",
                "/runner with spaces/umu-run",
                "game with spaces.exe"
            ]
        );
        assert!(command
            .get_envs()
            .any(|(key, value)| key == "MANGOHUD" && value.is_none()));
        for (scaling, flag, value) in [
            (Scaling::Linear, "-F", "linear"),
            (Scaling::Nearest, "-F", "nearest"),
            (Scaling::Nis, "-F", "nis"),
            (Scaling::Integer, "-S", "integer"),
        ] {
            let options = Options {
                enabled: true,
                scaling,
                window_mode: WindowMode::Borderless,
                ..Default::default()
            };
            let args: Vec<_> = options
                .command(Path::new("umu-run"), false)
                .unwrap()
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();
            assert_eq!(args, ["-b", flag, value, "--", "umu-run"]);
        }
    }

    #[test]
    fn missing_tools_invalid_values_and_disabled_mode_are_distinct() {
        let mut env = EnvGuard::new();
        let dir = TempDir::new("gamescope-missing");
        env.set("PATH", dir.path());
        let runner = Path::new("umu-run");
        assert_eq!(
            Options::default()
                .command(runner, false)
                .unwrap()
                .get_program(),
            runner
        );
        let mut options = Options {
            enabled: true,
            window_mode: WindowMode::Windowed,
            ..Default::default()
        };
        assert!(options
            .command(runner, false)
            .unwrap_err()
            .to_string()
            .contains("未找到 Gamescope"));
        let file = dir.write("gamescope", "#!/bin/sh\nexit 0\n");
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(options
            .command(runner, true)
            .unwrap_err()
            .to_string()
            .contains("mangoapp"));
        options.fps = 1001;
        assert!(options.validate().is_err());
        options.fps = 0;
        options.game_width = 16385;
        assert!(options.validate().is_err());
        options.game_width = 0;
        assert_eq!(
            options
                .command(runner, false)
                .unwrap()
                .get_args()
                .collect::<Vec<_>>(),
            ["-F", "linear", "--", "umu-run"]
        );
    }
}
