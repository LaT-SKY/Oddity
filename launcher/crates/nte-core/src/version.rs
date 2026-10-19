use std::cmp::Ordering;
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Ini;
use crate::{Error, Result, USER_AGENT};

#[derive(Debug, Clone, Default)]
pub struct VersionInfo {
    pub version: String,
    pub build: String,
    pub file_list_url: Option<String>,
    /// Any extra [UPDATEINFO] keys are preserved for diagnostics.
    pub update_info: Vec<(String, String)>,
}

fn now_tick() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn fetch(version_url: &str) -> Result<VersionInfo> {
    let sep = if version_url.contains('?') { '&' } else { '?' };
    let url = format!("{version_url}{sep}t={}", now_tick());
    let resp = ureq::get(&url)
        .set("User-Agent", USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| Error::Http(format!("GET {url}: {e}")))?;
    let mut body = String::new();
    resp.into_reader()
        .take(512 * 1024)
        .read_to_string(&mut body)
        .map_err(|e| Error::Http(format!("read {url}: {e}")))?;
    let ini = Ini::parse(&body);
    let v = ini.section("VERSION").cloned().unwrap_or_default();
    let version = v.get("Version").cloned().unwrap_or_default();
    if version.is_empty() {
        return Err(Error::Parse(format!(
            "Version.ini at {url} has no [VERSION] Version"
        )));
    }
    let update_info = ini
        .section("UPDATEINFO")
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    Ok(VersionInfo {
        version,
        build: v.get("Build").cloned().unwrap_or_default(),
        file_list_url: v.get("FileListURL").cloned(),
        update_info,
    })
}

/// Compare dotted numeric versions such as "1.0.7.0924".
pub fn compare(a: &str, b: &str) -> Ordering {
    let pa: Vec<u64> = a.split('.').map(|x| x.parse().unwrap_or(0)).collect();
    let pb: Vec<u64> = b.split('.').map(|x| x.parse().unwrap_or(0)).collect();
    for i in 0..pa.len().max(pb.len()) {
        let x = *pa.get(i).unwrap_or(&0);
        let y = *pb.get(i).unwrap_or(&0);
        match x.cmp(&y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

pub fn is_newer(local: &str, remote: &str) -> bool {
    compare(local, remote) == Ordering::Less
}
