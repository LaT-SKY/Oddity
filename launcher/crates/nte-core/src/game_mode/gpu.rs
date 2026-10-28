//! GPU 调优由系统 GameMode 执行, 此模块只读写其受信任的系统配置.
use std::fs;
use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Vendor {
    #[default]
    Nvidia,
    Amd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: u32,
    pub vendor: Vendor,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AmdLevel {
    #[default]
    Auto,
    High,
    Low,
    ProfileStandard,
    ProfilePeak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub enabled: bool,
    pub device: u32,
    pub vendor: Vendor,
    pub core_offset: i32,
    pub memory_offset: i32,
    pub power_mode: i32,
    pub amd_level: AmdLevel,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            device: 0,
            vendor: Vendor::Nvidia,
            core_offset: -1,
            memory_offset: -1,
            power_mode: -1,
            amd_level: AmdLevel::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub config: Config,
    pub devices: Vec<Device>,
}

fn devices_at(root: &Path) -> Vec<Device> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut devices: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let id = name.to_str()?.strip_prefix("card")?.parse().ok()?;
            let vendor = match fs::read_to_string(entry.path().join("device/vendor"))
                .ok()?
                .trim()
            {
                "0x10de" => Vendor::Nvidia,
                "0x1002" => Vendor::Amd,
                _ => return None,
            };
            Some(Device { id, vendor })
        })
        .collect();
    devices.sort_by_key(|device| device.id);
    devices
}

pub fn info() -> Result<Info> {
    let devices = devices_at(Path::new("/sys/class/drm"));
    let text = super::config::read()?;
    let mut config = parse(&text)?;
    if let Some(device) = devices
        .iter()
        .find(|device| device.id == config.device)
        .or(devices.first())
    {
        config.device = device.id;
        config.vendor = device.vendor;
    }
    Ok(Info { config, devices })
}

fn parse(text: &str) -> Result<Config> {
    let mut config = Config::default();
    let mut gpu = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            gpu = line.eq_ignore_ascii_case("[gpu]");
            continue;
        }
        if !gpu || line.starts_with([';', '#']) {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.split([';', '#']).next().unwrap_or_default().trim();
        match key.trim() {
            "apply_gpu_optimisations" => config.enabled = value == "accept-responsibility",
            "gpu_device" => config.device = value.parse().context("GPU 编号无效")?,
            "nv_core_clock_mhz_offset" => {
                config.core_offset = value.parse().context("核心偏移无效")?
            }
            "nv_mem_clock_mhz_offset" => {
                config.memory_offset = value.parse().context("显存偏移无效")?
            }
            "nv_powermizer_mode" => {
                config.power_mode = value.parse().context("PowerMizer 模式无效")?
            }
            "amd_performance_level" => {
                config.amd_level = match value {
                    "auto" => AmdLevel::Auto,
                    "high" => AmdLevel::High,
                    "low" => AmdLevel::Low,
                    "profile_standard" => AmdLevel::ProfileStandard,
                    "profile_peak" => AmdLevel::ProfilePeak,
                    _ => bail!("AMD 性能等级不受支持"),
                }
            }
            _ => {}
        }
    }
    Ok(config)
}

impl Config {
    fn validate(&self, devices: &[Device]) -> Result<()> {
        if self.core_offset < -1 || self.memory_offset < -1 || !(-1..=2).contains(&self.power_mode)
        {
            bail!("频率偏移不能小于 -1, PowerMizer 模式范围为 0-2");
        }
        if self.enabled
            && !devices
                .iter()
                .any(|device| device.id == self.device && device.vendor == self.vendor)
        {
            bail!("所选 GPU 不存在或不支持 GameMode GPU 调优");
        }
        Ok(())
    }

    fn apply_to(&self, path: &Path, devices: &[Device]) -> Result<()> {
        self.validate(devices)?;
        let mut settings = Vec::new();
        if self.enabled {
            settings.push("apply_gpu_optimisations=accept-responsibility".to_string());
            settings.push(format!("gpu_device={}", self.device));
            match self.vendor {
                Vendor::Nvidia => settings.extend([
                    format!("nv_powermizer_mode={}", self.power_mode),
                    format!("nv_core_clock_mhz_offset={}", self.core_offset),
                    format!("nv_mem_clock_mhz_offset={}", self.memory_offset),
                ]),
                Vendor::Amd => settings.push(format!(
                    "amd_performance_level={}",
                    match self.amd_level {
                        AmdLevel::Auto => "auto",
                        AmdLevel::High => "high",
                        AmdLevel::Low => "low",
                        AmdLevel::ProfileStandard => "profile_standard",
                        AmdLevel::ProfilePeak => "profile_peak",
                    }
                )),
            }
        }
        let keys = [
            "apply_gpu_optimisations",
            "gpu_device",
            "nv_powermizer_mode",
            "nv_core_clock_mhz_offset",
            "nv_mem_clock_mhz_offset",
            "amd_performance_level",
        ];
        let values = keys
            .iter()
            .map(|key| {
                (
                    *key,
                    settings.iter().find_map(|line| {
                        let (name, value) = line.split_once('=')?;
                        (name == *key).then(|| value.to_string())
                    }),
                )
            })
            .collect();
        super::config::write(path, &[("gpu", values)])
    }
}

