//! 每日和开机模式共享 systemd 用户服务。
use super::{Failure, FailureKind, Preferences, Result, Trigger};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
const SERVICE: &str = "oddity-taygedo.service";
const TIMER: &str = "oddity-taygedo.timer";
const BOOT_TIMER: &str = "oddity-taygedo-boot.timer";
fn error(_: impl std::fmt::Display) -> Failure {
    Failure::new(
        FailureKind::Service,
        "无法更新用户定时器, 请检查 systemctl --user status oddity-taygedo.timer",
    )
}
fn directory() -> PathBuf {
    super::default_dir()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("systemd/user")
}
fn quote(path: &Path) -> Result<String> {
    let s = path
        .to_str()
        .filter(|s| !s.chars().any(char::is_control))
        .ok_or_else(|| error("path"))?;
    if !path.is_absolute() {
        return Err(error("absolute path required"));
    }
    Ok(format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
            .replace('$', "$$")
    ))
}
fn service_unit(executable: &Path, config: &Path) -> Result<String> {
    Ok(format!("[Unit]\nDescription=Oddity Taygedo attendance\n\n[Service]\nType=oneshot\nExecStart={} --taygedo-attendance --taygedo-config {}\nUMask=0077\nTimeoutStartSec=10min\nNoNewPrivileges=true\n", quote(executable)?, quote(config)?))
}
fn daily_timer_unit(time: &str) -> String {
    format!("[Unit]\nDescription=Oddity daily Taygedo attendance\n\n[Timer]\nOnCalendar=*-*-* {time}:00 Asia/Shanghai\nPersistent=true\nRandomizedDelaySec=60\nUnit={SERVICE}\n\n[Install]\nWantedBy=timers.target\n")
}
fn boot_timer_unit() -> String {
    format!("[Unit]\nDescription=Oddity Taygedo attendance at login\n\n[Timer]\nOnStartupSec=45s\nRandomizedDelaySec=30\nUnit={SERVICE}\n\n[Install]\nWantedBy=timers.target\n")
}
pub fn units(executable: &Path, config: &Path, prefs: &Preferences) -> Result<(String, String)> {
    prefs.validate()?;
    Ok((
        service_unit(executable, config)?,
        if prefs.trigger == Trigger::Boot {
            boot_timer_unit()
        } else {
            daily_timer_unit(&prefs.time)
        },
    ))
}
fn control(args: &[&str]) -> Result<()> {
    if Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(error)?
        .success()
    {
        Ok(())
    } else {
        Err(error("systemctl"))
    }
}
pub fn apply(executable: &Path, config: &Path, prefs: &Preferences) -> Result<()> {
    apply_in(&directory(), executable, config, prefs, control)
}
fn apply_in(
    dir: &Path,
    executable: &Path,
    config: &Path,
    prefs: &Preferences,
    mut command: impl FnMut(&[&str]) -> Result<()>,
) -> Result<()> {
    prefs.validate()?;
    let service = dir.join(SERVICE);
    let timer = dir.join(TIMER);
    let boot = dir.join(BOOT_TIMER);
    if !matches!(prefs.trigger, Trigger::Daily | Trigger::Boot) {
        let mut changed = false;
        if timer.exists() {
            command(&["disable", "--now", TIMER])?;
            remove(&timer)?;
            changed = true;
        }
        if boot.exists() {
            command(&["disable", "--now", BOOT_TIMER])?;
            remove(&boot)?;
            changed = true;
        }
        if service.exists() {
            command(&["stop", SERVICE])?;
            remove(&service)?;
            changed = true;
        }
        if changed {
            command(&["daemon-reload"])?;
        }
        return Ok(());
    }
    let (a, b) = units(executable, config, prefs)?;
    let (active, inactive) = if prefs.trigger == Trigger::Boot {
        (BOOT_TIMER, TIMER)
    } else {
        (TIMER, BOOT_TIMER)
    };
    if dir.join(inactive).exists() {
        command(&["disable", "--now", inactive])?;
        remove(&dir.join(inactive))?;
    }
    fs::create_dir_all(dir).map_err(error)?;
    fs::write(service, a).map_err(error)?;
    let install = (|| {
        fs::write(dir.join(active), b).map_err(error)?;
        command(&["daemon-reload"])?;
        command(&["enable", "--now", active])?;
        if prefs.trigger == Trigger::Daily {
            command(&["restart", active])?;
        }
        Ok(())
    })();
    if install.is_err() {
        let _ = command(&["disable", "--now", active]);
        let _ = remove(&dir.join(active));
        let _ = command(&["daemon-reload"]);
    }
    install
}
fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(error(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    #[test]
    fn boot_switch_removal_and_failed_install_cleanup() {
        let dir = TempDir::new("boot-schedule");
        let exe = Path::new("/opt/a b/oddity%$");
        let cfg = Path::new("/config space");
        let boot = Preferences {
            trigger: Trigger::Boot,
            ..Preferences::default()
        };
        let (service, timer) = units(exe, cfg, &boot).unwrap();
        assert!(service.contains("\"/opt/a b/oddity%%$$\""));
        assert!(service.contains("\"/config space\""));
        for line in [
            "OnStartupSec=45s",
            "RandomizedDelaySec=30",
            "Unit=oddity-taygedo.service",
            "WantedBy=timers.target",
            "Description=Oddity Taygedo attendance at login",
        ] {
            assert!(timer.contains(line));
        }
        let mut commands = Vec::new();
        apply_in(dir.path(), exe, cfg, &boot, |args| {
            commands.push(args.join(" "));
            Ok(())
        })
        .unwrap();
        assert!(dir.is_file(BOOT_TIMER));
        assert!(dir.is_file(SERVICE));
        assert!(!dir.is_file(TIMER));
        assert_eq!(
            commands,
            ["daemon-reload", "enable --now oddity-taygedo-boot.timer"]
        );
        let daily = Preferences {
            trigger: Trigger::Daily,
            ..Preferences::default()
        };
        apply_in(dir.path(), exe, cfg, &daily, |_| Ok(())).unwrap();
        assert!(!dir.is_file(BOOT_TIMER));
        assert!(dir.is_file(TIMER));
        apply_in(dir.path(), exe, cfg, &boot, |_| Ok(())).unwrap();
        assert!(!dir.is_file(TIMER));
        apply_in(dir.path(), exe, cfg, &Preferences::default(), |_| Ok(())).unwrap();
        for name in [BOOT_TIMER, TIMER, SERVICE] {
            assert!(!dir.is_file(name));
        }
        let mut reloads = 0;
        assert!(apply_in(dir.path(), exe, cfg, &boot, |args| {
            if args == ["daemon-reload"] {
                reloads += 1;
            }
            if args.first() == Some(&"enable") {
                Err(error("test"))
            } else {
                Ok(())
            }
        })
        .is_err());
        assert!(!dir.is_file(BOOT_TIMER));
        assert_eq!(reloads, 2);
    }
    #[test]
    fn daily_install_off_removal_and_quoting() {
        let dir = TempDir::new("schedule");
        let exe = Path::new("/opt/a b/oddity%$");
        let cfg = Path::new("/config");
        apply_in(dir.path(), exe, cfg, &Preferences::default(), |_| {
            panic!("默认不调用 systemd")
        })
        .unwrap();
        let p = Preferences {
            trigger: Trigger::Daily,
            time: "20:30".into(),
            ..Preferences::default()
        };
        let (s, t) = units(exe, cfg, &p).unwrap();
        assert!(s.contains("oddity%%$$"));
        assert!(t.contains("20:30:00 Asia/Shanghai"));
        assert!(t.contains("Persistent=true"));
        apply_in(dir.path(), exe, cfg, &p, |_| Ok(())).unwrap();
        assert!(dir.is_file(TIMER));
        apply_in(dir.path(), exe, cfg, &Preferences::default(), |_| Ok(())).unwrap();
        assert!(!dir.is_file(TIMER));
        assert!(units(Path::new("relative"), cfg, &p).is_err());
        assert!(units(Path::new("/bad\n"), cfg, &p).is_err());
    }
}
