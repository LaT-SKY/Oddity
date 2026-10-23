use std::path::{Path, PathBuf};

use crate::config::LauncherConfig;
use crate::distribution::Distribution;
use crate::manifest::{Manifest, ManifestFile};
use crate::{update, version, Error, Result};

#[derive(Debug, Clone)]
pub struct LauncherPlan {
    pub root: PathBuf,
    pub version: String,
    pub files: Vec<ManifestFile>,
    pub download_bytes: u64,
    pub zip_bases: Vec<String>,
}

impl LauncherPlan {
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

fn mirror_url(url: &str, distribution: Distribution) -> String {
    match distribution {
        Distribution::Cn => url.replace("yhcdn1.wmupd.com", "yhcdn2.wmupd.com"),
        Distribution::Global => url.replace("ntecdn1.perfectworld.com", "ntecdn2.perfectworld.com"),
    }
}

pub fn plan_launcher(
    distribution: Distribution,
    install_root: &Path,
    full: bool,
) -> Result<LauncherPlan> {
    let other = match distribution {
        Distribution::Cn => Distribution::Global,
        Distribution::Global => Distribution::Cn,
    };
    if install_root
        .join(other.launcher_dir())
        .join("Config/Config.ini")
        .is_file()
    {
        return Err(Error::Integrity(format!(
            "安装目录已有另一地区的游戏，请为国服和国际服选择不同目录: {}",
            install_root.display()
        )));
    }
    let root = install_root.join(distribution.launcher_dir());
    if let Ok(config) = LauncherConfig::load(&root) {
        let expected = match distribution {
            Distribution::Cn => 1289,
            Distribution::Global => 3000001,
        };
        if config.game_id != 0 && config.game_id != expected {
            return Err(Error::Integrity(format!(
                "安装目录中的游戏地区与当前选择不一致: {}",
                root.display()
            )));
        }
    }
    let mut last_error = None;
    let mut found = None;
    for url in distribution.version_urls() {
        match version::fetch(url) {
            Ok(version) => {
                let Some(manifest_url) = version.file_list_url else {
                    last_error = Some(Error::Parse(format!("Version.ini 缺少 FileListURL: {url}")));
                    continue;
                };
                let backup_url = mirror_url(&manifest_url, distribution);
                let manifest =
                    Manifest::from_url(&manifest_url).or_else(|_| Manifest::from_url(&backup_url));
                match manifest {
                    Ok(manifest) => {
                        found = Some((manifest, manifest_url, backup_url));
                        break;
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    let (manifest, manifest_url, backup_url) = found.ok_or_else(|| {
        last_error.unwrap_or_else(|| Error::Http("无法获取官方启动器清单".into()))
    })?;
    let report = update::check(&root, &manifest, !full)?;
    let files: Vec<_> = report.missing.into_iter().chain(report.changed).collect();
    for file in &files {
        if file.zip_size.is_none()
            || file.zip_checksum.as_ref().is_none_or(|hash| {
                hash.len() != 32 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err(Error::Integrity(format!(
                "官方清单缺少 ZIP 校验信息: {}",
                file.path
            )));
        }
    }
    let download_bytes = files
        .iter()
        .map(|file| file.zip_size.unwrap_or(file.size))
        .sum();
    let zip_bases = [manifest_url, backup_url]
        .into_iter()
        .filter_map(|url| url.rsplit_once('/').map(|(base, _)| base.to_string()))
        .collect();
    Ok(LauncherPlan {
        root,
        version: manifest.product_version.unwrap_or_default(),
        files,
        download_bytes,
        zip_bases,
    })
}
