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
) -> Result<()> {
    if successful_paths.is_empty() {
        return Ok(());
    }
    let mut manifest =
        crate::game::check::load_local_ledger(config)?.unwrap_or_else(GameManifest::default);
    manifest.version = plan.remote_version.clone();
    if !plan.remote_tag.is_empty() {
        manifest.tag = plan.remote_tag.clone();
    }

    for path in successful_paths {
        let Some(item) = plan.items.iter().find(|i| &i.path == path) else {
            continue;
        };
        let target = join_install_path(&config.install_root, path);
        let timestamp = fs::metadata(&target)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let blocks = match &item.kind {
            UpdateKind::Res { res } => res.blocks.clone(),
            UpdateKind::PakEntry { .. } => Vec::new(),
        };
        let check = match &item.kind {
            UpdateKind::Res { res } => res.check,
            UpdateKind::PakEntry { entry, .. } => entry.check,
        };
        let fresh = ResEntry {
            filename: path.clone(),
            filesize: item.size,
            md5: item.md5.clone(),
            timestamp,
            check,
            blocks,
        };
        if !upsert(&mut manifest, fresh) {
            // A path can be new (for example an optional tag pack). Keep it in
            // the root ledger; the official parser accepts root Res entries.
            manifest.res.push(ResEntry {
                filename: path.clone(),
                filesize: item.size,
                md5: item.md5.clone(),
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
    fs::rename(&tmp, &path).map_err(|e| Error::Io(e))?;
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