/// 仅由 pkexec 提升权限后的 CLI 调用, 目标路径不可由输入指定.
pub fn apply_system_json(input: impl Read) -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        bail!("修改系统 GameMode 配置需要管理员权限");
    }
    let config: Config = serde_json::from_reader(input.take(8192)).context("GPU 配置无效")?;
    config.apply_to(
        Path::new("/etc/gamemode.ini"),
        &devices_at(Path::new("/sys/class/drm")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn gpu_profile_preserves_other_settings_and_backs_up_the_original() {
        let dir = TempDir::new("gpu-config");
        let path = dir.join("gamemode.ini");
        let original = "[general]\nrenice=5\n[gpu]\n; original comment\napply_gpu_optimisations=0\nnv_per_profile_editable=1\nnv_core_clock_mhz_offset=20\n[custom]\nstart=echo hello\n";
        fs::write(&path, original).unwrap();
        let config = Config {
            enabled: true,
            device: 1,
            core_offset: 75,
            memory_offset: 200,
            power_mode: 1,
            ..Default::default()
        };
        let devices = [Device {
            id: 1,
            vendor: Vendor::Nvidia,
        }];
        config.apply_to(&path, &devices).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("renice=5")
                && text.contains("; original comment")
                && text.contains("nv_per_profile_editable=1")
                && text.contains("start=echo hello")
        );
        assert_eq!(text.matches("nv_core_clock_mhz_offset=").count(), 1);
        let loaded = parse(&text).unwrap();
        assert!(loaded.enabled);
        assert_eq!(
            (
                loaded.device,
                loaded.core_offset,
                loaded.memory_offset,
                loaded.power_mode
            ),
            (1, 75, 200, 1)
        );
        assert_eq!(
            fs::read_to_string(path.with_extension("ini.oddity-backup")).unwrap(),
            original
        );
        Config::default().apply_to(&path, &[]).unwrap();
        let disabled = fs::read_to_string(&path).unwrap();
        assert!(!disabled.contains("apply_gpu_optimisations="));
        assert!(!parse(&disabled).unwrap().enabled);
        assert!(!disabled.contains("nv_core_clock_mhz_offset="));
        assert_eq!(
            fs::read_to_string(path.with_extension("ini.oddity-backup")).unwrap(),
            original
        );
    }

    #[test]
    fn gpu_devices_and_amd_profiles_use_the_selected_drm_card() {
        let dir = TempDir::new("gpu-devices");
        dir.write("card2/device/vendor", "0x1002\n");
        dir.write("card0/device/vendor", "0x8086\n");
        dir.write("card1/device/vendor", "0x10de\n");
        dir.write("card2-DP-1/device/vendor", "0x1002\n");
        let devices = devices_at(dir.path());
        assert_eq!(
            devices.iter().map(|device| device.id).collect::<Vec<_>>(),
            [1, 2]
        );
        let path = dir.join("gamemode.ini");
        for (level, expected) in [
            (AmdLevel::Auto, "auto"),
            (AmdLevel::High, "high"),
            (AmdLevel::Low, "low"),
            (AmdLevel::ProfileStandard, "profile_standard"),
            (AmdLevel::ProfilePeak, "profile_peak"),
        ] {
            let config = Config {
                enabled: true,
                vendor: Vendor::Amd,
                device: 2,
                amd_level: level,
                ..Default::default()
            };
            config.apply_to(&path, &devices).unwrap();
            let text = fs::read_to_string(&path).unwrap();
            assert!(text.contains(&format!("amd_performance_level={expected}")));
            assert!(parse(&text).unwrap().enabled);
            assert!(!text.contains("nv_powermizer_mode="));
        }
    }

    #[test]
    fn invalid_profiles_cannot_change_the_configuration() {
        let dir = TempDir::new("gpu-invalid");
        let path = dir.write("gamemode.ini", "[gpu]\napply_gpu_optimisations=0\n");
        let devices = [Device {
            id: 0,
            vendor: Vendor::Nvidia,
        }];
        for config in [
            Config {
                enabled: true,
                device: 99,
                ..Default::default()
            },
            Config {
                core_offset: -2,
                ..Default::default()
            },
            Config {
                power_mode: 3,
                ..Default::default()
            },
        ] {
            assert!(config.apply_to(&path, &devices).is_err());
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[gpu]\napply_gpu_optimisations=0\n"
        );
        for text in [
            "gpu_device=no",
            "nv_core_clock_mhz_offset=no",
            "nv_mem_clock_mhz_offset=no",
            "nv_powermizer_mode=no",
            "amd_performance_level=manual",
        ] {
            assert!(parse(&format!("[gpu]\n{text}\n")).is_err());
        }
        let link = dir.join("linked.ini");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(Config::default().apply_to(&link, &[]).is_err());
        assert_eq!(Config::default().core_offset, -1);
        assert!(devices_at(&dir.join("missing")).is_empty());
        if unsafe { libc::geteuid() } != 0 {
            assert!(apply_system_json("{}".as_bytes()).is_err());
        }
    }
}
