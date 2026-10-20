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
    let mut resp = ureq::get(&url)
        .header("User-Agent", USER_AGENT)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .call()
        .map_err(|e| Error::Http(format!("GET {url}: {e}")))?;
    let mut body = String::new();
    resp.body_mut()
        .as_reader()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::HttpServer;

    #[test]
    fn compares_dotted_versions_numerically() {
        assert_eq!(compare("1.10.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare("1.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare("1.0.0.1", "1.0"), Ordering::Greater);
        assert_eq!(compare("0.9", "1"), Ordering::Less);
        // 非数字段按 0 处理，和官方启动器的宽松解析保持一致
        assert_eq!(compare("1.x", "1.0"), Ordering::Equal);
        assert!(is_newer("1.0.0", "1.0.1"));
        assert!(!is_newer("1.0.1", "1.0.0"));
        assert!(!is_newer("1.0", "1.0"));
    }

    #[test]
    fn fetch_reads_version_ini_and_appends_tick() {
        let server = HttpServer::start();
        server.route(
            "/Version.ini",
            "[VERSION]\nVersion=1.4.16\nBuild=20240101\nFileListURL=https://cdn.test/AllFiles.xml\n\
             [UPDATEINFO]\nMode=full\nLevel=3\n",
        );
        let info = fetch(&server.url("/Version.ini")).expect("fetch version");
        assert_eq!(info.version, "1.4.16");
        assert_eq!(info.build, "20240101");
        assert_eq!(
            info.file_list_url.as_deref(),
            Some("https://cdn.test/AllFiles.xml")
        );
        assert_eq!(
            info.update_info,
            vec![
                ("Level".to_string(), "3".to_string()),
                ("Mode".to_string(), "full".to_string()),
            ]
        );
        let target = server
            .targets()
            .into_iter()
            .next()
            .expect("recorded request");
        assert!(target.starts_with("/Version.ini?t="), "{target}");
    }

    #[test]
    fn fetch_uses_ampersand_when_the_url_already_has_a_query() {
        let server = HttpServer::start();
        server.route("/Version.ini", "[VERSION]\nVersion=2.0\n");
        let info =
            fetch(&format!("{}?branch=ob", server.url("/Version.ini"))).expect("fetch version");
        assert_eq!(info.version, "2.0");
        assert_eq!(info.build, "");
        assert!(info.file_list_url.is_none());
        assert!(info.update_info.is_empty());
        let target = server
            .targets()
            .into_iter()
            .next()
            .expect("recorded request");
        assert!(target.starts_with("/Version.ini?branch=ob&t="), "{target}");
    }

    #[test]
    fn fetch_rejects_a_response_without_version() {
        let server = HttpServer::start();
        server.route("/Version.ini", "[UPDATEINFO]\nMode=full\n");
        let error = fetch(&server.url("/Version.ini")).expect_err("missing version");
        assert!(matches!(error, Error::Parse(_)), "{error:?}");
        assert!(error.to_string().contains("no [VERSION] Version"));
    }

    #[test]
    fn fetch_reports_http_failures() {
        let server = HttpServer::start();
        let error = fetch(&server.url("/missing.ini")).expect_err("404");
        assert!(matches!(error, Error::Http(_)), "{error:?}");
        assert!(error.to_string().contains("missing.ini"));
    }
}
