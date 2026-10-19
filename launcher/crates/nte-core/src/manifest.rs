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
