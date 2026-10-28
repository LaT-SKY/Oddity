use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{bail, Context, Result};

pub(super) fn read() -> Result<String> {
    match fs::read_to_string("/etc/gamemode.ini") {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).context("读取 GameMode 配置失败"),
    }
}

// 仅替换当前功能管理的字段, 保留其他功能、用户注释和未知配置.
type Section<'a> = (&'a str, Vec<(&'a str, Option<String>)>);

pub(super) fn write(path: &Path, sections: &[Section<'_>]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let original = match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => fs::read_to_string(path)?,
        Ok(_) => bail!("GameMode 配置不是普通文件, 请使用系统配置管理工具修改"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let mut text = original.clone();
    for (name, settings) in sections {
        let mut output = Vec::new();
        let mut inside = false;
        let mut inserted = false;
        let values: Vec<_> = settings
            .iter()
            .filter_map(|(key, value)| value.as_ref().map(|value| format!("{key}={value}")))
            .collect();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                if inside && !inserted {
                    output.extend(values.iter().cloned());
                    inserted = true;
                }
                inside = trimmed.eq_ignore_ascii_case(&format!("[{name}]"));
            }
            if inside
                && trimmed.split_once('=').is_some_and(|(key, _)| {
                    settings.iter().any(|(managed, _)| *managed == key.trim())
                })
            {
                continue;
            }
            output.push(line.to_string());
        }
        if !inserted && !values.is_empty() {
            if !inside {
                output.push(format!("[{name}]"));
            }
            output.extend(values);
        }
        text = output.join("\n");
    }
    if path.exists() {
        fs::copy(path, path.with_extension("ini.oddity-backup"))?;
    }
    let mut temporary = crate::paths::TemporaryFile::new(path)?;
    temporary
        .file
        .set_permissions(fs::Permissions::from_mode(0o644))?;
    writeln!(temporary.file, "{text}")?;
    temporary.commit(path, &|| false)?;
    Ok(())
}
