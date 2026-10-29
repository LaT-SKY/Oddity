use nte_core::playtime::{PlaytimeHistory, PlaytimeStore, MIN_SESSION_MS};
use std::fs;
use tempfile::tempdir;

#[test]
fn minimum_duration_is_inclusive_for_finish_and_recovery() {
    for duration in [MIN_SESSION_MS - 1, MIN_SESSION_MS, MIN_SESSION_MS + 1] {
        for recover in [false, true] {
            let dir = tempdir().unwrap();
            let path = dir.path().join("playtime.json");
            let mut store = PlaytimeStore::load(path.clone());
            store.start(1_000_000, "direct", "global").unwrap();
            if recover {
                store.touch(1_000_000 + duration).unwrap();
                store.recover().unwrap();
            } else {
                store.finish(1_000_000 + duration).unwrap();
            }
            let history = store.history();
            assert!(history.active.is_none());
            assert_eq!(
                history.sessions.len(),
                usize::from(duration >= MIN_SESSION_MS)
            );
            if let Some(session) = history.sessions.first() {
                assert_eq!(session.duration_ms(), duration);
                assert_eq!(session.mode, "direct");
                assert_eq!(session.distribution, "global");
            }
            assert_eq!(PlaytimeStore::load(path).history(), history);
        }
    }
}

#[test]
fn invalid_start_preserves_the_active_session_and_disk() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("playtime.json");
    let mut store = PlaytimeStore::load(path.clone());
    store.start(1_000_000, "official", "cn").unwrap();
    store.touch(1_120_000).unwrap();
    let before = store.history();
    let bytes = fs::read(&path).unwrap();
    for timestamp in [0, -1] {
        assert!(store.start(timestamp, "direct", "global").is_err());
        assert_eq!(store.history(), before);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn idle_touch_does_not_create_a_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("missing/playtime.json");
    let mut store = PlaytimeStore::load(path.clone());
    store.touch(1_000_000).unwrap();
    assert_eq!(store.history(), PlaytimeHistory::default());
    assert!(!path.exists());
}

#[test]
fn unchanged_or_earlier_heartbeat_does_not_write() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("playtime.json");
    let mut store = PlaytimeStore::load(path.clone());
    store.start(1_000_000, "background", "cn").unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    for now in [-1, 999_999, 1_000_000] {
        store.touch(now).unwrap();
        assert_eq!(store.history().active.unwrap().last_seen_at, 1_000_000);
    }
    assert!(path.is_dir());
}

#[test]
fn failed_mutations_keep_the_persisted_active_session_and_can_be_retried() {
    for operation in ["start", "touch", "finish", "recover"] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("playtime.json");
        let backup = dir.path().join("saved.json");
        let mut store = PlaytimeStore::load(path.clone());
        store.start(1_000_000, "background", "cn").unwrap();
        store.touch(1_120_000).unwrap();
        let before = store.history();
        fs::rename(&path, &backup).unwrap();
        fs::create_dir(&path).unwrap();
        let mutate = |store: &mut PlaytimeStore| match operation {
            "start" => store.start(2_000_000, "official", "global"),
            "touch" => store.touch(1_180_000),
            "finish" => store.finish(1_180_000),
            "recover" => store.recover(),
            _ => unreachable!(),
        };
        assert!(mutate(&mut store).is_err(), "{operation}");
        assert_eq!(store.history(), before, "{operation}");
        assert_eq!(PlaytimeStore::load(backup.clone()).history(), before);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        fs::remove_dir(&path).unwrap();
        fs::rename(&backup, &path).unwrap();
        mutate(&mut store).unwrap();
        assert_ne!(store.history(), before, "{operation}");
        assert_eq!(PlaytimeStore::load(path).history(), store.history());
    }
}

#[test]
fn starting_again_drops_a_short_stale_session_and_keeps_new_metadata() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("playtime.json");
    let mut store = PlaytimeStore::load(path.clone());
    store.start(1_000_000, "background", "cn").unwrap();
    store.touch(1_059_999).unwrap();
    store.start(2_000_000, "official", "global").unwrap();
    let history = store.history();
    assert!(history.sessions.is_empty());
    let active = history.active.as_ref().unwrap();
    assert_eq!(active.started_at, 2_000_000);
    assert_eq!(active.last_seen_at, 2_000_000);
    assert_eq!(active.mode, "official");
    assert_eq!(active.distribution, "global");
    assert_eq!(PlaytimeStore::load(path).history(), history);
}

#[test]
fn repeated_finish_and_recover_do_not_duplicate_sessions() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("playtime.json");
    let mut store = PlaytimeStore::load(path.clone());
    store.start(1_000_000, "background", "cn").unwrap();
    store.touch(1_120_000).unwrap();
    store.recover().unwrap();
    let before = fs::read(&path).unwrap();
    store.finish(2_000_000).unwrap();
    store.recover().unwrap();
    assert_eq!(store.history().sessions.len(), 1);
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn history_snapshots_cannot_mutate_the_store() {
    let dir = tempdir().unwrap();
    let mut store = PlaytimeStore::load(dir.path().join("playtime.json"));
    store.start(1_000_000, "background", "cn").unwrap();
    store.finish(1_120_000).unwrap();
    store.start(2_000_000, "official", "global").unwrap();
    let expected = store.history();
    let mut snapshot = store.history();
    snapshot.sessions.clear();
    snapshot.active.as_mut().unwrap().mode.clear();
    assert_eq!(store.history(), expected);
}

#[test]
fn loads_partial_data_with_defaults_without_creating_a_backup() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("playtime.json");
    fs::write(&path, b"{}").unwrap();
    assert_eq!(
        PlaytimeStore::load(path.clone()).history(),
        PlaytimeHistory::default()
    );
    assert_eq!(fs::read(path).unwrap(), b"{}");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn loads_sessions_in_time_order_and_removes_invalid_active_data() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("playtime.json");
    let data = serde_json::json!({
        "sessions": [
            {"startedAt": 2_000_000, "endedAt": 2_120_000, "mode": "direct", "distribution": "global"},
            {"startedAt": 1_000_000, "endedAt": 1_120_000, "mode": "official", "distribution": "cn"}
        ],
        "active": {"startedAt": -1, "lastSeenAt": 100, "mode": "background", "distribution": "cn"}
    });
    fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
    let history = PlaytimeStore::load(path).history();
    assert!(history.active.is_none());
    assert_eq!(
        history
            .sessions
            .iter()
            .map(|session| session.started_at)
            .collect::<Vec<_>>(),
        vec![1_000_000, 2_000_000]
    );
    assert_eq!(history.sessions[0].mode, "official");
    assert_eq!(history.sessions[1].distribution, "global");
}

#[test]
fn oversized_files_are_backed_up_before_new_records_are_saved() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("playtime.json");
    let file = fs::File::create(&path).unwrap();
    file.set_len(16 * 1024 * 1024 + 1).unwrap();
    drop(file);
    let mut store = PlaytimeStore::load(path.clone());
    assert_eq!(store.history(), PlaytimeHistory::default());
    assert!(!path.exists());
    let backup = fs::read_dir(dir.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("playtime.json.corrupt-"));
    assert_eq!(fs::metadata(&backup).unwrap().len(), 16 * 1024 * 1024 + 1);
    store.start(1_000_000, "background", "cn").unwrap();
    assert_eq!(PlaytimeStore::load(path).history(), store.history());
    assert!(backup.exists());
}
