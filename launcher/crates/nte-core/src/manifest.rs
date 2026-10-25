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

fn attr(tag: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    tag.attributes()
        .filter_map(std::result::Result::ok)
        .find(|attr| attr.key.as_ref() == name)
        .and_then(|attr| {
            quick_xml::escape::unescape(&attr.value)
                .ok()
                .map(|v| v.into_owned())
        })
}

impl Manifest {
    pub fn parse(xml: &str) -> Result<Self> {
        use quick_xml::events::Event;
        let mut m = Manifest::default();
        let mut reader = quick_xml::Reader::from_str(xml);
        let mut depth = 0usize;
        let mut root_seen = false;
        loop {
            let event = reader
                .read_event()
                .map_err(|error| Error::Parse(format!("启动器清单 XML 无效: {error}")))?;
            match &event {
                Event::Start(tag) | Event::Empty(tag) => {
                    if depth == 0 {
                        if root_seen {
                            return Err(Error::Parse("启动器清单包含多个根节点".into()));
                        }
                        root_seen = true;
                    }
                    for attribute in tag.attributes() {
                        let attribute = attribute.map_err(|error| {
                            Error::Parse(format!("启动器清单属性无效: {error}"))
                        })?;
                        quick_xml::escape::unescape(&attribute.value)
                            .map_err(|error| Error::Parse(error.to_string()))?;
                    }
                    if matches!(event, Event::Start(_)) {
                        depth += 1;
                    }
                }
                Event::End(_) => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| Error::Parse("启动器清单节点未配对".into()))?;
                }
                Event::Text(text) if depth == 0 && !text.trim_ascii().is_empty() => {
                    return Err(Error::Parse("启动器清单根节点外存在文本".into()));
                }
                _ => {}
            }
            match event {
                Event::Start(tag) | Event::Empty(tag) => match tag.name().as_ref() {
                    "Url" => m.base_url = attr(&tag, "BaseUrl"),
                    "ProductVersion" => m.product_version = attr(&tag, "Version"),
                    "BuildInfo" => m.build_time = attr(&tag, "Time"),
                    "File" => {
                        if let Some(path) = attr(&tag, "Path") {
                            m.files.push(ManifestFile {
                                path,
                                checksum: attr(&tag, "Checksum").unwrap_or_default(),
                                size: attr(&tag, "Size").and_then(|s| s.parse().ok()).unwrap_or(0),
                                zip_checksum: attr(&tag, "ZipChecksum"),
                                zip_size: attr(&tag, "ZipSize").and_then(|s| s.parse().ok()),
                            });
                        }
                    }
                    _ => {}
                },
                Event::Eof => {
                    if depth != 0 || !root_seen {
                        return Err(Error::Parse("启动器清单不完整".into()));
                    }
                    break;
                }
                _ => {}
            }
        }
        Ok(m)
    }

    pub fn from_url(url: &str) -> Result<Self> {
        let mut resp = ureq::get(url)
            .header("User-Agent", USER_AGENT)
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .build()
            .call()
            .map_err(|e| Error::Http(format!("GET {url}: {e}")))?;
        let mut body = String::new();
        use std::io::Read;
        resp.body_mut()
            .as_reader()
            .take(16 * 1024 * 1024 + 1)
            .read_to_string(&mut body)
            .map_err(|e| Error::Http(format!("read {url}: {e}")))?;
        if body.len() > 16 * 1024 * 1024 {
            return Err(Error::Parse("启动器清单超过读取上限".into()));
        }
        let m = Self::parse(&body)?;
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
    fn attributes_are_exact_and_independent_of_order() {
        let manifest = Manifest::parse(
            r#"<File ZipSize="99" ZipChecksum="ZIP" Size="4" Checksum="CONTENT" Path="/a&amp;b"/>"#,
        )
        .unwrap();
        let file = &manifest.files[0];
        assert_eq!(file.size, 4);
        assert_eq!(file.checksum, "CONTENT");
        assert_eq!(file.zip_size, Some(99));
        assert_eq!(file.zip_checksum.as_deref(), Some("ZIP"));
        assert_eq!(file.path, "/a&b");
    }

    #[test]
    fn parses_manifest_attributes() {
        let xml = r#"<AllFiles><Url BaseUrl="https://cdn.test/base/"/><ProductVersion Version="1.4.16"/><BuildInfo Time="2024-01-01T00:00:00"/><File Path="/a/b.dat" Checksum="ABC" Size="12" ZipChecksum="DEF" ZipSize="34"/><File Path="/c.dat" Size="bad"/><File Checksum="none"/></AllFiles>"#;
        let m = Manifest::parse(xml).unwrap();
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
    fn rejects_incomplete_or_malformed_documents() {
        for xml in [
            "",
            "<AllFiles><File Path=\"/a\"/>",
            "<AllFiles><File Path=\"/a\"/><File Path=\"/b\"",
            "<AllFiles></Wrong>",
            "<AllFiles/><AllFiles/>",
        ] {
            assert!(Manifest::parse(xml).is_err(), "{xml}");
        }
        let server = HttpServer::start();
        server.route("/cut.xml", "<AllFiles><File Path=\"/a\"/>");
        assert!(Manifest::from_url(&server.url("/cut.xml")).is_err());
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
