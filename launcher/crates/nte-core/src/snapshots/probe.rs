//! 系统与图形探测.
//!
//! 只读取内核和用户态已经提供的文件; glxinfo 是可选探测, 失败时保持未采集.
use std::env;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use super::{Gpu, Graphics, Opengl, System};

pub fn package() -> String {
    if env::var_os("APPIMAGE").is_some_and(|value| !value.is_empty()) {
        return "AppImage".into();
    }
    if env::var_os("FLATPAK_ID").is_some_and(|value| !value.is_empty()) {
        return "Flatpak".into();
    }
    if env::current_exe().is_ok_and(|path| path.starts_with("/nix/store")) {
        return "Nix".into();
    }
    "系统安装".into()
}

pub fn system() -> System {
    System {
        os: os_release(),
        kernel: read_trim("/proc/sys/kernel/osrelease"),
        session: env_value("XDG_SESSION_TYPE").map(|value| value.to_ascii_lowercase()),
        desktop: env_value("XDG_CURRENT_DESKTOP"),
    }
}

pub fn graphics() -> Graphics {
    static CACHE: OnceLock<Graphics> = OnceLock::new();
    CACHE
        .get_or_init(|| Graphics {
            gpus: gpu_cards(),
            opengl: opengl(),
        })
        .clone()
}

fn env_value(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_trim(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn os_release() -> Option<String> {
    let text = fs::read_to_string("/etc/os-release").ok()?;
    let mut name = None;
    let mut version = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        match key {
            "PRETTY_NAME" if !value.is_empty() => return Some(value),
            "NAME" if !value.is_empty() => name = Some(value),
            "VERSION_ID" if !value.is_empty() => version = Some(value),
            _ => {}
        }
    }
    match (name, version) {
        (Some(name), Some(version)) => Some(format!("{name} {version}")),
        (Some(name), None) => Some(name),
        _ => None,
    }
}

fn is_card(name: &str) -> bool {
    name.strip_prefix("card")
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
}

fn gpu_cards() -> Vec<Gpu> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut gpus = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_card(&name) {
            continue;
        }
        let device = entry.path().join("device");
        let uevent = parse_uevent(&fs::read_to_string(device.join("uevent")).unwrap_or_default());
        let Some(pci_id) = uevent.pci_id.or_else(|| pci_ids(&device)) else {
            continue;
        };
        let driver = uevent.driver;
        let driver_version = driver.as_deref().and_then(driver_version);
        gpus.push(Gpu {
            pci_id,
            slot: uevent.slot.or(Some(name)),
            driver,
            driver_version,
        });
    }
    gpus.sort_by(|left, right| {
        left.slot
            .cmp(&right.slot)
            .then_with(|| left.pci_id.cmp(&right.pci_id))
    });
    gpus
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Uevent {
    driver: Option<String>,
    pci_id: Option<String>,
    slot: Option<String>,
}

fn parse_uevent(text: &str) -> Uevent {
    let mut uevent = Uevent::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key {
            "DRIVER" => uevent.driver = Some(value.to_string()),
            "PCI_ID" => uevent.pci_id = Some(value.to_string()),
            "PCI_SLOT_NAME" => uevent.slot = Some(value.to_string()),
            _ => {}
        }
    }
    uevent
}

fn pci_ids(device: &Path) -> Option<String> {
    let vendor = fs::read_to_string(device.join("vendor"))
        .ok()?
        .trim()
        .trim_start_matches("0x")
        .to_ascii_uppercase();
    let id = fs::read_to_string(device.join("device"))
        .ok()?
        .trim()
        .trim_start_matches("0x")
        .to_ascii_uppercase();
    if vendor.len() != 4 || id.len() != 4 {
        return None;
    }
    Some(format!("{vendor}:{id}"))
}

fn driver_version(driver: &str) -> Option<String> {
    if !driver
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return None;
    }
    if driver == "nvidia" {
        return read_trim("/sys/module/nvidia/version").or_else(|| {
            let text = fs::read_to_string("/proc/driver/nvidia/version").ok()?;
            parse_nvidia_version(&text)
        });
    }
    read_trim(&format!("/sys/module/{driver}/version"))
}

