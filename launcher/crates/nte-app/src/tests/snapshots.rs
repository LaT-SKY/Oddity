use super::*;
use crate::snapshot_commands::Snapshots;
use crate::*;
use nte_core::distribution::Distribution;
use nte_core::settings::Settings;
use nte_core::snapshots::{
    Failure, FailureReason, Game, Gamescope, Gpu, Graphics, Launcher, Opengl, Runtime, Snapshot,
    SnapshotStore, Status, System, SUCCESS_AFTER_MS,
};

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
fn snapshot_commands_compare_baseline_reference_and_clear_history() {
    let dir = TempDir::new("snapshot-commands");
    let mut env = EnvGuard::new();
    env.set("HOME", dir.join("home"));
    env.set("XDG_DATA_HOME", dir.join("data"));
    std::fs::create_dir_all(dir.join("home")).unwrap();

    let current = snapshot();
    let now = now_millis();
    let data = data_dir();
    {
        let mut store = SnapshotStore::load(data.join("environment-snapshots.json"));
        let started = now.saturating_sub(SUCCESS_AFTER_MS);
        let id = store.begin(current.clone(), started).unwrap();
        store.mark_running(&id, started).unwrap();
        store
            .mark_success(&id, now, Some("1.0.7".into()), SUCCESS_AFTER_MS)
            .unwrap();
    }

    let snapshots = Snapshots::load(&data);
    let overview = snapshots.overview().unwrap();
    assert_eq!(overview.baseline.as_ref().unwrap().status, Status::Success);
    assert_eq!(overview.latest.as_ref().unwrap().status, Status::Success);

    let baseline = snapshots.compare("baseline", current.clone()).unwrap();
    assert_eq!(baseline.target, "baseline");
    assert_eq!(baseline.target_label, "上次成功启动");
    assert_eq!(baseline.changed, 0);
    assert!(baseline.rows.iter().any(|row| row.key == "runtime.proton"));
    assert!(snapshots.compare("missing", current.clone()).is_err());

    let id = snapshots.begin(current.clone()).unwrap();
    snapshots.mark_running(&id, now);
    assert!(!snapshots.mark_success(&id, now, None, SUCCESS_AFTER_MS));
    snapshots.finish(&id, None);
    let failed = snapshots.overview().unwrap().latest.unwrap();
    assert_eq!(failed.status, Status::Failed);
    assert_eq!(
        failed.failure.as_ref().map(|failure| failure.reason),
        Some(FailureReason::EarlyExit)
    );

    snapshots.mark_running("missing", now);
    snapshots.finish("missing", None);
    snapshots.fail("missing", Failure::spawn("missing"));
    assert!(snapshots.compare("baseline", current.clone()).is_ok());

    let reference = snapshots
        .save_reference(&current, Some("参考".into()), None)
        .unwrap();
    assert_eq!(reference.label, "参考");
    let comparison = snapshots.compare("reference", current.clone()).unwrap();
    assert_eq!(comparison.target, "reference");
    assert_eq!(comparison.target_label, "参考快照");
    assert_eq!(comparison.current_environment.as_deref(), Some("稳定 A"));
    assert_eq!(comparison.changed, 0);

    let attempt_id = snapshots.begin(current.clone()).unwrap();
    let historical = snapshots
        .save_reference(&current, Some("历史".into()), Some(attempt_id.clone()))
        .unwrap();
    assert_eq!(historical.label, "历史");
    assert!(snapshots
        .save_reference(&current, None, Some("missing".into()))
        .is_err());

    snapshots.fail(&attempt_id, Failure::spawn("boom"));
    assert_eq!(
        snapshots
            .overview()
            .unwrap()
            .latest
            .unwrap()
            .failure
            .unwrap()
            .reason,
        FailureReason::Spawn
    );

    snapshots.clear_history().unwrap();
    let cleared = snapshots.overview().unwrap();
    assert_eq!(cleared.attempts, 0);
    assert!(cleared.baseline.is_none());
    assert!(cleared.latest.is_none());
    assert_eq!(cleared.reference.as_ref().unwrap().label, "历史");
    assert!(snapshots.compare("baseline", current.clone()).is_err());
    assert!(snapshots.compare("reference", current.clone()).is_ok());

    snapshots.delete_reference().unwrap();
    assert!(snapshots.overview().unwrap().reference.is_none());
    snapshots.delete_reference().unwrap();
    snapshots.clear_history().unwrap();

    let empty = snapshot_commands::current_inputs(&Settings::default());
    assert!(empty.environment_id.is_none());
    assert!(empty.environment_name.is_none());
    assert!(empty.prefix.is_none());
    assert!(!empty.proton_ok);
}
