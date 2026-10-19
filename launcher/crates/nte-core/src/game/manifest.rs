use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct Block {
    pub index: u32,
    pub start: u64,
    pub size: u64,
    pub md5: String,
}

#[derive(Debug, Clone, Default)]
pub struct ResEntry {
    pub filename: String,
    pub filesize: u64,
    pub md5: String,
    pub timestamp: Option<u64>,
    pub check: bool,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Default)]
pub struct PakEntry {
    pub name: String,
    pub start: u64,
    pub offset: u64,
    pub size: u64,
    pub md5: String,
    pub check: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Pak {
    pub md5: String,
    pub filesize: u64,
    pub entries: Vec<PakEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct Package {
    pub paks: Vec<Pak>,
}

#[derive(Debug, Clone, Default)]
pub struct BaseVersion {
    pub version: String,
    pub tag: String,
    pub res: Vec<ResEntry>,
    pub packages: Vec<Package>,
}

#[derive(Debug, Clone, Default)]
pub struct GameManifest {
    pub version: String,
    pub tag: String,
    pub res: Vec<ResEntry>,
    pub packages: Vec<Package>,
    pub base_versions: Vec<BaseVersion>,
}

pub type LocalManifest = GameManifest;

#[derive(Debug, Clone, Default)]
pub struct PatchEntry {
    pub old_file: String,
    pub new_file: String,
    pub patch: String,
    pub version: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Default)]
pub struct DiffManifest {
    pub patches: Vec<PatchEntry>,
    pub sections: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BaseVersionInfo {
    pub section: String,
    pub version: String,
    pub tag: String,
    pub res_size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct RemoteConfig {
    pub res_version: String,
    pub hash: String,
    pub list_hash: String,
    pub diff_hash: String,
    pub res_size: u64,
    pub tag: String,
    pub max_threads: usize,
    pub base_versions: Vec<BaseVersionInfo>,
}

fn attrs(e: &BytesStart<'_>) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for a in e.attributes().with_checks(false).flatten() {
        let key = String::from_utf8_lossy(a.key.as_ref()).to_string();
        let value = a
            .unescape_value()
            .map(|v| v.into_owned())
            .unwrap_or_else(|_| String::from_utf8_lossy(&a.value).to_string());
        map.insert(key, value);
    }
    map
}

fn get(map: &BTreeMap<String, String>, key: &str) -> String {
    map.get(key).cloned().unwrap_or_default()
}

fn num(map: &BTreeMap<String, String>, key: &str) -> u64 {
    get(map, key).parse().unwrap_or(0)
}

impl ResEntry {
    fn from_attrs(map: &BTreeMap<String, String>) -> Self {
        Self {
            filename: get(map, "filename"),
            filesize: num(map, "filesize"),
            md5: get(map, "md5").to_ascii_lowercase(),
            timestamp: get(map, "timestamp").parse().ok(),
            check: get(map, "check") == "1",
            blocks: Vec::new(),
        }
    }
}

impl PakEntry {
    fn from_attrs(map: &BTreeMap<String, String>) -> Self {
        Self {
            name: get(map, "name"),
            start: num(map, "start"),
            offset: num(map, "offset"),
            size: num(map, "size"),
            md5: get(map, "md5").to_ascii_lowercase(),
            check: get(map, "check") == "1",
        }
    }
}

impl Pak {
    fn from_attrs(map: &BTreeMap<String, String>) -> Self {
        Self {
            md5: get(map, "md5").to_ascii_lowercase(),
            filesize: num(map, "filesize"),
            entries: Vec::new(),
        }
    }
}

impl PatchEntry {
    fn from_attrs(map: &BTreeMap<String, String>) -> Self {
        Self {
            old_file: get(map, "oldfile"),
            new_file: get(map, "newfile"),
            patch: get(map, "patch"),
            version: get(map, "v"),
            blocks: Vec::new(),
        }
    }
}

fn block_from_attrs(map: &BTreeMap<String, String>) -> Block {
    Block {
        index: num(map, "index") as u32,
        start: num(map, "start"),
        size: num(map, "size"),
        md5: get(map, "md5").to_ascii_lowercase(),
    }
}

pub fn parse_res_list(xml: &str) -> Result<GameManifest> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut m = GameManifest::default();
    let mut cur_res: Option<ResEntry> = None;
    let mut cur_pak: Option<Pak> = None;
    let mut cur_pkg: Option<Package> = None;
    let mut cur_base: Option<BaseVersion> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"ResList" => {
                    let a = attrs(&e);
                    if m.version.is_empty() {
                        m.version = get(&a, "version");
                    }
                    if m.tag.is_empty() {
                        m.tag = get(&a, "tag");
                    }
                }
                b"BaseVersion" => {
                    let a = attrs(&e);
                    cur_base = Some(BaseVersion {
                        version: get(&a, "version"),
                        tag: get(&a, "tag"),
                        res: Vec::new(),
                        packages: Vec::new(),
                    });
                }
                b"Res" => cur_res = Some(ResEntry::from_attrs(&attrs(&e))),
                b"Block" => {
                    let b = block_from_attrs(&attrs(&e));
                    if let Some(r) = cur_res.as_mut() {
                        r.blocks.push(b);
                    }
                }
                b"Package" => cur_pkg = Some(Package::default()),
                b"Pak" => cur_pak = Some(Pak::from_attrs(&attrs(&e))),
                b"Entry" => {
                    if let Some(p) = cur_pak.as_mut() {
                        p.entries.push(PakEntry::from_attrs(&attrs(&e)));
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"Res" => {
                    let r = ResEntry::from_attrs(&attrs(&e));
                    if let Some(base) = cur_base.as_mut() {
                        base.res.push(r);
                    } else {
                        m.res.push(r);
                    }
                }
                b"Block" => {
                    let b = block_from_attrs(&attrs(&e));
                    if let Some(r) = cur_res.as_mut() {
                        r.blocks.push(b);
                    }
                }
                b"Entry" => {
                    if let Some(p) = cur_pak.as_mut() {
                        p.entries.push(PakEntry::from_attrs(&attrs(&e)));
                    }
                }
                b"Pak" => {
                    if let Some(pkg) = cur_pkg.as_mut() {
                        pkg.paks.push(Pak::from_attrs(&attrs(&e)));
                    }
                }
                b"Package" => {
                    if let Some(base) = cur_base.as_mut() {
                        base.packages.push(Package::default());
                    } else {
                        m.packages.push(Package::default());
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"Res" => {
                    if let Some(r) = cur_res.take() {
                        if let Some(base) = cur_base.as_mut() {
                            base.res.push(r);
                        } else {
                            m.res.push(r);
                        }
                    }
                }
                b"Pak" => {
                    if let Some(p) = cur_pak.take() {
                        if let Some(pkg) = cur_pkg.as_mut() {
                            pkg.paks.push(p);
                        }
                    }
                }
                b"Package" => {
                    if let Some(pkg) = cur_pkg.take() {
                        if let Some(base) = cur_base.as_mut() {
                            base.packages.push(pkg);
                        } else {
                            m.packages.push(pkg);
                        }
                    }
                }
                b"BaseVersion" => {
                    if let Some(base) = cur_base.take() {
                        m.base_versions.push(base);
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Parse(format!("ResList XML: {e}"))),
            _ => {}
        }
    }

    if m.res.is_empty() && m.packages.is_empty() && m.base_versions.is_empty() {
        return Err(Error::Parse("ResList XML has no entries".into()));
    }
    Ok(m)
}

pub fn parse_diff_list(xml: &str) -> Result<DiffManifest> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = DiffManifest::default();
    let mut cur: Option<PatchEntry> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"Patch" => cur = Some(PatchEntry::from_attrs(&attrs(&e))),
                b"Block" => {
                    if let Some(p) = cur.as_mut() {
                        p.blocks.push(block_from_attrs(&attrs(&e)));
                    }
                }
                b"Section" => {
                    let a = attrs(&e);
                    let v = get(&a, "resversion");
                    if !v.is_empty() {
                        out.sections.push(v);
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"Patch" => out.patches.push(PatchEntry::from_attrs(&attrs(&e))),
                b"Block" => {
                    if let Some(p) = cur.as_mut() {
                        p.blocks.push(block_from_attrs(&attrs(&e)));
                    }
                }
                b"Section" => {
                    let v = get(&attrs(&e), "resversion");
                    if !v.is_empty() {
                        out.sections.push(v);
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"Patch" {
                    if let Some(p) = cur.take() {
                        out.patches.push(p);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Parse(format!("lastdiff XML: {e}"))),
            _ => {}
        }
    }
    Ok(out)
}

pub fn parse_remote_config(xml: &str) -> Result<RemoteConfig> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = RemoteConfig {
        max_threads: 5,
        ..Default::default()
    };
    let mut text_tag: Option<Vec<u8>> = None;
    let mut text = String::new();
    let mut in_base = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_vec();
                match name.as_slice() {
                    b"BaseVerson" | b"BaseVersion" => in_base = true,
                    b"Res" if in_base => {
                        let a = attrs(&e);
                        out.base_versions.push(BaseVersionInfo {
                            section: get(&a, "section"),
                            version: get(&a, "version"),
                            tag: get(&a, "Tag"),
                            res_size: num(&a, "ResSize"),
                        });
                    }
                    b"ResVersion" | b"Hash" | b"ResSize" | b"Tag" | b"listHash" | b"diffHash"
                    | b"maxThreadCnt" => {
                        text_tag = Some(name);
                        text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                if in_base && e.name().as_ref() == b"Res" {
                    let a = attrs(&e);
                    out.base_versions.push(BaseVersionInfo {
                        section: get(&a, "section"),
                        version: get(&a, "version"),
                        tag: get(&a, "Tag"),
                        res_size: num(&a, "ResSize"),
                    });
                }
            }
            Ok(Event::Text(e)) => {
                if text_tag.is_some() {
                    text.push_str(&e.unescape().unwrap_or_default());
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_vec();
                if name == b"BaseVerson" || name == b"BaseVersion" {
                    in_base = false;
                }
                if let Some(tag) = text_tag.as_ref() {
                    if tag.as_slice() == name.as_slice() {
                        let value = text.trim().to_string();
                        match name.as_slice() {
                            b"ResVersion" => out.res_version = value,
                            b"Hash" => out.hash = value,
                            b"ResSize" => out.res_size = value.parse().unwrap_or(0),
                            b"Tag" => out.tag = value,
                            b"listHash" => out.list_hash = value,
                            b"diffHash" => out.diff_hash = value,
                            b"maxThreadCnt" => out.max_threads = value.parse().unwrap_or(5).max(1),
                            _ => {}
                        }
                        text_tag = None;
                        text.clear();
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Parse(format!("config.xml: {e}"))),
            _ => {}
        }
    }

    if out.res_version.is_empty() {
        return Err(Error::Parse("config.xml has no ResVersion".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_res_list() {
        let xml = r#"<ResList version="1.0" tag="baseTag"><Res filename="a" filesize="1" md5="ABC"><Block index="0" start="0" size="1" md5="DEF"/></Res><Package><Pak md5="P" filesize="2"><Entry name="b" start="1" offset="2" size="3" md5="GHI"/></Pak></Package></ResList>"#;
        let m = parse_res_list(xml).unwrap();
        assert_eq!(m.version, "1.0");
        assert_eq!(m.res[0].md5, "abc");
        assert_eq!(m.res[0].blocks[0].md5, "def");
        assert_eq!(m.packages[0].paks[0].entries[0].offset, 2);
    }
}
