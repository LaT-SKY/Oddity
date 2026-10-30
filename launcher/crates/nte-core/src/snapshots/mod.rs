//! 启动环境快照.
//!
//! 快照只保存启动参数和可读取的系统信息, 不读取 prefix 内部文件或游戏文件.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::distribution::Distribution;

pub mod diff;
pub mod probe;
pub mod store;

pub use diff::{Diff, DiffRow, DiffStatus};
pub use store::{Attempt, Failure, FailureReason, Reference, SnapshotStore, Status};

/// 自动基准成功判定可选的运行时长 (分钟).
pub const SUCCESS_AFTER_MINUTES: [u64; 4] = [1, 5, 10, 30];
/// 默认的成功判定运行时长 (分钟).
pub const DEFAULT_SUCCESS_MINUTES: u64 = 1;
/// 默认成功判定时长对应的毫秒值.
pub const SUCCESS_AFTER_MS: u64 = DEFAULT_SUCCESS_MINUTES * 60_000;
/// 自动记录保留的尝试数量; 自动基准不受裁剪影响.
pub const MAX_ATTEMPTS: usize = 30;
/// 参考快照标签的最大字符数.
pub const MAX_LABEL_CHARS: usize = 32;
pub(super) const MAX_TEXT_CHARS: usize = 512;
pub(super) const MAX_DETAIL_CHARS: usize = 200;

/// 将设置中的分钟数归一到受支持的选项.
pub fn normalize_success_minutes(minutes: u64) -> u64 {
    if SUCCESS_AFTER_MINUTES.contains(&minutes) {
        minutes
    } else {
        DEFAULT_SUCCESS_MINUTES
    }
}

/// 成功判定时长, 非法值按默认值处理.
pub fn success_after_ms(minutes: u64) -> u64 {
    normalize_success_minutes(minutes) * 60_000
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub launcher: Launcher,
    pub system: System,
    pub graphics: Graphics,
    pub game: Game,
    pub runtime: Runtime,
}

