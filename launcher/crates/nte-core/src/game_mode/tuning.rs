use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub pin_cores: Option<String>,
    pub park_cores: Option<String>,
    pub renice: Option<u8>,
    pub ioprio: Option<String>,
}

impl Config {
    fn validate(&self) -> Result<()> {
        if self.renice.is_some_and(|value| value > 20) {
            bail!("进程优先级范围为 0-20");
        }
        if self.ioprio.as_ref().is_some_and(|value| {
            !matches!(
                value.as_str(),
                "off" | "reset" | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7"
            )
        }) {
            bail!("I/O 优先级应为 0-7、off 或 reset");
        }
        for value in [&self.pin_cores, &self.park_cores].into_iter().flatten() {
            if matches!(value.as_str(), "yes" | "true" | "1" | "no" | "false" | "0") {
                continue;
            }
            if value.is_empty() || value.len() > 4096 {
                bail!("CPU 核心列表无效");
            }
            for entry in value.split(',') {
                let (first, last) = entry.split_once('-').unwrap_or((entry, entry));
                let first: u16 = first.parse().context("CPU 核心列表无效")?;
                let last: u16 = last.parse().context("CPU 核心列表无效")?;
                if first > last {
                    bail!("CPU 核心范围起点不能大于终点");
                }
            }
        }
        Ok(())
    }

    fn apply_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        super::config::write(
            path,
            &[
                (
                    "cpu",
                    vec![
                        ("pin_cores", self.pin_cores.clone()),
                        ("park_cores", self.park_cores.clone()),
                    ],
                ),
                (
                    "general",
                    vec![
                        ("renice", self.renice.map(|value| value.to_string())),
                        ("ioprio", self.ioprio.clone()),
                    ],
                ),
            ],
        )
    }
}

fn parse(text: &str) -> Result<Config> {
    let mut config = Config::default();
    let mut section = "";
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
            continue;
        }
        if line.starts_with([';', '#']) {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.split([';', '#']).next().unwrap_or_default().trim();
        match (section.to_ascii_lowercase().as_str(), key.trim()) {
            ("[cpu]", "pin_cores") => config.pin_cores = Some(value.into()),
            ("[cpu]", "park_cores") => config.park_cores = Some(value.into()),
            ("[general]", "renice") => {
                config.renice = Some(value.parse().context("进程优先级无效")?)
            }
            ("[general]", "ioprio") => config.ioprio = Some(value.into()),
            _ => {}
        }
    }
    config.validate()?;
    Ok(config)
}

pub fn info() -> Result<Config> {
    parse(&super::config::read()?)
}

pub fn apply_system_json(input: impl Read) -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        bail!("修改系统 GameMode 配置需要管理员权限");
    }
    let config: Config =
        serde_json::from_reader(input.take(16384)).context("CPU 与优先级配置无效")?;
    config.apply_to(Path::new("/etc/gamemode.ini"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn tuning_preserves_gpu_and_other_settings_and_uses_the_upstream_sections() {
        let dir = TempDir::new("gamemode-tuning");
        let original = "[general]\n; custom\ndesiredgov=performance\nrenice=0\n[cpu]\npin_cores=yes\npark_cores=no\n[gpu]\napply_gpu_optimisations=accept-responsibility\n[custom]\nstart=example\n";
        let path = dir.write("gamemode.ini", original);
        let config = Config {
            pin_cores: Some("0-3,8-11".into()),
            park_cores: Some("no".into()),
            renice: Some(10),
            ioprio: Some("reset".into()),
        };
        config.apply_to(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("desiredgov=performance") && text.contains("; custom"));
        assert!(
            text.contains("apply_gpu_optimisations=accept-responsibility")
                && text.contains("start=example")
        );
        assert_eq!(text.matches("renice=").count(), 1);
        assert!(!text.contains("[priority]"));
        let loaded = parse(&text).unwrap();
        assert_eq!(loaded.renice, Some(10));
        assert_eq!(loaded.pin_cores.as_deref(), Some("0-3,8-11"));
        assert_eq!(loaded.ioprio.as_deref(), Some("reset"));
        assert_eq!(
            std::fs::read_to_string(path.with_extension("ini.oddity-backup")).unwrap(),
            original
        );
        Config::default().apply_to(&path).unwrap();
        let restored = std::fs::read_to_string(&path).unwrap();
        assert!(!restored.contains("pin_cores=") && !restored.contains("renice="));
        assert!(restored.contains("desiredgov=performance") && restored.contains("[gpu]"));
    }

    #[test]
    fn tuning_creates_sections_without_touching_other_configuration() {
        let dir = TempDir::new("gamemode-tuning-new");
        let path = dir.join("gamemode.ini");
        Config {
            pin_cores: Some("yes".into()),
            renice: Some(0),
            ..Default::default()
        }
        .apply_to(&path)
        .unwrap();
        let first = std::fs::read_to_string(&path).unwrap();
        assert_eq!(parse(&first).unwrap().pin_cores.as_deref(), Some("yes"));
        assert_eq!(parse(&first).unwrap().renice, Some(0));
        let text = format!("[gpu]\n; GPU settings\nnv_core_clock_mhz_offset=50\n{first}");
        std::fs::write(&path, &text).unwrap();
        Config {
            park_cores: Some("2-5".into()),
            ioprio: Some("7".into()),
            ..Default::default()
        }
        .apply_to(&path)
        .unwrap();
        let updated = std::fs::read_to_string(&path).unwrap();
        assert!(updated.contains("; GPU settings\nnv_core_clock_mhz_offset=50"));
        assert_eq!(parse(&updated).unwrap().park_cores.as_deref(), Some("2-5"));
        assert!(parse(&updated).unwrap().pin_cores.is_none());
    }

    #[test]
    fn invalid_tuning_never_changes_the_file() {
        let dir = TempDir::new("gamemode-tuning-invalid");
        let path = dir.write("gamemode.ini", "[general]\nrenice=0\n");
        for value in ["", "-1", "3-0", "1,", "yes\n[custom]", "1-2-3", "65536"] {
            assert!(Config {
                pin_cores: Some(value.into()),
                ..Default::default()
            }
            .apply_to(&path)
            .is_err());
        }
        for value in ["off", "reset", "0", "7"] {
            Config {
                ioprio: Some(value.into()),
                ..Default::default()
            }
            .validate()
            .unwrap();
        }
        for value in ["yes", "no", "true", "false", "1", "0", "2,4-6"] {
            Config {
                park_cores: Some(value.into()),
                ..Default::default()
            }
            .validate()
            .unwrap();
        }
        assert!(Config {
            renice: Some(21),
            ..Default::default()
        }
        .apply_to(&path)
        .is_err());
        assert!(Config {
            ioprio: Some("8".into()),
            ..Default::default()
        }
        .apply_to(&path)
        .is_err());
        assert!(parse("[general]\nrenice=unknown\n").is_err());
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "[general]\nrenice=0\n"
        );
        if unsafe { libc::geteuid() } != 0 {
            assert!(apply_system_json("{}".as_bytes()).is_err());
        }
    }
}
