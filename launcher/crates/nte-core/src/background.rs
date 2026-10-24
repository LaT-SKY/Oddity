//! 读取官方启动器自带的背景素材。
//!
//! 素材只在本地读取, 不复制也不随 Oddity 分发。

use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::config::LauncherConfig;

/// 官方启动器提供的本地背景素材。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherBackground {
    pub video: Option<PathBuf>,
    pub image: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
struct BackgroundManifest {
    #[serde(default)]
    imgs: Vec<BackgroundImage>,
    #[serde(default)]
    video: Option<String>,
    #[serde(default, rename = "noVideoBg")]
    no_video_bg: Option<String>,
    #[serde(default)]
    first: i64,
}

#[derive(Debug, Deserialize)]
struct BackgroundImage {
    file: String,
}

/// 在官方启动器根目录中查找视频和静态壁纸。
pub fn resolve(launcher_root: &Path) -> Option<LauncherBackground> {
    let res_root = launcher_root.join("ResFilesM");
    let game_id = LauncherConfig::load(launcher_root)
        .ok()
        .map(|config| config.game_id)
        .filter(|id| *id > 0);

    let preferred = game_id.map(|id| res_root.join(id.to_string()).join("bgimgs"));
    if let Some(dir) = &preferred {
        if let Some(background) = read_background(dir) {
            return Some(background);
        }
    }

    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&res_root)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join("bgimgs"))
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .filter(|dir| preferred.as_ref() != Some(dir))
        .find_map(|dir| read_background(&dir))
}

fn read_background(dir: &Path) -> Option<LauncherBackground> {
    if !dir.is_dir() {
        return None;
    }
    let manifest = std::fs::read_to_string(dir.join("config.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<BackgroundManifest>(&text).ok())
        .unwrap_or_default();
    let video = resolve_video(dir, &manifest);
    let image = resolve_image(dir, &manifest);
    (video.is_some() || image.is_some()).then_some(LauncherBackground { video, image })
}

fn resolve_video(dir: &Path, manifest: &BackgroundManifest) -> Option<PathBuf> {
    manifest
        .video
        .as_deref()
        .and_then(|name| asset_path(dir, name))
        .or_else(|| asset_path(dir, "bg.mp4"))
}

fn resolve_image(dir: &Path, manifest: &BackgroundManifest) -> Option<PathBuf> {
    let index = if manifest.first >= 0 && (manifest.first as usize) < manifest.imgs.len() {
        manifest.first as usize
    } else {
        0
    };
    manifest
        .imgs
        .get(index)
        .or_else(|| manifest.imgs.first())
        .and_then(|image| asset_path(dir, &image.file))
        .or_else(|| {
            manifest
                .no_video_bg
                .as_deref()
                .and_then(|name| asset_path(dir, name))
        })
        .or_else(|| asset_path(dir, "bg_0.png"))
}

/// 只接受背景目录下的单层文件名, 配置无法把路径引到目录外。
fn asset_path(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return None;
    }
    let path = dir.join(name);
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn launcher(dir: &TempDir, game_id: &str) -> PathBuf {
        dir.write(
            "launcher/Config/Config.ini",
            format!("[General]\nGameID={game_id}\n"),
        );
        dir.join("launcher")
    }

    fn background_dir(dir: &TempDir, game_id: &str) -> PathBuf {
        dir.join(format!("launcher/ResFilesM/{game_id}/bgimgs"))
    }

    #[test]
    fn reads_video_and_selected_image_from_manifest() {
        let dir = TempDir::new("background-manifest");
        let root = launcher(&dir, "1289");
        let bgimgs = background_dir(&dir, "1289");
        dir.write("launcher/ResFilesM/1289/bgimgs/bg.mp4", b"video");
        dir.write("launcher/ResFilesM/1289/bgimgs/bg_0.png", b"first");
        dir.write("launcher/ResFilesM/1289/bgimgs/bg_1.png", b"second");
        dir.write(
            "launcher/ResFilesM/1289/bgimgs/config.json",
            r#"{"video":"bg.mp4","imgs":[{"file":"bg_0.png"},{"file":"bg_1.png"}],"first":1}"#,
        );

        let background = resolve(&root).expect("find background");
        assert_eq!(background.video, Some(bgimgs.join("bg.mp4")));
        assert_eq!(background.image, Some(bgimgs.join("bg_1.png")));
    }

    #[test]
    fn ignores_manifest_paths_outside_the_background_dir() {
        let dir = TempDir::new("background-traversal");
        let root = launcher(&dir, "1289");
        dir.write("outside.png", b"outside");
        dir.write("launcher/ResFilesM/1289/bgimgs/bg_0.png", b"inside");
        dir.write(
            "launcher/ResFilesM/1289/bgimgs/config.json",
            r#"{"video":"../../outside.png","imgs":[{"file":"../outside.png"}],"noVideoBg":"bg_0.png"}"#,
        );

        let background = resolve(&root).expect("find background");
        assert_eq!(background.video, None);
        assert_eq!(
            background.image,
            Some(background_dir(&dir, "1289").join("bg_0.png"))
        );
    }

    #[test]
    fn falls_back_to_known_names_without_a_manifest() {
        let dir = TempDir::new("background-names");
        let root = launcher(&dir, "1289");
        dir.write("launcher/ResFilesM/1289/bgimgs/bg.mp4", b"video");
        dir.write("launcher/ResFilesM/1289/bgimgs/bg_0.png", b"image");

        let background = resolve(&root).expect("find background");
        assert_eq!(
            background.video,
            Some(background_dir(&dir, "1289").join("bg.mp4"))
        );
        assert_eq!(
            background.image,
            Some(background_dir(&dir, "1289").join("bg_0.png"))
        );
    }

    #[test]
    fn uses_no_video_bg_when_images_are_missing() {
        let dir = TempDir::new("background-no-video");
        let root = launcher(&dir, "1289");
        dir.write("launcher/ResFilesM/1289/bgimgs/bg_0.png", b"image");
        dir.write(
            "launcher/ResFilesM/1289/bgimgs/config.json",
            r#"{"imgs":[{"file":"missing.png"}],"noVideoBg":"bg_0.png"}"#,
        );

        let background = resolve(&root).expect("find background");
        assert_eq!(
            background.image,
            Some(background_dir(&dir, "1289").join("bg_0.png"))
        );
    }

    #[test]
    fn scans_game_directories_without_a_launcher_config() {
        let dir = TempDir::new("background-scan");
        let root = dir.join("launcher");
        dir.write("launcher/ResFilesM/3000001/bgimgs/bg.mp4", b"video");
        dir.write(
            "launcher/ResFilesM/3000001/bgimgs/config.json",
            r#"{"video":"bg.mp4"}"#,
        );

        let background = resolve(&root).expect("find background");
        assert_eq!(
            background.video,
            Some(dir.join("launcher/ResFilesM/3000001/bgimgs/bg.mp4"))
        );
        assert_eq!(background.image, None);
    }

    #[test]
    fn returns_none_without_usable_assets() {
        let dir = TempDir::new("background-empty");
        let root = launcher(&dir, "1289");
        dir.write(
            "launcher/ResFilesM/1289/bgimgs/config.json",
            r#"{"video":"bg.mp4","imgs":[{"file":"bg_0.png"}]}"#,
        );
        assert_eq!(resolve(&root), None);
    }
}
