use std::collections::BTreeSet;
use std::fs;
use std::time::UNIX_EPOCH;

use crate::game::check::{join_install_path, UpdateKind, UpdatePlan};
use crate::game::config::GameUpdateConfig;
use crate::game::manifest::{GameManifest, ResEntry};
use crate::game::pxml;
use crate::{Error, Result};

/// Refresh the official local ResList.xml ledger after a successful apply.
/// The ledger is a PatcherXML0 file; preserving it keeps the next check fast
/// and lets the shipped launcher/our CLI agree on file state.
pub fn update_after_apply(
    config: &GameUpdateConfig,
    plan: &UpdatePlan,
    successful_paths: &[String],
    bump_version: bool,
) -> Result<()> {
    if successful_paths.is_empty() {
        return Ok(());
    }
    let ledger_path = config.patcher_dir.join("ResList.xml");
    let relative = ledger_path
        .strip_prefix(&config.install_root)
        .map_err(|_| Error::Integrity("资源记录超出安装目录".into()))?;
    crate::paths::ensure_no_symlink(&config.install_root, relative)?;
    let local = crate::game::check::load_local_ledger(config)?;
    let path = config.patcher_dir.join("ResList.xml");
    if local.is_none() && path.is_file() {
        // 只在写回时保留损坏账本, 只读检查不改动安装文件。
        let stamp = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let backup = path.with_extension(format!("xml.corrupt-{stamp}"));
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(backup)?;
        std::io::copy(&mut fs::File::open(&path)?, &mut output)?;
        output.sync_all()?;
    }
    let mut manifest = local.unwrap_or_else(GameManifest::default);
    let completed: BTreeSet<_> = successful_paths.iter().map(String::as_str).collect();
    if bump_version
        && plan.unverified.is_empty()
        && plan
            .items
            .iter()
            .all(|item| completed.contains(item.path.as_str()))
    {
        manifest.version = plan.remote_version.clone();
        if !plan.remote_tag.is_empty() {
            manifest.tag = plan.remote_tag.clone();
        }
    }

    for path in successful_paths {
        let Some(item) = plan
            .items
            .iter()
            .map(|item| &item.kind)
            .chain(&plan.verified)
            .find(|kind| kind.path() == path)
        else {
            continue;
        };
        let target = join_install_path(&config.install_root, path);
        let timestamp = fs::metadata(&target)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let blocks = match item {
            UpdateKind::Res { res } => res.blocks.clone(),
            UpdateKind::PakEntry { .. } => Vec::new(),
        };
        let check = match item {
            UpdateKind::Res { res } => res.check,
            UpdateKind::PakEntry { entry, .. } => entry.check,
        };
        let fresh = ResEntry {
            filename: path.clone(),
            filesize: item.size(),
            md5: item.md5().into(),
            timestamp,
            check,
            blocks,
        };
        if !upsert(&mut manifest, fresh) {
            // A path can be new (for example an optional tag pack). Keep it in
            // the root ledger; the official parser accepts root Res entries.
            manifest.res.push(ResEntry {
                filename: path.clone(),
                filesize: item.size(),
                md5: item.md5().into(),
                timestamp,
                check,
                blocks: Vec::new(),
            });
        }
    }

    let xml = serialize(&manifest);
    let encoded = pxml::encode(xml.as_bytes(), &config.app_id)?;
    let path = config.patcher_dir.join("ResList.xml");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("xml.nte-new");
    fs::write(&tmp, encoded)?;
    fs::rename(&tmp, &path).map_err(Error::Io)?;
    Ok(())
}

fn upsert(manifest: &mut GameManifest, fresh: ResEntry) -> bool {
    for r in &mut manifest.res {
        if r.filename.eq_ignore_ascii_case(&fresh.filename) {
            *r = fresh.clone();
            return true;
        }
    }
    for base in &mut manifest.base_versions {
        for r in &mut base.res {
            if r.filename.eq_ignore_ascii_case(&fresh.filename) {
                *r = fresh.clone();
                return true;
            }
        }
    }
    false
}

