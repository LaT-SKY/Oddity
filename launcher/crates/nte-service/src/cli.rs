//! 外部启动参数. 保持手写解析, 避免为几个固定参数引入完整命令行框架.

use std::ffi::OsString;

use nte_core::distribution::Distribution;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    Official,
    Background,
    Direct,
}

impl LaunchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Background => "background",
            Self::Direct => "direct",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "official" => Ok(Self::Official),
            "background" => Ok(Self::Background),
            "direct" => Ok(Self::Direct),
            _ => Err(format!(
                "无法识别 --launch-mode 参数: {value}. 可选值: official, background, direct."
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    pub region: Option<Distribution>,
    pub mode: LaunchMode,
    pub environment: Option<String>,
}

pub const USAGE: &str = "用法: oddity --launch [--region cn|global] [--launch-mode official|background|direct] [--environment <名称或 ID>]";

/// --launch 配合 --help 时只打印用法, 避免把帮助当成一次启动请求.
pub fn help_requested(args: &[OsString]) -> bool {
    args.iter().any(|arg| arg == "--launch")
        && args.iter().any(|arg| arg == "--help" || arg == "-h")
}

/// 解析外部启动参数; 没有 --launch 时返回 None.
pub fn parse(args: &[OsString]) -> Result<Option<LaunchRequest>, String> {
    let mut launch = false;
    let mut region = None;
    let mut mode = None;
    let mut environment = None;
    let mut index = 0;
    while index < args.len() {
        let raw = args[index].to_string_lossy();
        if raw == "--launch" {
            launch = true;
            index += 1;
            continue;
        }
        if let Some(value) = take_value(args, &mut index, "--region")? {
            set_once(&mut region, value, "--region")?;
            continue;
        }
        if let Some(value) = take_value(args, &mut index, "--launch-mode")? {
            set_once(&mut mode, value, "--launch-mode")?;
            continue;
        }
        if let Some(value) = take_value(args, &mut index, "--environment")? {
            set_once(&mut environment, value, "--environment")?;
            continue;
        }
        // 宿主可能追加自己的参数, 未知项保持忽略.
        index += 1;
    }

    if !launch {
        if region.is_some() || mode.is_some() || environment.is_some() {
            return Err(
                "--region, --launch-mode 和 --environment 只能与 --launch 一起使用.".into(),
            );
        }
        return Ok(None);
    }

    let region = match region {
        Some(value) => Some(
            value
                .parse::<Distribution>()
                .map_err(|_| format!("无法识别 --region 参数: {value}. 可选值: cn, global."))?,
        ),
        None => None,
    };
    let mode = match mode {
        Some(value) => LaunchMode::parse(&value)?,
        // 桌面动作不传模式时也走官方启动器与正常登录流程.
        None => LaunchMode::Background,
    };
    Ok(Some(LaunchRequest {
        region,
        mode,
        environment,
    }))
}

fn take_value(args: &[OsString], index: &mut usize, name: &str) -> Result<Option<String>, String> {
    let raw = args[*index].to_string_lossy();
    let value = if raw == name {
        *index += 1;
        let value = args
            .get(*index)
            .ok_or_else(|| format!("{name} 缺少参数值."))?;
        value.to_string_lossy().to_string()
    } else if let Some(value) = raw.strip_prefix(&format!("{name}=")) {
        *index += 1;
        value.to_string()
    } else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() || value.starts_with("--") {
        return Err(format!("{name} 缺少参数值."));
    }
    Ok(Some(value.to_string()))
}

fn set_once(slot: &mut Option<String>, value: String, name: &str) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("{name} 不能重复指定."));
    }
    *slot = Some(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn plain_startup_has_no_launch_request() {
        assert!(parse(&args(&[])).unwrap().is_none());
        assert!(parse(&args(&["--recovery"])).unwrap().is_none());
        assert!(parse(&args(&["--some-gtk-flag"])).unwrap().is_none());
    }

    #[test]
    fn launch_defaults_to_background_and_current_region() {
        let request = parse(&args(&["--launch"])).unwrap().unwrap();
        assert_eq!(request.region, None);
        assert_eq!(request.mode, LaunchMode::Background);
        assert_eq!(request.environment, None);
    }

    #[test]
    fn parses_both_region_forms_and_all_modes() {
        let request = parse(&args(&["--launch", "--region", "CN"]))
            .unwrap()
            .unwrap();
        assert_eq!(request.region, Some(Distribution::Cn));
        let request = parse(&args(&["--region=global", "--launch"]))
            .unwrap()
            .unwrap();
        assert_eq!(request.region, Some(Distribution::Global));
        for (value, expected) in [
            ("official", LaunchMode::Official),
            ("background", LaunchMode::Background),
            ("direct", LaunchMode::Direct),
        ] {
            let request = parse(&args(&["--launch", "--launch-mode", value]))
                .unwrap()
                .unwrap();
            assert_eq!(request.mode, expected);
        }
    }

    #[test]
    fn parses_environment_by_name_or_id() {
        let request = parse(&args(&["--launch", "--environment", "GE-Proton 9"]))
            .unwrap()
            .unwrap();
        assert_eq!(request.environment.as_deref(), Some("GE-Proton 9"));
        let request = parse(&args(&["--launch", "--environment=abc123"]))
            .unwrap()
            .unwrap();
        assert_eq!(request.environment.as_deref(), Some("abc123"));
    }

    #[test]
    fn rejects_options_without_launch_and_bad_values() {
        assert!(parse(&args(&["--region", "cn"])).is_err());
        assert!(parse(&args(&["--launch", "--region", "eu"])).is_err());
        assert!(parse(&args(&["--launch", "--region"])).is_err());
        assert!(parse(&args(&["--launch", "--region", "--launch-mode"])).is_err());
        assert!(parse(&args(&["--launch", "--launch-mode", "debug"])).is_err());
        assert!(parse(&args(&["--launch", "--environment"])).is_err());
        assert!(parse(&args(&["--launch", "--region", "cn", "--region", "global"])).is_err());
        assert!(parse(&args(&[
            "--launch",
            "--launch-mode",
            "direct",
            "--launch-mode",
            "official"
        ]))
        .is_err());
    }

    #[test]
    fn help_only_applies_to_launch_requests() {
        assert!(!help_requested(&args(&["--help"])));
        assert!(help_requested(&args(&["--launch", "--help"])));
        assert!(help_requested(&args(&["-h", "--launch"])));
        assert!(!help_requested(&args(&["--launch"])));
    }

    #[test]
    fn ignores_unknown_arguments_between_known_options() {
        let request = parse(&args(&[
            "--display",
            ":0",
            "--launch",
            "--region",
            "global",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(request.region, Some(Distribution::Global));
    }
}
