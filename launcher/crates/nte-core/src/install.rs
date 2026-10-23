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
    plan_launcher_with_urls(
        distribution,
        install_root,
        full,
        &distribution.version_urls(),
    )
}

fn plan_launcher_with_urls(
    distribution: Distribution,
    install_root: &Path,
    full: bool,
    urls: &[&str],
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
    for url in urls {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{HttpServer, TempDir};

    fn publish(server: &HttpServer, zip_metadata: bool) -> String {
        let manifest_url = server.url("/launcher/AllFiles.xml");
        server.route(
            "/Version.ini",
            format!("[VERSION]\nVersion=1.0\nFileListURL={manifest_url}\n"),
        );
        let zip = if zip_metadata {
            " ZipChecksum=\"0123456789abcdef0123456789abcdef\" ZipSize=\"100\""
        } else {
            ""
        };
        server.route("/launcher/AllFiles.xml", format!(
            "<AllFiles><ProductVersion Version=\"1.0\"/><File Path=\"/Config/Config.ini\" Size=\"4\" Checksum=\"{}\"{zip}/></AllFiles>",
            update::md5_bytes(b"good")
        ));
        server.url("/Version.ini")
    }

    #[test]
    fn plans_both_distributions_and_detects_same_size_corruption() {
        let server = HttpServer::start();
        let url = publish(&server, true);
        for distribution in [Distribution::Cn, Distribution::Global] {
            let dir = TempDir::new("install-plan");
            let plan = plan_launcher_with_urls(distribution, dir.path(), false, &[&url]).unwrap();
            assert_eq!(plan.root, dir.join(distribution.launcher_dir()));
            assert_eq!(plan.file_count(), 1);
            assert_eq!(plan.download_bytes, 100);
            assert_eq!(plan.version, "1.0");
            assert_eq!(plan.zip_bases[0], server.url("/launcher"));
            dir.write(
                format!("{}/Config/Config.ini", distribution.launcher_dir()),
                b"evil",
            );
            assert_eq!(
                plan_launcher_with_urls(distribution, dir.path(), false, &[&url])
                    .unwrap()
                    .file_count(),
                0
            );
            assert_eq!(
                plan_launcher_with_urls(distribution, dir.path(), true, &[&url])
                    .unwrap()
                    .file_count(),
                1
            );
        }
    }

    #[test]
    fn rejects_region_collisions_before_contacting_the_cdn() {
        for (distribution, other, game_id) in [
            (Distribution::Cn, Distribution::Global, 3000001),
            (Distribution::Global, Distribution::Cn, 1289),
        ] {
            let dir = TempDir::new("install-region");
            dir.write(format!("{}/Config/Config.ini", other.launcher_dir()), b"");
            assert!(
                plan_launcher_with_urls(distribution, dir.path(), false, &[])
                    .unwrap_err()
                    .to_string()
                    .contains("另一地区")
            );
            std::fs::remove_dir_all(dir.join(other.launcher_dir())).unwrap();
            dir.write(
                format!("{}/Config/Config.ini", distribution.launcher_dir()),
                format!("[General]\nGameID={game_id}\n"),
            );
            assert!(
                plan_launcher_with_urls(distribution, dir.path(), false, &[])
                    .unwrap_err()
                    .to_string()
                    .contains("地区与当前选择不一致")
            );
        }
    }

    #[test]
    fn requires_zip_integrity_and_tries_the_next_source() {
        let server = HttpServer::start();
        let url = publish(&server, false);
        let dir = TempDir::new("install-metadata");
        assert!(
            plan_launcher_with_urls(Distribution::Cn, dir.path(), false, &[&url])
                .unwrap_err()
                .to_string()
                .contains("ZIP 校验信息")
        );
        publish(&server, true);
        let missing = server.url("/missing.ini");
        let no_manifest = server.url("/no-manifest.ini");
        server.route("/no-manifest.ini", b"[VERSION]\nVersion=1\n");
        assert_eq!(
            plan_launcher_with_urls(
                Distribution::Cn,
                dir.path(),
                false,
                &[&missing, &no_manifest, &url]
            )
            .unwrap()
            .file_count(),
            1
        );
        assert!(plan_launcher_with_urls(Distribution::Cn, dir.path(), false, &[&missing]).is_err());
    }

    #[test]
    fn mirrors_use_the_selected_region() {
        assert_eq!(
            mirror_url("https://yhcdn1.wmupd.com/file", Distribution::Cn),
            "https://yhcdn2.wmupd.com/file"
        );
        assert_eq!(
            mirror_url(
                "https://ntecdn1.perfectworld.com/file",
                Distribution::Global
            ),
            "https://ntecdn2.perfectworld.com/file"
        );
    }
}
