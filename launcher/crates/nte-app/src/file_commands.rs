use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    name: String,
    path: String,
    directory: bool,
}
#[derive(Serialize)]
pub struct Listing {
    path: String,
    parent: Option<String>,
    home: String,
    entries: Vec<Entry>,
}

#[tauri::command]
pub fn browse_files(path: Option<String>) -> Result<Listing, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("无法定位主目录.")?;
    let requested = path
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| home.display().to_string());
    let requested = if requested == "~" {
        home.clone()
    } else if let Some(suffix) = requested.strip_prefix("~/") {
        home.join(suffix)
    } else {
        PathBuf::from(requested)
    };
    if !requested.is_absolute() {
        return Err("请选择绝对路径.".into());
    }
    let path = requested.canonicalize().map_err(|e| e.to_string())?;
    let mut entries = std::fs::read_dir(&path)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let metadata = entry.path().metadata().ok()?;
            (metadata.is_dir() || metadata.is_file()).then(|| Entry {
                name,
                path: entry.path().display().to_string(),
                directory: metadata.is_dir(),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(Listing {
        path: path.display().to_string(),
        parent: path.parent().map(|p| p.display().to_string()),
        home: home.display().to_string(),
        entries,
    })
}

#[cfg(test)]
#[path = "../../../../internal/gamepad/tests/files.rs"]
mod tests;