impl Snapshot {
    /// 写入启动日志的一行摘要.
    pub fn log_line(&self) -> String {
        let mut parts = vec![format!("oddity={}", self.launcher.version)];
        if let Some(session) = &self.system.session {
            parts.push(match &self.system.desktop {
                Some(desktop) if !desktop.is_empty() => {
                    format!("session={session}/{desktop}")
                }
                _ => format!("session={session}"),
            });
        }
        let graphics = self.graphics.summary();
        if !graphics.is_empty() {
            parts.push(format!("graphics={graphics}"));
        }
        if let Some(version) = &self.game.version {
            parts.push(format!("game={version}"));
        }
        parts.join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Launcher {
    pub version: String,
    pub package: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct System {
    pub os: Option<String>,
    pub kernel: Option<String>,
    pub session: Option<String>,
    pub desktop: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graphics {
    pub gpus: Vec<Gpu>,
    pub opengl: Option<Opengl>,
}

impl Graphics {
    fn summary(&self) -> String {
        if let Some(opengl) = &self.opengl {
            let parts: Vec<&str> = [opengl.renderer.as_deref(), opengl.version.as_deref()]
                .into_iter()
                .flatten()
                .filter(|part| !part.is_empty())
                .collect();
            if !parts.is_empty() {
                return parts.join(" ");
            }
        }
        self.gpus
            .iter()
            .filter_map(|gpu| {
                let driver = gpu.driver.as_deref()?;
                Some(match &gpu.driver_version {
                    Some(version) if !version.is_empty() => format!("{driver} {version}"),
                    _ => driver.to_string(),
                })
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub pci_id: String,
    pub slot: Option<String>,
    pub driver: Option<String>,
    pub driver_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opengl {
    pub vendor: Option<String>,
    pub renderer: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub distribution: Distribution,
    pub install_root: Option<String>,
    pub launcher_root: Option<String>,
    pub version: Option<String>,
    pub launcher_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub environment_id: Option<String>,
    pub environment_name: Option<String>,
    pub proton_name: Option<String>,
    pub proton_identity: Option<String>,
    pub proton_ok: bool,
    pub prefix: Option<String>,
    pub prefix_ok: bool,
    pub launch_mode: String,
    pub gamescope: Gamescope,
    pub umu: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gamescope {
    pub enabled: bool,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Gamescope {
    pub fn summary(&self) -> String {
        if !self.enabled {
            return "关闭".into();
        }
        let resolution = if self.width > 0 && self.height > 0 {
            format!("{}x{}", self.width, self.height)
        } else {
            "默认分辨率".into()
        };
        let fps = if self.fps > 0 {
            format!("{} FPS", self.fps)
        } else {
            "不限帧".into()
        };
        format!("开启 ({resolution}, {fps})")
    }
}

/// 从启动流程或设置中收集到的快照输入; 系统与图形信息由本模块补齐.
#[derive(Debug, Clone)]
pub struct Inputs {
    pub distribution: Distribution,
    pub install_root: Option<PathBuf>,
    pub launcher_root: Option<PathBuf>,
    pub game_version: Option<String>,
    pub launcher_version: Option<String>,
    pub environment_id: Option<String>,
    pub environment_name: Option<String>,
    pub proton_name: Option<String>,
    pub proton_identity: Option<String>,
    pub proton_ok: bool,
    pub prefix: Option<PathBuf>,
    pub prefix_ok: bool,
    pub launch_mode: String,
    pub gamescope: Gamescope,
    pub umu: Option<PathBuf>,
}

pub fn capture(inputs: Inputs) -> Snapshot {
    Snapshot {
        launcher: Launcher {
            version: env!("CARGO_PKG_VERSION").to_string(),
            package: probe::package(),
        },
        system: probe::system(),
        graphics: probe::graphics(),
        game: Game {
            distribution: inputs.distribution,
            install_root: path_text(inputs.install_root),
            launcher_root: path_text(inputs.launcher_root),
            version: text(inputs.game_version),
            launcher_version: text(inputs.launcher_version),
        },
        runtime: Runtime {
            environment_id: text(inputs.environment_id),
            environment_name: text(inputs.environment_name),
            proton_name: text(inputs.proton_name),
            proton_identity: text(inputs.proton_identity),
            proton_ok: inputs.proton_ok,
            prefix: path_text(inputs.prefix),
            prefix_ok: inputs.prefix_ok,
            launch_mode: inputs.launch_mode,
            gamescope: inputs.gamescope,
            umu: path_text(inputs.umu),
        },
    }
}

/// 去掉空值并限制字段长度, 避免损坏文件或异常输出撑大界面.
pub(super) fn text(value: Option<String>) -> Option<String> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(MAX_TEXT_CHARS).collect())
}

fn path_text(value: Option<PathBuf>) -> Option<String> {
    value
        .map(|value| value.display().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(MAX_TEXT_CHARS).collect())
}

pub fn distribution_label(distribution: Distribution) -> &'static str {
    match distribution {
        Distribution::Cn => "国服",
        Distribution::Global => "国际服",
    }
}

pub fn session_label(session: &str) -> &str {
    match session {
        "wayland" => "Wayland",
        "x11" => "X11",
        "tty" => "终端",
        other => other,
    }
}

pub fn launch_mode_label(mode: &str) -> &str {
    match mode {
        "direct" => "直接启动",
        "background" => "后台启动",
        "official" => "前台启动",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot {
            launcher: Launcher {
                version: "0.1.0".into(),
                package: "系统安装".into(),
            },
            system: System {
                os: Some("Test Linux".into()),
                kernel: Some("6.0.0".into()),
                session: Some("wayland".into()),
                desktop: Some("KDE".into()),
            },
            graphics: Graphics {
                gpus: vec![Gpu {
                    pci_id: "10DE:2684".into(),
                    slot: Some("0000:01:00.0".into()),
                    driver: Some("nvidia".into()),
                    driver_version: Some("550.54".into()),
                }],
                opengl: Some(Opengl {
                    vendor: Some("NVIDIA Corporation".into()),
                    renderer: Some("NVIDIA GeForce RTX 4090".into()),
                    version: Some("4.6.0 NVIDIA 550.54".into()),
                }),
            },
            game: Game {
                distribution: Distribution::Cn,
                install_root: Some("/game".into()),
                launcher_root: Some("/game/NTELauncher".into()),
                version: Some("1.0.7".into()),
                launcher_version: Some("1.4.16".into()),
            },
            runtime: Runtime {
                environment_id: Some("environment-a".into()),
                environment_name: Some("稳定 A".into()),
                proton_name: Some("GE-Proton9-20".into()),
                proton_identity: Some("pin-a".into()),
                proton_ok: true,
                prefix: Some("/prefix".into()),
                prefix_ok: true,
                launch_mode: "direct".into(),
                gamescope: Gamescope::default(),
                umu: Some("/usr/bin/umu-run".into()),
            },
        }
    }

    #[test]
    fn normalizes_success_minutes_to_supported_options() {
        assert_eq!(SUCCESS_AFTER_MINUTES, [1, 5, 10, 30]);
        assert_eq!(normalize_success_minutes(0), 1);
        assert_eq!(normalize_success_minutes(2), 1);
        assert_eq!(normalize_success_minutes(5), 5);
        assert_eq!(normalize_success_minutes(30), 30);
        assert_eq!(success_after_ms(5), 300_000);
    }

    #[test]
    fn log_line_contains_session_graphics_and_game() {
        let mut snapshot = snapshot();
        let line = snapshot.log_line();
        assert!(line.contains("oddity=0.1.0"));
        assert!(line.contains("session=wayland/KDE"));
        assert!(line.contains("graphics=NVIDIA GeForce RTX 4090 4.6.0 NVIDIA 550.54"));
        assert!(line.contains("game=1.0.7"));

        snapshot.system.desktop = None;
        assert!(snapshot.log_line().contains("session=wayland"));

        snapshot.system.session = None;
        snapshot.graphics.opengl = None;
        snapshot.graphics.gpus.clear();
        snapshot.game.version = None;
        let line = snapshot.log_line();
        assert!(!line.contains("session="));
        assert!(!line.contains("graphics="));
        assert!(!line.contains("game="));
    }

    #[test]
    fn graphics_summary_falls_back_to_gpu_driver() {
        let mut snapshot = snapshot();
        snapshot.graphics.opengl = None;
        assert_eq!(snapshot.graphics.summary(), "nvidia 550.54");
        snapshot.graphics.gpus[0].driver_version = None;
        assert_eq!(snapshot.graphics.summary(), "nvidia");
        snapshot.graphics.gpus.clear();
        assert_eq!(snapshot.graphics.summary(), "");
    }

    #[test]
    fn gamescope_and_labels_cover_fallbacks() {
        assert_eq!(Gamescope::default().summary(), "关闭");
        assert_eq!(
            Gamescope {
                enabled: true,
                ..Default::default()
            }
            .summary(),
            "开启 (默认分辨率, 不限帧)"
        );
        assert_eq!(
            Gamescope {
                enabled: true,
                width: 1280,
                height: 720,
                fps: 60,
            }
            .summary(),
            "开启 (1280x720, 60 FPS)"
        );
        assert_eq!(distribution_label(Distribution::Cn), "国服");
        assert_eq!(distribution_label(Distribution::Global), "国际服");
        assert_eq!(session_label("wayland"), "Wayland");
        assert_eq!(session_label("x11"), "X11");
        assert_eq!(session_label("tty"), "终端");
        assert_eq!(session_label("other"), "other");
        assert_eq!(launch_mode_label("direct"), "直接启动");
        assert_eq!(launch_mode_label("background"), "后台启动");
        assert_eq!(launch_mode_label("official"), "前台启动");
        assert_eq!(launch_mode_label("other"), "other");
    }
}