fn parse_nvidia_version(text: &str) -> Option<String> {
    text.lines().next()?.split_whitespace().find_map(|token| {
        (token
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
            && token.contains('.'))
        .then(|| token.to_string())
    })
}

fn opengl() -> Option<Opengl> {
    let text = probe_output("glxinfo", &["-B"], Duration::from_millis(600))?;
    parse_opengl(&text)
}

fn probe_output(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let output = tempfile::tempfile().ok()?;
    let mut reader = output.try_clone().ok()?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut bytes = Vec::new();
    reader.seek(SeekFrom::Start(0)).ok()?;
    reader.take(64 * 1024).read_to_end(&mut bytes).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn parse_opengl(text: &str) -> Option<Opengl> {
    let mut vendor = None;
    let mut renderer = None;
    let mut version = None;
    let mut core = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "OpenGL vendor string" => vendor = Some(value.to_string()),
            "OpenGL renderer string" => renderer = Some(value.to_string()),
            "OpenGL version string" => version = Some(value.to_string()),
            "OpenGL core profile version string" => core = Some(value.to_string()),
            _ => {}
        }
    }
    let version = version.or(core);
    if vendor.is_none() && renderer.is_none() && version.is_none() {
        return None;
    }
    Some(Opengl {
        vendor,
        renderer,
        version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nvidia_opengl_output() {
        let output = "name of display: :0\n\
            OpenGL vendor string: NVIDIA Corporation\n\
            OpenGL renderer string: NVIDIA GeForce RTX 5060 Laptop GPU/PCIe/SSE2\n\
            OpenGL core profile version string: 4.6.0 NVIDIA 615.78.08\n\
            OpenGL version string: 4.6.0 NVIDIA 615.78.08\n";
        let parsed = parse_opengl(output).unwrap();
        assert_eq!(parsed.vendor.as_deref(), Some("NVIDIA Corporation"));
        assert_eq!(
            parsed.renderer.as_deref(),
            Some("NVIDIA GeForce RTX 5060 Laptop GPU/PCIe/SSE2")
        );
        assert_eq!(parsed.version.as_deref(), Some("4.6.0 NVIDIA 615.78.08"));
    }

    #[test]
    fn parses_mesa_core_version_when_compatibility_is_absent() {
        let output = "OpenGL vendor string: AMD\n\
            OpenGL renderer string: AMD Radeon 780M (radeonsi, phoenix, LLVM 18.1.8)\n\
            OpenGL core profile version string: 4.6 (Core Profile) Mesa 24.2.7\n";
        let parsed = parse_opengl(output).unwrap();
        assert_eq!(
            parsed.version.as_deref(),
            Some("4.6 (Core Profile) Mesa 24.2.7")
        );
        assert_eq!(parsed.vendor.as_deref(), Some("AMD"));
    }

    #[test]
    fn ignores_output_without_opengl_fields() {
        assert!(parse_opengl("Error: unable to open display\n").is_none());
    }

    #[test]
    fn parses_drm_uevent() {
        let uevent = parse_uevent(
            "DRIVER=amdgpu\nPCI_CLASS=30000\nPCI_ID=1002:164E\nPCI_SLOT_NAME=0000:06:00.0\n",
        );
        assert_eq!(uevent.driver.as_deref(), Some("amdgpu"));
        assert_eq!(uevent.pci_id.as_deref(), Some("1002:164E"));
        assert_eq!(uevent.slot.as_deref(), Some("0000:06:00.0"));
    }

    #[test]
    fn parses_nvidia_proc_version() {
        let text = "NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  615.78.08  Release Build\nGCC version:  gcc\n";
        assert_eq!(parse_nvidia_version(text).as_deref(), Some("615.78.08"));
    }

    #[test]
    fn recognizes_card_names_only() {
        assert!(is_card("card0"));
        assert!(is_card("card12"));
        assert!(!is_card("card0-DP-2"));
        assert!(!is_card("renderD128"));
        assert!(!is_card("card"));
    }
}
