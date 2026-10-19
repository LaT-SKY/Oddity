use crate::{Error, Result, USER_AGENT};

#[derive(Debug, Clone, Default)]
pub struct ManifestFile {
    pub path: String,
    pub checksum: String,
    pub size: u64,
    pub zip_checksum: Option<String>,
    pub zip_size: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub base_url: Option<String>,
    pub product_version: Option<String>,
    pub build_time: Option<String>,
    pub files: Vec<ManifestFile>,
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

impl Manifest {
    pub fn parse(xml: &str) -> Self {
        let mut m = Manifest::default();
        if let Some(i) = xml.find("<Url ") {
            if let Some(end) = xml[i..].find('>') {
                let tag = &xml[i..i + end];
                m.base_url = attr(tag, "BaseUrl");
            }
        }
        if let Some(i) = xml.find("<ProductVersion") {
            if let Some(end) = xml[i..].find('>') {
                let tag = &xml[i..i + end];
                m.product_version = attr(tag, "Version");
            }
        }
        if let Some(i) = xml.find("<BuildInfo") {
            if let Some(end) = xml[i..].find('>') {
                let tag = &xml[i..i + end];
                m.build_time = attr(tag, "Time");
            }
        }
        let mut rest = xml;
        while let Some(i) = rest.find("<File ") {
            let tail = &rest[i..];
            let Some(end) = tail.find("/>") else { break };
            let tag = &tail[..end + 2];
            if let Some(path) = attr(tag, "Path") {
                m.files.push(ManifestFile {
                    path,
                    checksum: attr(tag, "Checksum").unwrap_or_default(),
                    size: attr(tag, "Size").and_then(|s| s.parse().ok()).unwrap_or(0),
                    zip_checksum: attr(tag, "ZipChecksum"),
                    zip_size: attr(tag, "ZipSize").and_then(|s| s.parse().ok()),
                });
            }
            rest = &tail[end + 2..];
        }
        m
    }

    pub fn from_url(url: &str) -> Result<Self> {
        let resp = ureq::get(url)
            .set("User-Agent", USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .call()
            .map_err(|e| Error::Http(format!("GET {url}: {e}")))?;
        let mut body = String::new();
        use std::io::Read;
        resp.into_reader()
            .take(16 * 1024 * 1024)
            .read_to_string(&mut body)
            .map_err(|e| Error::Http(format!("read {url}: {e}")))?;
        let m = Self::parse(&body);
        if m.files.is_empty() {
            return Err(Error::Parse(format!("no <File> entries at {url}")));
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::HttpServer;

    #[test]
    fn parses_manifest_attributes() {
        let xml = r#"<AllFiles><Url BaseUrl="https://cdn.test/base/"/><ProductVersion Version="1.4.16"/><BuildInfo Time="2024-01-01T00:00:00"/><File Path="/a/b.dat" Checksum="ABC" Size="12" ZipChecksum="DEF" ZipSize="34"/><File Path="/c.dat" Size="bad"/><File Checksum="none"/></AllFiles>"#;
        let m = Manifest::parse(xml);
        assert_eq!(m.base_url.as_deref(), Some("https://cdn.test/base/"));
        assert_eq!(m.product_version.as_deref(), Some("1.4.16"));
        assert_eq!(m.build_time.as_deref(), Some("2024-01-01T00:00:00"));
        assert_eq!(m.files.len(), 2);
        assert_eq!(m.files[0].path, "/a/b.dat");
        assert_eq!(m.files[0].checksum, "ABC");
        assert_eq!(m.files[0].size, 12);
        assert_eq!(m.files[0].zip_checksum.as_deref(), Some("DEF"));
        assert_eq!(m.files[0].zip_size, Some(34));
        // 没有 Path 的条目被忽略，非法 Size 退化为 0
        assert_eq!(m.files[1].path, "/c.dat");
        assert_eq!(m.files[1].size, 0);
        assert_eq!(m.files[1].checksum, "");
        assert!(m.files[1].zip_checksum.is_none());
        assert!(m.files[1].zip_size.is_none());
    }

    #[test]
    fn parses_an_empty_manifest() {
        let m = Manifest::parse("");
        assert!(m.base_url.is_none());
        assert!(m.product_version.is_none());
        assert!(m.build_time.is_none());
        assert!(m.files.is_empty());
    }

    #[test]
    fn parse_stops_at_an_unterminated_file_tag() {
        let m = Manifest::parse("<File Path=\"/a\" Size=\"1\"/><File Path=\"/b\"");
        assert_eq!(m.files.len(), 1);
        assert_eq!(m.files[0].path, "/a");
    }

    #[test]
    fn from_url_downloads_and_parses_a_manifest() {
        let server = HttpServer::start();
        server.route(
            "/AllFiles.xml",
            "<AllFiles><File Path=\"/a.dat\" Checksum=\"X\" Size=\"2\"/></AllFiles>",
        );
        let m = Manifest::from_url(&server.url("/AllFiles.xml")).expect("manifest");
        assert_eq!(m.files.len(), 1);
        assert_eq!(m.files[0].path, "/a.dat");
    }

    #[test]
    fn from_url_rejects_a_manifest_without_files() {
        let server = HttpServer::start();
        server.route("/AllFiles.xml", "<AllFiles/>");
        let error = Manifest::from_url(&server.url("/AllFiles.xml")).expect_err("no files");
        assert!(error.to_string().contains("no <File> entries"), "{error}");
    }

    #[test]
    fn from_url_reports_http_failures() {
        let server = HttpServer::start();
        let error = Manifest::from_url(&server.url("/nope.xml")).expect_err("404");
        assert!(matches!(error, Error::Http(_)), "{error:?}");
        assert!(error.to_string().contains("nope.xml"), "{error}");
    }
}