fn serialize(m: &GameManifest) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\" ?>\n");
    out.push_str(&format!(
        "<ResList version=\"{}\" tag=\"{}\">\n",
        esc(&m.version),
        esc(&m.tag)
    ));
    for r in &m.res {
        write_res(&mut out, r, 1);
    }
    for base in &m.base_versions {
        out.push_str(&format!(
            "    <BaseVersion version=\"{}\" tag=\"{}\">\n        <ResList>\n",
            esc(&base.version),
            esc(&base.tag)
        ));
        for r in &base.res {
            write_res(&mut out, r, 3);
        }
        out.push_str("        </ResList>\n    </BaseVersion>\n");
    }
    out.push_str("</ResList>\n");
    out
}

fn write_res(out: &mut String, r: &ResEntry, depth: usize) {
    let pad = "    ".repeat(depth);
    out.push_str(&pad);
    out.push_str(&format!(
        "<Res filename=\"{}\" md5=\"{}\" filesize=\"{}\"",
        esc(&r.filename),
        esc(&r.md5),
        r.filesize
    ));
    if let Some(t) = r.timestamp {
        out.push_str(&format!(" timestamp=\"{t}\""));
    }
    if r.check {
        out.push_str(" check=\"1\"");
    }
    if !r.blocks.is_empty() {
        out.push_str(&format!(" blockSize=\"{}\">\n", r.blocks.len()));
        for b in &r.blocks {
            out.push_str(&format!(
                "{}    <Block index=\"{}\" start=\"{}\" size=\"{}\" md5=\"{}\"/>\n",
                pad,
                b.index,
                b.start,
                b.size,
                esc(&b.md5)
            ));
        }
        out.push_str(&format!("{pad}</Res>\n"));
    } else {
        out.push_str("/>\n");
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::check::{UpdateItem, UpdateReason};
    use crate::game::manifest::{Block, PakEntry};
    use crate::testing::TempDir;

    fn config_for(dir: &TempDir) -> GameUpdateConfig {
        GameUpdateConfig {
            launcher_root: dir.join("install/NTELauncher"),
            install_root: dir.join("install"),
            config_dir: dir.join("install/NTELauncher/ResFilesM/1289/PatcherConfig"),
            patcher_dir: dir.join("install/NTELauncher/UserData/Patcher/PatcherSDK"),
            branch: "publish_ob".to_string(),
            update_branch: "publish_Updata".to_string(),
            app_id: "1289".to_string(),
            cdns: vec!["https://cdn.test/clientRes".to_string()],
            rules: Vec::new(),
            check_dirs: Vec::new(),
            max_threads: 5,
        }
    }

    fn res_item(path: &str, md5: &str, size: u64, check: bool, blocks: Vec<Block>) -> UpdateItem {
        UpdateItem {
            path: path.to_string(),
            size,
            md5: md5.to_string(),
            reason: UpdateReason::Md5,
            kind: UpdateKind::Res {
                res: ResEntry {
                    filename: path.to_string(),
                    filesize: size,
                    md5: md5.to_string(),
                    timestamp: None,
                    check,
                    blocks,
                },
            },
            patches: Vec::new(),
            download_size: size,
        }
    }

    fn plan_with(items: Vec<UpdateItem>) -> UpdatePlan {
        UpdatePlan {
            remote_version: "1.4.16".to_string(),
            remote_tag: "baseTag".to_string(),
            remote_hash: "HASH".to_string(),
            items,
            ..Default::default()
        }
    }

    fn write_ledger(config: &GameUpdateConfig, xml: &str) {
        let encoded = pxml::encode(xml.as_bytes(), &config.app_id).expect("encode ledger");
        fs::create_dir_all(&config.patcher_dir).expect("create patcher dir");
        fs::write(config.patcher_dir.join("ResList.xml"), encoded).expect("write ledger");
    }

    fn read_ledger(config: &GameUpdateConfig) -> GameManifest {
        crate::game::check::load_local_ledger(config)
            .expect("load ledger")
            .expect("ledger written")
    }

    #[test]
    fn corrupt_ledger_is_rebuilt_after_full_verification_and_preserved() {
        use crate::game::check::{check_bundle, CheckOptions, RemoteBundle};
        use crate::game::manifest::RemoteConfig;
        let dir = TempDir::new("ledger-recovery");
        let config = config_for(&dir);
        dir.write("install/data/a.bin", b"good");
        fs::create_dir_all(&config.patcher_dir).unwrap();
        let path = config.patcher_dir.join("ResList.xml");
        fs::write(&path, b"broken-ledger").unwrap();
        let res = ResEntry {
            filename: "data/a.bin".into(),
            filesize: 4,
            md5: crate::update::md5_bytes(b"good"),
            ..Default::default()
        };
        let bundle = RemoteBundle {
            config: RemoteConfig {
                res_version: "1".into(),
                ..Default::default()
            },
            manifest: GameManifest {
                res: vec![res],
                ..Default::default()
            },
            diff: None,
        };
        let plan = check_bundle(
            &config,
            &bundle,
            CheckOptions {
                full: true,
                include_optional: false,
            },
        )
        .unwrap();
        assert_eq!(plan.verified.len(), 1);
        assert_eq!(fs::read(&path).unwrap(), b"broken-ledger");
        assert_eq!(fs::read_dir(&config.patcher_dir).unwrap().count(), 1);
        let report = crate::game::download::apply_plan(&config, &plan, &Default::default());
        assert!(report.ledger_error.is_none());
        assert_eq!(read_ledger(&config).version, "1");
        let backup = fs::read_dir(&config.patcher_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains(".corrupt-")
            })
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), b"broken-ledger");
    }

    #[test]
    fn empty_results_leave_the_ledger_alone() {
        let dir = TempDir::new("ledger-empty");
        let config = config_for(&dir);
        update_after_apply(
            &config,
            &plan_with(vec![res_item("data/a.bin", "aa", 1, false, Vec::new())]),
            &[],
            true,
        )
        .expect("update");
        assert!(!config.patcher_dir.join("ResList.xml").exists());
    }

    #[test]
    fn partial_apply_keeps_the_installed_version() {
        let dir = TempDir::new("ledger-partial");
        let config = config_for(&dir);
        write_ledger(
            &config,
            "<ResList version=\"1.4.15\" tag=\"old\"><Res filename=\"data/a.bin\" filesize=\"3\" md5=\"00\"/></ResList>",
        );
        dir.write("install/data/a.bin", b"new");
        let plan = plan_with(vec![
            res_item(
                "data/a.bin",
                &crate::update::md5_bytes(b"new"),
                3,
                false,
                Vec::new(),
            ),
            res_item("data/b.bin", "bb", 3, false, Vec::new()),
        ]);
        update_after_apply(&config, &plan, &["data/a.bin".into()], true).expect("update");

        let manifest = read_ledger(&config);
        assert_eq!(manifest.version, "1.4.15");
        assert_eq!(manifest.tag, "old");
        assert_eq!(manifest.res[0].filename, "data/a.bin");
    }

    #[test]
    fn updates_existing_entries_and_appends_new_ones() {
        let dir = TempDir::new("ledger-update");
        let config = config_for(&dir);
        dir.write("install/data/known.bin", b"known");
        dir.write("install/data/new.bin", b"new");
        write_ledger(
            &config,
            "<ResList version=\"1.0\" tag=\"old\"><Res filename=\"data/known.bin\" md5=\"00\" filesize=\"5\" timestamp=\"1\"/></ResList>",
        );

        let known_md5 = crate::update::md5_bytes(b"known");
        let block = Block {
            index: 0,
            start: 0,
            size: 5,
            md5: "bb".repeat(16),
        };
        let plan = plan_with(vec![
            res_item("data/known.bin", &known_md5, 5, true, vec![block]),
            res_item(
                "data/new.bin",
                &crate::update::md5_bytes(b"new"),
                3,
                false,
                Vec::new(),
            ),
        ]);
        update_after_apply(
            &config,
            &plan,
            &[
                "data/known.bin".to_string(),
                "data/new.bin".to_string(),
                // 计划里没有的路径会被忽略
                "data/unknown.bin".to_string(),
            ],
            true,
        )
        .expect("update");

        let manifest = read_ledger(&config);
        assert_eq!(manifest.version, "1.4.16");
        assert_eq!(manifest.tag, "baseTag");
        assert_eq!(manifest.res.len(), 2);

        let known = manifest
            .res
            .iter()
            .find(|r| r.filename == "data/known.bin")
            .expect("known entry");
        assert_eq!(known.md5, known_md5);
        assert_eq!(known.filesize, 5);
        assert!(known.timestamp.is_some());
        assert!(known.check);
        assert_eq!(known.blocks.len(), 1);
        assert_eq!(known.blocks[0].md5, "bb".repeat(16));

        let fresh = manifest
            .res
            .iter()
            .find(|r| r.filename == "data/new.bin")
            .expect("new entry");
        assert_eq!(fresh.filesize, 3);
        assert!(!fresh.check);
        assert!(fresh.blocks.is_empty());
    }

    #[test]
    fn updates_entries_inside_base_versions() {
        let dir = TempDir::new("ledger-base");
        let config = config_for(&dir);
        dir.write("install/data/base.bin", b"base");
        write_ledger(
            &config,
            "<ResList version=\"1.0\" tag=\"old\"><BaseVersion version=\"1.0\" tag=\"t\"><ResList><Res filename=\"data/base.bin\" md5=\"00\" filesize=\"4\"/></ResList></BaseVersion></ResList>",
        );

        let md5 = crate::update::md5_bytes(b"base");
        update_after_apply(
            &config,
            &plan_with(vec![res_item("data/base.bin", &md5, 4, false, Vec::new())]),
            &["data/base.bin".to_string()],
            true,
        )
        .expect("update");

        let manifest = read_ledger(&config);
        assert!(manifest.res.is_empty());
        assert_eq!(manifest.base_versions.len(), 1);
        assert_eq!(manifest.base_versions[0].res[0].filename, "data/base.bin");
        assert_eq!(manifest.base_versions[0].res[0].md5, md5);
    }

    #[test]
    fn pak_entries_are_recorded_without_blocks() {
        let dir = TempDir::new("ledger-pak");
        let config = config_for(&dir);
        let path = "Client/Content/Paks/pakchunk0.pak";
        let md5 = crate::update::md5_bytes(b"chunk");
        let item = UpdateItem {
            path: path.to_string(),
            size: 5,
            md5: md5.clone(),
            reason: UpdateReason::Missing,
            kind: UpdateKind::PakEntry {
                pak_md5: "aa".repeat(16),
                pak_size: 9,
                entry: PakEntry {
                    name: path.to_string(),
                    start: 0,
                    offset: 0,
                    size: 5,
                    md5: md5.clone(),
                    check: true,
                },
            },
            patches: Vec::new(),
            download_size: 5,
        };

        update_after_apply(&config, &plan_with(vec![item]), &[path.to_string()], true)
            .expect("update");
        let manifest = read_ledger(&config);
        assert_eq!(manifest.res.len(), 1);
        assert_eq!(manifest.res[0].filename, path);
        assert_eq!(manifest.res[0].md5, md5);
        assert!(manifest.res[0].check);
        assert!(manifest.res[0].blocks.is_empty());
        // 文件不存在，时间戳留空
        assert!(manifest.res[0].timestamp.is_none());
    }

    #[test]
    fn escapes_special_characters_in_names() {
        let dir = TempDir::new("ledger-escape");
        let config = config_for(&dir);
        let name = "data/a&b<c>\"d'.bin";
        update_after_apply(
            &config,
            &plan_with(vec![res_item(name, "aa", 1, false, Vec::new())]),
            &[name.to_string()],
            true,
        )
        .expect("update");
        let manifest = read_ledger(&config);
        assert_eq!(manifest.res[0].filename, name);
    }
}
