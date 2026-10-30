//! 快照字段展开与差异计算.
use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::{distribution_label, launch_mode_label, session_label, Gamescope, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffStatus {
    Same,
    Changed,
    Unknown,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub key: String,
    pub section: String,
    pub label: String,
    pub baseline: Option<String>,
    pub current: Option<String>,
    pub status: DiffStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub rows: Vec<DiffRow>,
    pub changed: usize,
    pub unknown: usize,
    pub unchanged: usize,
}

#[derive(Debug, Clone)]
struct Row {
    key: String,
    section: &'static str,
    label: String,
    value: Option<String>,
    compare: Option<String>,
}

impl Row {
    fn new(
        section: &'static str,
        key: impl Into<String>,
        label: impl Into<String>,
        value: Option<String>,
    ) -> Self {
        let value = value.filter(|value| !value.is_empty());
        Self {
            key: key.into(),
            section,
            label: label.into(),
            compare: value.clone(),
            value,
        }
    }

    fn with_compare(
        section: &'static str,
        key: impl Into<String>,
        label: impl Into<String>,
        value: Option<String>,
        compare: Option<String>,
    ) -> Self {
        let value = value.filter(|value| !value.is_empty());
        Self {
            key: key.into(),
            section,
            label: label.into(),
            compare: compare.or_else(|| value.clone()),
            value,
        }
    }

    fn compare(&self) -> Option<&str> {
        self.compare.as_deref().or(self.value.as_deref())
    }
}

impl Snapshot {
    fn rows(&self) -> Vec<Row> {
        let mut rows = vec![
            Row::new(
                "启动器",
                "launcher.version",
                "Oddity 版本",
                Some(self.launcher.version.clone()),
            ),
            Row::new(
                "启动器",
                "launcher.package",
                "安装形式",
                Some(self.launcher.package.clone()),
            ),
            Row::new("系统", "system.os", "发行版", self.system.os.clone()),
            Row::new("系统", "system.kernel", "内核", self.system.kernel.clone()),
            Row::with_compare(
                "系统",
                "system.session",
                "会话类型",
                self.system
                    .session
                    .as_deref()
                    .map(session_label)
                    .map(str::to_string),
                self.system.session.clone(),
            ),
            Row::new(
                "系统",
                "system.desktop",
                "桌面环境",
                self.system.desktop.clone(),
            ),
        ];

        if self.graphics.gpus.is_empty() {
            rows.push(Row::new("图形", "graphics.gpu", "显卡", None));
        } else {
            for gpu in &self.graphics.gpus {
                let key = match &gpu.slot {
                    Some(slot) => format!("graphics.gpu.{}.{slot}", gpu.pci_id),
                    None => format!("graphics.gpu.{}", gpu.pci_id),
                };
                let value = gpu.driver.as_ref().map(|driver| match &gpu.driver_version {
                    Some(version) if !version.is_empty() => format!("{driver} {version}"),
                    _ => driver.to_string(),
                });
                rows.push(Row::new(
                    "图形",
                    unique_key(&rows, key),
                    format!("显卡 {}", gpu.pci_id),
                    value,
                ));
            }
        }
        let opengl = self.graphics.opengl.as_ref().map(|opengl| {
            let mut parts = Vec::new();
            if let Some(renderer) = &opengl.renderer {
                parts.push(renderer.clone());
            } else if let Some(vendor) = &opengl.vendor {
                parts.push(vendor.clone());
            }
            if let Some(version) = &opengl.version {
                parts.push(version.clone());
            }
            parts.join(" · ")
        });
        rows.push(Row::new("图形", "graphics.opengl", "OpenGL 驱动", opengl));

        rows.push(Row::with_compare(
            "游戏",
            "game.distribution",
            "游戏地区",
            Some(distribution_label(self.game.distribution).to_string()),
            Some(self.game.distribution.as_str().to_string()),
        ));
        let install_root = self
            .game
            .install_root
            .clone()
            .unwrap_or_else(|| "未安装".into());
        let game_version = self.game.version.clone().unwrap_or_else(|| "未记录".into());
        let launcher_version = self
            .game
            .launcher_version
            .clone()
            .unwrap_or_else(|| "未记录".into());
        rows.push(Row::new(
            "游戏",
            "game.install_root",
            "安装目录",
            Some(install_root),
        ));
        rows.push(Row::new(
            "游戏",
            "game.version",
            "游戏版本",
            Some(game_version),
        ));
        rows.push(Row::new(
            "游戏",
            "game.launcher_version",
            "启动器版本",
            Some(launcher_version),
        ));

        let environment = self
            .runtime
            .environment_name
            .clone()
            .unwrap_or_else(|| "未选择".into());
        let environment_id = self
            .runtime
            .environment_id
            .clone()
            .unwrap_or_else(|| "未选择".into());
        rows.push(Row::with_compare(
            "运行环境",
            "runtime.environment",
            "运行环境",
            Some(environment),
            Some(environment_id),
        ));
        let proton_identity = self
            .runtime
            .proton_identity
            .clone()
            .or_else(|| self.runtime.proton_name.clone())
            .unwrap_or_else(|| "未选择".into());
        let proton_name = match (&self.runtime.proton_name, self.runtime.proton_ok) {
            (Some(name), true) => Some(name.clone()),
            (Some(name), false) => Some(format!("{name}（不可用）")),
            (None, _) => Some("未选择".into()),
        };
        rows.push(Row::with_compare(
            "运行环境",
            "runtime.proton",
            "Proton",
            proton_name,
            Some(format!("{proton_identity}|{}", self.runtime.proton_ok)),
        ));
        let prefix = self
            .runtime
            .prefix
            .clone()
            .unwrap_or_else(|| "不可用".into());
        let prefix_identity = format!(
            "{}|{}",
            self.runtime.environment_id.as_deref().unwrap_or("未选择"),
            self.runtime.prefix.as_deref().unwrap_or("不可用")
        );
        rows.push(Row::with_compare(
            "运行环境",
            "runtime.prefix",
            "Wine prefix",
            Some(prefix),
            Some(prefix_identity),
        ));
        rows.push(Row::with_compare(
            "运行环境",
            "runtime.launch_mode",
            "启动方式",
            Some(launch_mode_label(&self.runtime.launch_mode).to_string()),
            Some(self.runtime.launch_mode.clone()),
        ));
        rows.push(Row::new(
            "运行环境",
            "runtime.gamescope",
            "Gamescope",
            Some(self.runtime.gamescope.summary()),
        ));
        let umu = self.runtime.umu.clone().unwrap_or_else(|| "未找到".into());
        rows.push(Row::with_compare(
            "运行环境",
            "runtime.umu",
            "umu-run",
            Some(umu.clone()),
            Some(umu),
        ));
        rows
    }

    pub fn gamescope_summary(&self) -> String {
        self.runtime.gamescope.summary()
    }
}

fn unique_key(rows: &[Row], key: String) -> String {
    if !rows.iter().any(|row| row.key == key) {
        return key;
    }
    let mut index = 2;
    loop {
        let candidate = format!("{key}#{index}");
        if !rows.iter().any(|row| row.key == candidate) {
            return candidate;
        }
        index += 1;
    }
}

impl Diff {
    pub fn between(baseline: &Snapshot, current: &Snapshot) -> Self {
        Self::from_rows(baseline.rows(), current.rows())
    }

    fn from_rows(baseline: Vec<Row>, current: Vec<Row>) -> Self {
        let current_by_key: HashMap<&str, &Row> =
            current.iter().map(|row| (row.key.as_str(), row)).collect();
        let baseline_keys: HashSet<&str> = baseline.iter().map(|row| row.key.as_str()).collect();
        let mut rows = Vec::new();
        for baseline_row in &baseline {
            match current_by_key.get(baseline_row.key.as_str()) {
                Some(current_row) => {
                    let status = match (&baseline_row.value, &current_row.value) {
                        (Some(_), Some(_)) => match (baseline_row.compare(), current_row.compare())
                        {
                            (Some(left), Some(right)) if left == right => DiffStatus::Same,
                            (Some(_), Some(_)) => DiffStatus::Changed,
                            _ => DiffStatus::Unknown,
                        },
                        _ => DiffStatus::Unknown,
                    };
                    rows.push(DiffRow {
                        key: baseline_row.key.clone(),
                        section: baseline_row.section.to_string(),
                        label: baseline_row.label.clone(),
                        baseline: baseline_row.value.clone(),
                        current: current_row.value.clone(),
                        status,
                    });
                }
                None => rows.push(DiffRow {
                    key: baseline_row.key.clone(),
                    section: baseline_row.section.to_string(),
                    label: baseline_row.label.clone(),
                    baseline: baseline_row.value.clone(),
                    current: None,
                    status: if baseline_row.value.is_some() {
                        DiffStatus::Removed
                    } else {
                        DiffStatus::Unknown
                    },
                }),
            }
        }
        for current_row in &current {
            if baseline_keys.contains(current_row.key.as_str()) {
                continue;
            }
            rows.push(DiffRow {
                key: current_row.key.clone(),
                section: current_row.section.to_string(),
                label: current_row.label.clone(),
                baseline: None,
                current: current_row.value.clone(),
                status: if current_row.value.is_some() {
                    DiffStatus::Added
                } else {
                    DiffStatus::Unknown
                },
            });
        }
        let changed = rows
            .iter()
            .filter(|row| {
                matches!(
                    row.status,
                    DiffStatus::Changed | DiffStatus::Added | DiffStatus::Removed
                )
            })
            .count();
        let unknown = rows
            .iter()
            .filter(|row| row.status == DiffStatus::Unknown)
            .count();
        let unchanged = rows
            .iter()
            .filter(|row| row.status == DiffStatus::Same)
            .count();
        Self {
            rows,
            changed,
            unknown,
            unchanged,
        }
    }
}

/// 供界面直接使用的 Gamescope 文本; 保持与快照行一致.
pub fn gamescope_label(gamescope: Gamescope) -> String {
    gamescope.summary()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::distribution::Distribution;
    use crate::snapshots::{Game, Gamescope, Gpu, Graphics, Launcher, Opengl, Runtime, System};

    fn snapshot() -> Snapshot {
        Snapshot {
            launcher: Launcher {
                version: "0.1.0".into(),
                package: "系统安装".into(),
            },
            system: System {
                os: Some("Arch Linux".into()),
                kernel: Some("6.17.1".into()),
                session: Some("wayland".into()),
                desktop: Some("KDE".into()),
            },
            graphics: Graphics {
                gpus: vec![Gpu {
                    pci_id: "10DE:2C58".into(),
                    slot: Some("0000:01:00.0".into()),
                    driver: Some("nvidia".into()),
                    driver_version: Some("615.78.08".into()),
                }],
                opengl: Some(Opengl {
                    vendor: Some("NVIDIA Corporation".into()),
                    renderer: Some("NVIDIA GeForce RTX 5060".into()),
                    version: Some("4.6.0 NVIDIA 615.78.08".into()),
                }),
            },
            game: Game {
                distribution: Distribution::Cn,
                install_root: Some("/game".into()),
                launcher_root: Some("/game/NTELauncher".into()),
                version: Some("1.0.7".into()),
                launcher_version: Some("1.0.7".into()),
            },
            runtime: Runtime {
                environment_id: Some("env-a".into()),
                environment_name: Some("稳定 A".into()),
                proton_name: Some("GE-Proton9-20".into()),
                proton_identity: Some("pin-a".into()),
                proton_ok: true,
                prefix: Some("/prefix".into()),
                prefix_ok: true,
                launch_mode: "background".into(),
                gamescope: Gamescope::default(),
                umu: Some("/usr/bin/umu-run".into()),
            },
        }
    }

    fn row<'a>(diff: &'a Diff, key: &str) -> &'a DiffRow {
        diff.rows.iter().find(|row| row.key == key).unwrap()
    }

    #[test]
    fn classifies_same_changed_and_unknown() {
        let baseline = snapshot();
        let mut current = snapshot();
        current.system.kernel = Some("6.18.1".into());
        current.system.session = None;
        let diff = Diff::between(&baseline, &current);
        assert_eq!(row(&diff, "system.os").status, DiffStatus::Same);
        assert_eq!(row(&diff, "system.kernel").status, DiffStatus::Changed);
        assert_eq!(row(&diff, "system.session").status, DiffStatus::Unknown);
        assert!(!row(&diff, "system.session").baseline.is_none());
        assert!(row(&diff, "system.session").current.is_none());
    }

    #[test]
    fn unknown_values_are_not_counted_as_changes() {
        let baseline = snapshot();
        let mut current = snapshot();
        current.graphics.opengl = None;
        let diff = Diff::between(&baseline, &current);
        assert_eq!(row(&diff, "graphics.opengl").status, DiffStatus::Unknown);
        assert_eq!(diff.unknown, 1);
        assert_eq!(
            diff.rows
                .iter()
                .filter(|row| row.status == DiffStatus::Changed)
                .count(),
            0
        );
    }

    #[test]
    fn identity_changes_are_visible_even_with_the_same_name() {
        let baseline = snapshot();
        let mut current = snapshot();
        current.runtime.proton_identity = Some("pin-b".into());
        let diff = Diff::between(&baseline, &current);
        let proton = row(&diff, "runtime.proton");
        assert_eq!(proton.status, DiffStatus::Changed);
        assert_eq!(proton.baseline, proton.current);
    }

    #[test]
    fn a_missing_runtime_is_reported_as_a_change() {
        let baseline = snapshot();
        let mut current = snapshot();
        current.runtime.environment_id = None;
        current.runtime.environment_name = None;
        current.runtime.proton_name = None;
        current.runtime.proton_identity = None;
        current.runtime.proton_ok = false;
        current.runtime.prefix = None;
        current.runtime.umu = None;
        let diff = Diff::between(&baseline, &current);
        for key in [
            "runtime.environment",
            "runtime.proton",
            "runtime.prefix",
            "runtime.umu",
        ] {
            assert_eq!(row(&diff, key).status, DiffStatus::Changed, "{key}");
        }
        assert!(row(&diff, "runtime.environment").current.as_deref() == Some("未选择"));
        assert!(row(&diff, "runtime.umu").current.as_deref() == Some("未找到"));
    }

    #[test]
    fn empty_gpu_placeholders_are_unknown_not_removed() {
        let mut baseline = snapshot();
        baseline.graphics.gpus.clear();
        let mut current = snapshot();
        current.graphics.gpus = vec![Gpu {
            pci_id: "1002:164E".into(),
            slot: Some("0000:06:00.0".into()),
            driver: Some("amdgpu".into()),
            driver_version: None,
        }];
        let diff = Diff::between(&baseline, &current);
        let placeholder = row(&diff, "graphics.gpu");
        assert_eq!(placeholder.status, DiffStatus::Unknown);
        assert!(placeholder.baseline.is_none());
        assert!(!diff
            .rows
            .iter()
            .any(|row| row.status == DiffStatus::Removed));
        assert_eq!(diff.changed, 1);
        assert!(diff
            .rows
            .iter()
            .any(|row| row.status == DiffStatus::Added && row.label.contains("1002")));
    }

    #[test]
    fn added_and_removed_gpu_rows_are_counted() {
        let baseline = snapshot();
        let mut current = snapshot();
        current.graphics.gpus = vec![Gpu {
            pci_id: "1002:164E".into(),
            slot: Some("0000:06:00.0".into()),
            driver: Some("amdgpu".into()),
            driver_version: None,
        }];
        let diff = Diff::between(&baseline, &current);
        assert!(diff
            .rows
            .iter()
            .any(|row| row.status == DiffStatus::Removed && row.label.contains("10DE")));
        assert!(diff
            .rows
            .iter()
            .any(|row| row.status == DiffStatus::Added && row.label.contains("1002")));
        assert_eq!(diff.changed, 2);
    }

    #[test]
    fn session_labels_are_display_only() {
        let baseline = snapshot();
        let mut current = snapshot();
        current.system.session = Some("x11".into());
        let diff = Diff::between(&baseline, &current);
        let session = row(&diff, "system.session");
        assert_eq!(session.baseline.as_deref(), Some("Wayland"));
        assert_eq!(session.current.as_deref(), Some("X11"));
        assert_eq!(session.status, DiffStatus::Changed);
    }

    #[test]
    fn gamescope_label_matches_summary() {
        let gamescope = Gamescope {
            enabled: true,
            width: 1280,
            height: 720,
            fps: 60,
        };
        assert_eq!(gamescope_label(gamescope), "开启 (1280x720, 60 FPS)");
    }
}
