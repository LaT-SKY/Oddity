//! Oddity native Linux launcher shell.
//!
//! Tauri 2 host for the Vue UI built into `launcher/ui/dist`. The launcher deliberately keeps
//! the official Windows launcher + game on the verified Proton bridge for v1:
//! our UI owns update/check/diagnostics/experience, the official components own
//! login and resource patching.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent};

use nte_core::bulletin::{self, BulletinFeed};
use nte_core::config::{Ini, LauncherConfig};
use nte_core::manifest::Manifest;
use nte_core::{update, version};

mod attendance_commands;

const APP_IDENTIFIER: &str = "io.github.oddity.Launcher";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Settings {
    install_root: Option<String>,
    bridge_root: Option<String>,
    prefix: Option<String>,
    proton_path: Option<String>,
    launch_mode: String,
    language: String,
    reduce_motion: bool,
    theme: String,
    hero_media: String,
    minimize_to_tray_on_close: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            install_root: None,
            bridge_root: None,
            prefix: None,
            proton_path: None,
            launch_mode: "background".to_string(),
            language: "zh-CN".to_string(),
            reduce_motion: false,
            theme: "light".to_string(),
            hero_media: "video".to_string(),
            minimize_to_tray_on_close: true,
        }
    }
}

struct AppState {
    settings: Mutex<Settings>,
    game: Mutex<GameTracker>,
}

#[derive(Default)]
struct GameTracker {
    session: Option<GameSession>,
    starting: bool,
    next_id: u64,
}

/// A launched official launcher/game process tree. `pgid` is the process group
/// created for `umu-run`, so signalling it stops the wrapper, Proton and the
/// Windows game together.
#[derive(Debug, Clone)]
struct GameSession {
    id: u64,
    pid: u32,
    pgid: i32,
    exe: String,
    started_at: u64,
    log: String,
    closing: bool,
}

/// A validated Proton installation. umu-launcher expects `PROTONPATH` to be
/// the installation directory (the one holding `proton` and
/// `toolmanifest.vdf`), not the `proton` executable itself.
#[derive(Debug, Clone)]
struct ProtonInstall {
    name: String,
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckItem {
    label: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Bootstrap {
    ok: bool,
    error: Option<String>,
    install_root: String,
    launcher_root: String,
    bridge_root: String,
    bridge_ready: bool,
    prefix: String,
    prefix_exists: bool,
    proton_name: String,
    proton_path: String,
    umu_available: bool,
    launcher_version: String,
    launcher_build: String,
    game_version: String,
    game_exe: String,
    game_exists: bool,
    sdk_path: String,
    checks: Vec<CheckItem>,
    settings: Settings,
    app_version: String,
    os: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckResult {
    remote_version: String,
    local_version: String,
    product_version: String,
    files: usize,
    ok: usize,
    missing: usize,
    changed: usize,
    bytes: u64,
    duration_ms: u128,
    missing_preview: Vec<String>,
    changed_preview: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchResult {
    mode: String,
    pid: u32,
    exe: String,
    prefix: String,
    log: String,
    started_at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GameStatus {
    running: bool,
    closing: bool,
    pid: Option<u32>,
    started_at: Option<u64>,
    exe: Option<String>,
    log: Option<String>,
}

impl GameStatus {
    fn idle() -> Self {
        Self {
            running: false,
            closing: false,
            pid: None,
            started_at: None,
            exe: None,
            log: None,
        }
    }

    fn from_session(session: Option<&GameSession>) -> Self {
        match session {
            Some(session) => Self {
                running: true,
                closing: session.closing,
                pid: Some(session.pid),
                started_at: Some(session.started_at),
                exe: Some(session.exe.clone()),
                log: Some(session.log.clone()),
            },
            None => Self::idle(),
        }
    }
}

struct LaunchInvocation {
    result: LaunchResult,
    child: Child,
    pgid: i32,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Last useful lines of the launch log for an immediate-exit error.
fn launch_log_tail(path: &Path) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    let start = lines.len().saturating_sub(3);
    let mut tail = lines[start..].join(" | ");
    if tail.chars().count() > 400 {
        tail = tail.chars().take(400).collect::<String>() + "…";
    }
    format!("：{tail}")
}

/// True when at least one process still belongs to the group. Zombie members
/// count as alive until the monitor thread reaps the group leader, which is
/// intentional: the game is not gone until its whole tree is gone.
fn process_group_alive(pgid: i32) -> bool {
    if pgid <= 0 {
        return false;
    }
    let result = unsafe { libc::kill(-pgid, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

fn signal_process_group(pgid: i32, signal: libc::c_int) -> Result<(), String> {
    if pgid <= 0 {
        return Ok(());
    }
    let result = unsafe { libc::kill(-pgid, signal) };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        // Already gone between the liveness check and the signal.
        return Ok(());
    }
    Err(format!("向游戏进程组发送信号失败：{error}"))
}

/// Ask the whole group to quit, then force-kill whatever is left. The monitor
/// thread reaps the leader concurrently so the group can actually disappear.
fn terminate_process_group(pgid: i32) -> Result<(), String> {
    if !process_group_alive(pgid) {
        return Ok(());
    }
    signal_process_group(pgid, libc::SIGTERM)?;
    let graceful_deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < graceful_deadline {
        if !process_group_alive(pgid) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(120));
    }

    signal_process_group(pgid, libc::SIGKILL)?;
    let forced_deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < forced_deadline {
        if !process_group_alive(pgid) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(80));
    }
    Err("游戏进程未响应结束请求，请稍后重试。".to_string())
}

/// Reap the wrapper and clear the session as soon as it exits.
///
/// Proton/Wine teardown can keep the original process group alive for a while
/// (wineserver, CEF helpers, the tray host). The game session is over when the
/// helper/wrapper exits, so the UI must not stay on "关闭游戏" while waiting
/// for that cleanup; otherwise a normal in-game exit leaves a stale running
/// state and the "启动游戏" button is unavailable.
fn spawn_game_monitor(app: AppHandle, mut child: Child, session: GameSession) {
    thread::spawn(move || {
        let _ = child.wait();

        let cleared = {
            let state = app.state::<AppState>();
            let mut tracker = match state.game.lock() {
                Ok(tracker) => tracker,
                Err(_) => return,
            };
            if tracker.session.as_ref().map(|current| current.id) == Some(session.id) {
                tracker.session = None;
                true
            } else {
                false
            }
        };
        if cleared {
            let _ = app.emit("game-state", GameStatus::idle());
        }
    });
}

fn emit_game_status(app: &AppHandle, state: &AppState) {
    let status = state
        .game
        .lock()
        .map(|tracker| GameStatus::from_session(tracker.session.as_ref()))
        .unwrap_or_else(|_| GameStatus::idle());
    let _ = app.emit("game-state", status);
}

// ---------------------------------------------------------------------------
// Paths and detection
// ---------------------------------------------------------------------------

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// Expand the `~` forms accepted by the settings UI. Everything else is
/// returned unchanged so relative paths keep their current-directory meaning.
fn expand_path(raw: &str) -> PathBuf {
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(raw)
}

fn default_prefix() -> PathBuf {
    std::env::var_os("WINEPREFIX")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| data_dir().join("prefix"))
}

/// Steam roots in discovery order. Steam exports the client path while it is
/// running; XDG and the legacy `~/.steam` layout cover the rest.
fn steam_roots() -> Vec<PathBuf> {
    fn push(roots: &mut Vec<PathBuf>, path: PathBuf) {
        if !path.as_os_str().is_empty() && !roots.contains(&path) {
            roots.push(path);
        }
    }

    let mut roots = Vec::new();
    if let Some(raw) = std::env::var_os("STEAM_COMPAT_CLIENT_INSTALL_PATH") {
        push(&mut roots, PathBuf::from(raw));
    }
    if let Some(raw) = std::env::var_os("XDG_DATA_HOME") {
        push(&mut roots, PathBuf::from(raw).join("Steam"));
    }
    push(
        &mut roots,
        home_dir().join(".local").join("share").join("Steam"),
    );
    push(&mut roots, home_dir().join(".steam").join("steam"));
    push(&mut roots, home_dir().join(".steam").join("root"));
    roots
}

fn install_seeds(base: &Path, seeds: &mut Vec<PathBuf>) {
    seeds.push(
        base.join("Program Files (x86)")
            .join("Neverness To Everness")
            .join("NTELauncher"),
    );
    seeds.push(
        base.join("Program Files")
            .join("Neverness To Everness")
            .join("NTELauncher"),
    );
    seeds.push(base.join("Neverness To Everness").join("NTELauncher"));
    seeds.push(base.join("NTELauncher"));
}

fn data_base() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local").join("share"))
}

fn data_dir() -> PathBuf {
    data_base().join("oddity")
}

fn legacy_data_dir() -> PathBuf {
    data_base().join("TheBetterNTE")
}

fn config_base() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
}

fn config_dir() -> PathBuf {
    config_base().join("oddity")
}

fn legacy_config_dir() -> PathBuf {
    config_base().join("TheBetterNTE")
}

/// One-time rename from the old project name. The old data directory is kept
/// as a symlink so absolute paths embedded in the Wine prefix and the official
/// launcher configs keep resolving.
///
/// Settings `prefix` / `bridgeRoot` are intentionally **not** rewritten: the
/// official OneSDK auto-login is sensitive to the exact Wine prefix / bridge
/// path, so upgraded installs keep using the legacy path through the symlink
/// while new installs use the canonical `oddity` path.
fn migrate_legacy_dirs() {
    let old_data = legacy_data_dir();
    let new_data = data_dir();
    if old_data.is_dir() && !new_data.exists() {
        match fs::rename(&old_data, &new_data) {
            Ok(()) => {
                let _ = symlink(&new_data, &old_data);
            }
            Err(error) => {
                eprintln!(
                    "migration: rename {} -> {} failed: {error}",
                    old_data.display(),
                    new_data.display()
                );
            }
        }
    }

    let old_config = legacy_config_dir();
    let new_config = config_dir();
    if old_config.is_dir() && !new_config.exists() {
        match fs::rename(&old_config, &new_config) {
            Ok(()) => {
                let _ = symlink(&new_config, &old_config);
            }
            Err(error) => {
                eprintln!(
                    "migration: rename {} -> {} failed: {error}",
                    old_config.display(),
                    new_config.display()
                );
                if fs::create_dir_all(&new_config).is_ok() {
                    let _ = fs::copy(
                        old_config.join("settings.json"),
                        new_config.join("settings.json"),
                    );
                }
            }
        }
    }
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

fn load_settings() -> Settings {
    let path = settings_path();
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

fn persist_settings(settings: &Settings) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let tmp = dir.join("settings.json.tmp");
    fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, settings_path()).map_err(|e| format!("rename settings: {e}"))?;
    Ok(())
}

fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(cmd);
        if let Ok(meta) = fs::metadata(&candidate) {
            if meta.is_file() && (meta.permissions().mode() & 0o111) != 0 {
                return Some(candidate);
            }
        }
    }
    None
}

/// Accept either the launcher directory (contains Config/Config.ini) or the
/// install directory (contains NTELauncher/Config/Config.ini).
fn normalize_launcher_root(path: &Path) -> Option<PathBuf> {
    if path.join("Config").join("Config.ini").is_file() {
        return Some(path.to_path_buf());
    }
    let nested = path.join("NTELauncher");
    if nested.join("Config").join("Config.ini").is_file() {
        return Some(nested);
    }
    None
}

fn detect_launcher_root(settings: &Settings) -> Option<PathBuf> {
    if let Some(raw) = &settings.install_root {
        if let Some(root) = normalize_launcher_root(&expand_path(raw)) {
            return Some(root);
        }
    }

    for key in ["NTE_ROOT", "NTE_INSTALL"] {
        if let Some(raw) = std::env::var_os(key) {
            if let Some(root) = normalize_launcher_root(&expand_path(&raw.to_string_lossy())) {
                return Some(root);
            }
        }
    }

    let mut seeds: Vec<PathBuf> = Vec::new();
    seeds.push(
        home_dir()
            .join("Games")
            .join("Neverness To Everness")
            .join("NTELauncher"),
    );
    seeds.push(home_dir().join("Games").join("异环").join("NTELauncher"));

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            install_seeds(ancestor, &mut seeds);
        }
    }

    // Removable/extra drives: /run/media and /media hold per-user directories,
    // while /mnt usually holds mount points directly. Probe both levels so the
    // install is discovered regardless of the mount convention.
    for mount_root in ["/run/media", "/media", "/mnt"] {
        if let Ok(entries) = fs::read_dir(mount_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                install_seeds(&path, &mut seeds);
                if let Ok(children) = fs::read_dir(&path) {
                    for child in children.flatten() {
                        install_seeds(&child.path(), &mut seeds);
                    }
                }
            }
        }
    }

    // The local bridge copy is a fallback only: the real install is more useful
    // for display, version detection and log/diagnostic paths.
    seeds.push(data_dir().join("winroot").join("NTELauncher"));

    for seed in seeds {
        if let Some(root) = normalize_launcher_root(&seed) {
            return Some(root);
        }
    }
    None
}

fn bridge_root_for(settings: &Settings, launcher_root: &Path) -> PathBuf {
    if let Some(raw) = &settings.bridge_root {
        let p = expand_path(raw);
        if p.join("NTELauncher").join("NTEGame.exe").is_file() {
            return p;
        }
    }
    let default = data_dir().join("winroot");
    if default.join("NTELauncher").join("NTEGame.exe").is_file() {
        return default;
    }
    launcher_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| launcher_root.to_path_buf())
}

fn find_ci(base: &Path, components: &[&str]) -> Option<PathBuf> {
    let mut current = base.to_path_buf();
    for component in components {
        let entries = fs::read_dir(&current).ok()?;
        let mut found = None;
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(component)
            {
                found = Some(entry.path());
                break;
            }
        }
        current = found?;
    }
    Some(current)
}

fn game_exe_for(launcher_root: &Path) -> Option<PathBuf> {
    let install = launcher_root.parent()?;
    find_ci(
        install,
        &[
            "Client",
            "WindowsNoEditor",
            "HT",
            "Binaries",
            "Win64",
            "HTGame.exe",
        ],
    )
}

fn find_sdk(launcher_root: &Path) -> Option<PathBuf> {
    find_ci(launcher_root, &["WPGameClientSDK_64.dll"])
}

fn game_version_for(launcher_root: &Path) -> String {
    let user = launcher_root
        .join("UserData")
        .join("Config")
        .join("Config.ini");
    if let Ok(ini) = Ini::load(&user) {
        if let Some(v) = ini.get("Game", "Version_") {
            if !v.is_empty() {
                return v.to_string();
            }
        }
    }
    String::new()
}

fn proton_install(path: &Path) -> Option<ProtonInstall> {
    let root = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    // umu-launcher validates both files before it starts Proton.
    if !root.join("proton").is_file() || !root.join("toolmanifest.vdf").is_file() {
        return None;
    }
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "custom Proton".to_string());
    Some(ProtonInstall { name, root })
}

fn detect_proton(settings: &Settings) -> Option<ProtonInstall> {
    if let Some(raw) = &settings.proton_path {
        if let Some(install) = proton_install(&expand_path(raw)) {
            return Some(install);
        }
    }
    if let Some(raw) = std::env::var_os("PROTONPATH") {
        if let Some(install) = proton_install(&expand_path(&raw.to_string_lossy())) {
            return Some(install);
        }
    }

    let mut candidates: Vec<ProtonInstall> = Vec::new();
    for root in steam_roots() {
        // Custom Proton builds (GE-Proton, CachyOS, ...) live here.
        let compat = root.join("compatibilitytools.d");
        if let Ok(entries) = fs::read_dir(&compat) {
            for entry in entries.flatten() {
                if let Some(install) = proton_install(&entry.path()) {
                    candidates.push(install);
                }
            }
        }
        // Valve's own builds live in the Steam library.
        let common = root.join("steamapps").join("common");
        if let Ok(entries) = fs::read_dir(&common) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("Proton") {
                    continue;
                }
                if let Some(install) = proton_install(&entry.path()) {
                    candidates.push(install);
                }
            }
        }
    }
    fn proton_score(name: &str) -> u8 {
        if name.contains("GE-Proton") {
            0
        } else if name.contains("CachyOS") {
            1
        } else if name.contains("Experimental") {
            3
        } else {
            2
        }
    }
    candidates.sort_by(|a, b| {
        proton_score(&a.name)
            .cmp(&proton_score(&b.name))
            .then_with(|| b.name.cmp(&a.name))
    });
    candidates.into_iter().next()
}

fn ensure_sdk_link(prefix: &Path, install_dir: &Path) -> Result<(), String> {
    let drive_c = prefix.join("drive_c");
    if !drive_c.is_dir() {
        return Err(format!(
            "Wine prefix 尚未初始化：{}（先运行一次 Proton 以创建 prefix）",
            prefix.display()
        ));
    }
    let link = drive_c
        .join("Program Files (x86)")
        .join("Neverness To Everness");
    if let Ok(meta) = link.symlink_metadata() {
        if meta.file_type().is_symlink() {
            let good = fs::read_link(&link)
                .map(|target| target == install_dir)
                .unwrap_or(false);
            if good {
                return Ok(());
            }
            fs::remove_file(&link).map_err(|e| format!("remove stale {}: {e}", link.display()))?;
        } else {
            return Ok(());
        }
    }
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    symlink(install_dir, &link).map_err(|e| format!("symlink {}: {e}", link.display()))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn bootstrap_impl(settings: &Settings) -> Bootstrap {
    let mut error: Option<String> = None;
    let launcher_root = detect_launcher_root(settings);
    let install_root = launcher_root
        .as_ref()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from(""));
    let bridge_root = launcher_root
        .as_ref()
        .map(|p| bridge_root_for(settings, p))
        .unwrap_or_else(|| data_dir().join("winroot"));
    let prefix = settings
        .prefix
        .as_deref()
        .map(expand_path)
        .unwrap_or_else(default_prefix);
    let prefix_exists = prefix.join("drive_c").is_dir();

    let mut launcher_version = String::new();
    let mut launcher_build = String::new();
    if let Some(root) = &launcher_root {
        match LauncherConfig::load(root) {
            Ok(cfg) => {
                launcher_version = cfg.version;
                launcher_build = cfg.build;
            }
            Err(e) => error = Some(format!("读取启动器配置失败：{e}")),
        }
    } else {
        error = Some("尚未找到《异环》启动器安装目录。请到设置里指定安装路径。".to_string());
    }

    let mut game_version = launcher_root
        .as_ref()
        .map(|p| game_version_for(p))
        .unwrap_or_default();
    if let Some(root) = &launcher_root {
        let bridge = bridge_root_for(settings, root).join("NTELauncher");
        if bridge.is_dir() {
            let bridge_version = game_version_for(&bridge);
            if nte_core::version::compare(&bridge_version, &game_version)
                == std::cmp::Ordering::Greater
            {
                game_version = bridge_version;
            }
        }
    }
    let game_exe = launcher_root
        .as_ref()
        .and_then(|p| game_exe_for(p))
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let game_exists = !game_exe.is_empty() && Path::new(&game_exe).is_file();
    let sdk_path = launcher_root
        .as_ref()
        .and_then(|p| find_sdk(p))
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let proton = detect_proton(settings);
    let umu = which("umu-run");
    let bridge_ready = bridge_root
        .join("NTELauncher")
        .join("NTEGame.exe")
        .is_file();

    let mut checks = Vec::new();
    checks.push(CheckItem {
        label: "启动器配置".into(),
        ok: launcher_root.is_some(),
        detail: launcher_root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "未找到".into()),
    });
    checks.push(CheckItem {
        label: "游戏可执行文件".into(),
        ok: game_exists,
        detail: if game_exists {
            game_exe.clone()
        } else {
            "未找到 HTGame.exe（注意 Win64 大小写）".into()
        },
    });
    checks.push(CheckItem {
        label: "umu-launcher".into(),
        ok: umu.is_some(),
        detail: umu
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "PATH 中未找到 umu-run".into()),
    });
    checks.push(CheckItem {
        label: "Proton".into(),
        ok: proton.is_some(),
        detail: proton
            .as_ref()
            .map(|install| install.name.clone())
            .unwrap_or_else(|| "未找到 Proton".into()),
    });
    checks.push(CheckItem {
        label: "Wine prefix".into(),
        ok: prefix_exists,
        detail: prefix.display().to_string(),
    });
    checks.push(CheckItem {
        label: "官方启动器桥接".into(),
        ok: bridge_ready,
        detail: if bridge_ready {
            bridge_root.display().to_string()
        } else {
            "尚未准备；将使用安装目录内启动器".into()
        },
    });

    Bootstrap {
        ok: error.is_none() && launcher_root.is_some(),
        error,
        install_root: install_root.display().to_string(),
        launcher_root: launcher_root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        bridge_root: bridge_root.display().to_string(),
        bridge_ready,
        prefix: prefix.display().to_string(),
        prefix_exists,
        proton_name: proton
            .as_ref()
            .map(|install| install.name.clone())
            .unwrap_or_default(),
        proton_path: proton
            .as_ref()
            .map(|install| install.root.display().to_string())
            .unwrap_or_default(),
        umu_available: umu.is_some(),
        launcher_version,
        launcher_build,
        game_version,
        game_exe,
        game_exists,
        sdk_path,
        checks,
        settings: settings.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: format!(
            "{} / {} / {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into())
        ),
    }
}

fn check_impl(launcher_root: &Path, full: bool) -> Result<CheckResult, String> {
    let cfg =
        LauncherConfig::load(launcher_root).map_err(|e| format!("读取启动器配置失败：{e}"))?;
    let started = Instant::now();
    let remote =
        version::fetch(&cfg.version_url).map_err(|e| format!("拉取 Version.ini 失败：{e}"))?;
    let file_list_url = remote
        .file_list_url
        .clone()
        .ok_or_else(|| "远端 Version.ini 缺少 FileListURL".to_string())?;
    let manifest =
        Manifest::from_url(&file_list_url).map_err(|e| format!("拉取 AllFiles.xml 失败：{e}"))?;
    let report =
        update::check(&cfg.root, &manifest, !full).map_err(|e| format!("本地校验失败：{e}"))?;
    let duration_ms = started.elapsed().as_millis();
    let missing_preview = report
        .missing
        .iter()
        .take(12)
        .map(|f| f.path.clone())
        .collect();
    let changed_preview = report
        .changed
        .iter()
        .take(12)
        .map(|f| f.path.clone())
        .collect();
    Ok(CheckResult {
        remote_version: remote.version,
        local_version: cfg.version,
        product_version: manifest.product_version.unwrap_or_default(),
        files: manifest.files.len(),
        ok: report.ok,
        missing: report.missing.len(),
        changed: report.changed.len(),
        bytes: report.total_missing_bytes(),
        duration_ms,
        missing_preview,
        changed_preview,
    })
}

#[tauri::command]
fn bootstrap(state: State<AppState>) -> Bootstrap {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    bootstrap_impl(&settings)
}

#[tauri::command]
async fn check_update(state: State<'_, AppState>, full: bool) -> Result<CheckResult, String> {
    let settings = state
        .settings
        .lock()
        .map(|s| s.clone())
        .map_err(|e| e.to_string())?;
    let root = detect_launcher_root(&settings)
        .ok_or_else(|| "尚未找到《异环》启动器安装目录".to_string())?;
    tauri::async_runtime::spawn_blocking(move || check_impl(&root, full))
        .await
        .map_err(|e| format!("校验任务失败：{e}"))?
}

#[tauri::command]
async fn fetch_bulletins() -> Result<BulletinFeed, String> {
    tauri::async_runtime::spawn_blocking(bulletin::fetch)
        .await
        .map_err(|error| format!("公告抓取任务失败：{error}"))
}

#[tauri::command]
fn save_settings(state: State<AppState>, settings: Settings) -> Result<Bootstrap, String> {
    let mut next = settings;
    if !matches!(
        next.launch_mode.as_str(),
        "official" | "background" | "direct"
    ) {
        next.launch_mode = "background".into();
    }
    if next.theme != "light" && next.theme != "dark" {
        next.theme = "light".into();
    }
    if next.hero_media != "video" && next.hero_media != "photo" {
        next.hero_media = "video".into();
    }
    persist_settings(&next)?;
    if let Ok(mut current) = state.settings.lock() {
        *current = next.clone();
    }
    Ok(bootstrap_impl(&next))
}

const LAUNCH_HELPER_BYTES: &[u8] = include_bytes!("../../../helpers/nte-launch-helper.exe");

fn ensure_launch_helper() -> Result<PathBuf, String> {
    let dir = data_dir().join("helpers");
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join("nte-launch-helper.exe");
    let current = fs::read(&path).ok();
    if current.as_deref() != Some(LAUNCH_HELPER_BYTES) {
        let tmp = dir.join("nte-launch-helper.exe.new");
        fs::write(&tmp, LAUNCH_HELPER_BYTES)
            .map_err(|e| format!("write {}: {e}", tmp.display()))?;
        fs::rename(&tmp, &path).map_err(|e| format!("install {}: {e}", path.display()))?;
    }
    Ok(path)
}

/// Convert a Unix absolute path to a Wine `Z:` path. `Z:` maps `/` in the
/// standard prefix and avoids drive-letter assumptions for removable mounts.
fn windows_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut out = String::from("Z:");
    if !raw.starts_with('/') {
        out.push('\\');
    }
    for ch in raw.chars() {
        out.push(if ch == '/' { '\\' } else { ch });
    }
    out
}

/// Official NTEGame settings that make the launcher behave like a thin login
/// helper: no automatic updates, no "popup after game exit", quit when the game
/// exits, and quit (rather than minimize to tray) when its close button is used.
/// Only [Setting] and [UPDATER] are touched; account/token sections are left
/// byte-for-byte intact. A one-time backup is written next to the file.
const PRESENCE_SETTINGS: [(&str, &str); 4] = [
    ("autoUpdate", "0"),
    ("quitWithGame", "1"),
    ("showAfterGameQuit", "0"),
    ("closeAction", "0"),
];
const PRESENCE_UPDATER: [(&str, &str); 1] = [("NeedUpdate", "0")];

fn line_ending(line: &str, fallback: &str) -> &'static str {
    if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        // A last line without a newline must not swallow the keys we append.
        if fallback == "\r\n" {
            "\r\n"
        } else {
            "\n"
        }
    }
}

fn append_presence_keys(
    out: &mut String,
    section: &str,
    seen_setting: &mut Vec<&'static str>,
    seen_updater: &mut Vec<&'static str>,
    eol: &str,
) {
    if section.eq_ignore_ascii_case("setting") {
        for (key, value) in PRESENCE_SETTINGS {
            if !seen_setting.contains(&key) {
                out.push_str(&format!("{key}={value}{eol}"));
                seen_setting.push(key);
            }
        }
    } else if section.eq_ignore_ascii_case("updater") {
        for (key, value) in PRESENCE_UPDATER {
            if !seen_updater.contains(&key) {
                out.push_str(&format!("{key}={value}{eol}"));
                seen_updater.push(key);
            }
        }
    }
}

/// Pure text transform, kept separate so the INI-preserving behavior is testable.
fn apply_presence_settings(text: &str) -> String {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(text.len() + 128);
    let mut section = String::new();
    let mut setting_seen = false;
    let mut updater_seen = false;
    let mut seen_setting: Vec<&'static str> = Vec::new();
    let mut seen_updater: Vec<&'static str> = Vec::new();

    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']).trim();
        if body.starts_with('[') && body.ends_with(']') {
            append_presence_keys(
                &mut out,
                &section,
                &mut seen_setting,
                &mut seen_updater,
                eol,
            );
            section = body[1..body.len() - 1].trim().to_string();
            if section.eq_ignore_ascii_case("setting") {
                setting_seen = true;
            }
            if section.eq_ignore_ascii_case("updater") {
                updater_seen = true;
            }
            out.push_str(line);
            continue;
        }

        if let Some((key, _)) = body.split_once('=') {
            let key_trim = key.trim();
            if section.eq_ignore_ascii_case("setting") {
                if let Some((matched, value)) = PRESENCE_SETTINGS
                    .iter()
                    .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key_trim))
                {
                    let ending = line_ending(line, eol);
                    out.push_str(&format!("{key_trim}={value}{ending}"));
                    if !seen_setting.contains(matched) {
                        seen_setting.push(matched);
                    }
                    continue;
                }
            } else if section.eq_ignore_ascii_case("updater") {
                if let Some((matched, value)) = PRESENCE_UPDATER
                    .iter()
                    .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key_trim))
                {
                    let ending = line_ending(line, eol);
                    out.push_str(&format!("{key_trim}={value}{ending}"));
                    if !seen_updater.contains(matched) {
                        seen_updater.push(matched);
                    }
                    continue;
                }
            }
        }

        out.push_str(line);
    }

    append_presence_keys(
        &mut out,
        &section,
        &mut seen_setting,
        &mut seen_updater,
        eol,
    );

    if !setting_seen {
        out.push_str(&format!("[Setting]{eol}"));
        for (key, value) in PRESENCE_SETTINGS {
            out.push_str(&format!("{key}={value}{eol}"));
        }
    }
    if !updater_seen {
        out.push_str(&format!("[UPDATER]{eol}"));
        for (key, value) in PRESENCE_UPDATER {
            out.push_str(&format!("{key}={value}{eol}"));
        }
    }
    out
}

/// Apply the presence settings to the launcher that is about to run. A missing
/// config is left alone (the official launcher will create defaults); a write
/// failure is reported but must not block the launch.
fn prepare_launcher_presence(launcher_dir: &Path) -> Result<(), String> {
    let config = launcher_dir
        .join("UserData")
        .join("Config")
        .join("Config.ini");
    let original = match fs::read_to_string(&config) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("read {}: {error}", config.display())),
    };
    let updated = apply_presence_settings(&original);
    if updated == original {
        return Ok(());
    }

    let backup = config.with_file_name("Config.ini.nte-presence.bak");
    if !backup.exists() {
        fs::write(&backup, &original)
            .map_err(|error| format!("write {}: {error}", backup.display()))?;
        // The config may contain an opaque account blob; keep the backup private.
        let _ = fs::set_permissions(&backup, fs::Permissions::from_mode(0o600));
    }
    let tmp = config.with_file_name("Config.ini.nte-presence.tmp");
    fs::write(&tmp, updated).map_err(|error| format!("write {}: {error}", tmp.display()))?;
    let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    fs::rename(&tmp, &config).map_err(|error| format!("rename {}: {error}", config.display()))?;
    Ok(())
}

/// Hide the Wine systray host item (the extra official-launcher icon in the
/// Linux tray) without killing explorer.exe. Wine reads the Windows
/// NoTrayItemsDisplay policy when its shell starts; setting it for the
/// dedicated prefix suppresses the host tray icon while auto-login and the
/// game windows keep working. Verified with GE-Proton 11-5 / Wine 11.0.
fn hide_wine_tray_items(proton: &ProtonInstall, prefix: &Path) -> Result<(), String> {
    let wine = proton.root.join("files").join("bin").join("wine");
    if !wine.is_file() {
        return Err(format!("未找到 {}", wine.display()));
    }
    let status = Command::new(&wine)
        .args([
            "reg",
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Policies\Explorer",
            "/v",
            "NoTrayItemsDisplay",
            "/t",
            "REG_DWORD",
            "/d",
            "1",
            "/f",
        ])
        .env("WINEPREFIX", prefix)
        .env("WINEDEBUG", "-all")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("执行 wine reg 失败：{error}"))?;
    if !status.success() {
        return Err(format!("wine reg 返回 {status}"));
    }
    Ok(())
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("只允许打开 http(s) 链接".into());
    }
    Command::new("xdg-open")
        .arg(&url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("打开链接失败：{e}"))?;
    Ok(())
}

fn launch_impl(settings: &Settings) -> Result<LaunchInvocation, String> {
    let launcher_root = detect_launcher_root(settings)
        .ok_or_else(|| "尚未找到《异环》启动器安装目录".to_string())?;
    let install_dir = launcher_root
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "安装目录结构异常".to_string())?;
    let bridge_root = bridge_root_for(settings, &launcher_root);
    let bridge_exe = bridge_root.join("NTELauncher").join("NTEGame.exe");
    let direct_exe = game_exe_for(&launcher_root);
    let prefix = settings
        .prefix
        .as_deref()
        .map(expand_path)
        .unwrap_or_else(default_prefix);
    let umu = which("umu-run").ok_or_else(|| "PATH 中未找到 umu-run".to_string())?;
    let proton = detect_proton(settings)
        .ok_or_else(|| "未找到可用的 Proton（需要 proton 与 toolmanifest.vdf）".to_string())?;
    if !prefix.join("drive_c").is_dir() {
        return Err(format!("Wine prefix 尚未初始化：{}", prefix.display()));
    }
    ensure_sdk_link(&prefix, &install_dir)?;

    let (mode, exe, args, display_exe, helper_env, bridge_env) = if settings.launch_mode == "direct"
    {
        let game_exe = direct_exe.ok_or_else(|| "未找到 HTGame.exe".to_string())?;
        if !game_exe.is_file() {
            return Err(format!("游戏可执行文件不存在：{}", game_exe.display()));
        }
        let helper = ensure_launch_helper()?;
        let win_exe = windows_path(&game_exe);
        let win_cwd = windows_path(
            game_exe
                .parent()
                .ok_or_else(|| "HTGame.exe 没有父目录".to_string())?,
        );
        (
            "direct".to_string(),
            helper,
            Vec::new(),
            game_exe,
            Some((win_exe, win_cwd)),
            None,
        )
    } else if settings.launch_mode == "background" {
        let official = if bridge_exe.is_file() {
            bridge_exe
        } else {
            launcher_root.join("NTEGame.exe")
        };
        if !official.is_file() {
            return Err(format!("官方启动器不存在：{}", official.display()));
        }
        let helper = ensure_launch_helper()?;
        let win_exe = windows_path(&official);
        let win_cwd = windows_path(
            official
                .parent()
                .ok_or_else(|| "NTEGame.exe 没有父目录".to_string())?,
        );
        (
            "background".to_string(),
            helper,
            Vec::new(),
            official,
            None,
            Some((win_exe, win_cwd)),
        )
    } else {
        let exe = if bridge_exe.is_file() {
            bridge_exe
        } else {
            launcher_root.join("NTEGame.exe")
        };
        if !exe.is_file() {
            return Err(format!("官方启动器不存在：{}", exe.display()));
        }
        (
            "official".to_string(),
            exe.clone(),
            vec![
                "/launcher".to_string(),
                "/directly".to_string(),
                "/autoplay".to_string(),
            ],
            exe,
            None,
            None,
        )
    };

    let log_dir = data_dir().join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| format!("create log dir: {e}"))?;
    let log_path = log_dir.join("nte-app-launch.log");
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("open {}: {e}", log_path.display()))?;
    let mut log_clone = log.try_clone().map_err(|e| e.to_string())?;
    writeln!(
        log_clone,
        "\n=== oddity launch {mode} ===\ntarget={} runner={} proton={} prefix={}",
        display_exe.display(),
        exe.display(),
        proton.name,
        prefix.display()
    )
    .ok();

    if mode != "direct" && std::env::var_os("NTE_DISABLE_PRESENCE_TUNING").is_none() {
        if let Some(dir) = display_exe.parent() {
            match prepare_launcher_presence(dir) {
                Ok(()) => {
                    writeln!(
                        log_clone,
                        "presence settings: autoUpdate=0 quitWithGame=1 \
                         showAfterGameQuit=0 closeAction=0 NeedUpdate=0"
                    )
                    .ok();
                }
                Err(error) => {
                    writeln!(log_clone, "presence settings not applied: {error}").ok();
                }
            }
        }
        match hide_wine_tray_items(&proton, &prefix) {
            Ok(()) => {
                writeln!(
                    log_clone,
                    "wine tray: NoTrayItemsDisplay=1 (official/Wine item hidden)"
                )
                .ok();
            }
            Err(error) => {
                writeln!(log_clone, "wine tray not hidden: {error}").ok();
            }
        }
    }

    let mut cmd = Command::new(&umu);
    cmd.arg(&exe).args(&args);
    cmd.env("WINEPREFIX", &prefix)
        .env("PROTONPATH", &proton.root)
        .env("GAMEID", "0")
        .env("LauncherStarted", "1")
        .env("pgp_scale_value", "1")
        .env("UMU_LOG", "info")
        .env("PROTON_LOG", "0")
        .env_remove("PROTON_ENABLE_WAYLAND");
    if let Some((win_exe, win_cwd)) = &helper_env {
        // The helper uses CreateProcessW(lpApplicationName=exe, lpCommandLine=args)
        // so UE does not mistake the executable path for the map URL.
        cmd.env("NTE_GAME_EXE", win_exe)
            .env("NTE_GAME_CWD", win_cwd)
            .env(
                "NTE_GAME_ARGS",
                "/Game/LoginAndCreate/Map/Updater/Updater_P -SAVEWINPOS=1",
            );
    }
    if let Some((win_exe, win_cwd)) = &bridge_env {
        // Keep the official login/pipe handoff. The helper now runs a monitor
        // thread: it hides launcher-owned windows but explicitly preserves the
        // newautologin.qml cover (class ...ToolSaveBitsOwnDC), instead of
        // hiding every visible window once after a fixed Sleep.
        cmd.env("NTE_BRIDGE_EXE", win_exe)
            .env("NTE_BRIDGE_CWD", win_cwd)
            .env("NTE_BRIDGE_ARGS", "/launcher /directly /autoplay")
            .env(
                "NTE_HELPER_LOG",
                windows_path(&log_dir.join("helper-window.log")),
            );
    }
    cmd.stdout(Stdio::from(log_clone))
        .stderr(Stdio::from(log))
        .current_dir(install_dir);
    cmd.process_group(0);
    let mut child = cmd.spawn().map_err(|e| format!("启动游戏失败：{e}"))?;
    let pid = child.id();
    let pgid = i32::try_from(pid).map_err(|_| "游戏进程 ID 超出范围".to_string())?;

    // umu-run exits immediately when the runtime or PROTONPATH is invalid.
    // Catch that short window so the UI can show the actual log line instead
    // of the generic "process exited" message.
    let immediate_exit = {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {}
                Err(error) => return Err(format!("检查启动器状态失败：{error}")),
            }
            if Instant::now() >= deadline {
                break None;
            }
            thread::sleep(Duration::from_millis(25));
        }
    };
    if let Some(status) = immediate_exit {
        let code = status
            .code()
            .map(|code| format!("退出码 {code}"))
            .unwrap_or_else(|| "被信号终止".to_string());
        let label = match mode.as_str() {
            "direct" => "游戏启动",
            "background" => "后台启动",
            _ => "官方启动器启动",
        };
        return Err(format!(
            "{label}失败（{code}）{}",
            launch_log_tail(&log_path)
        ));
    }

    Ok(LaunchInvocation {
        result: LaunchResult {
            mode,
            pid,
            exe: display_exe.display().to_string(),
            prefix: prefix.display().to_string(),
            log: log_path.display().to_string(),
            started_at: now_millis(),
        },
        child,
        pgid,
    })
}

#[tauri::command]
async fn launch_official(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<LaunchResult, String> {
    let settings = state
        .settings
        .lock()
        .map(|s| s.clone())
        .map_err(|e| e.to_string())?;

    {
        let mut tracker = state.game.lock().map_err(|e| e.to_string())?;
        if tracker.starting {
            return Err("游戏正在启动，请稍候。".to_string());
        }
        if let Some(session) = tracker.session.as_ref() {
            if session.closing {
                return Err("游戏正在关闭，请稍候。".to_string());
            }
            if process_group_alive(session.pgid) {
                return Err("游戏已在运行。".to_string());
            }
        }
        // A session whose process group disappeared without the monitor
        // clearing it yet is stale; replace it instead of blocking a relaunch.
        tracker.session = None;
        tracker.starting = true;
    }

    let invocation =
        match tauri::async_runtime::spawn_blocking(move || launch_impl(&settings)).await {
            Ok(Ok(invocation)) => invocation,
            Ok(Err(error)) => {
                if let Ok(mut tracker) = state.game.lock() {
                    tracker.starting = false;
                }
                return Err(error);
            }
            Err(error) => {
                if let Ok(mut tracker) = state.game.lock() {
                    tracker.starting = false;
                }
                return Err(format!("启动任务失败：{error}"));
            }
        };

    let session = {
        let mut tracker = state
            .game
            .lock()
            .map_err(|e| format!("记录游戏进程失败：{e}"))?;
        tracker.starting = false;
        tracker.next_id = tracker.next_id.wrapping_add(1);
        let session = GameSession {
            id: tracker.next_id,
            pid: invocation.result.pid,
            pgid: invocation.pgid,
            exe: invocation.result.exe.clone(),
            started_at: invocation.result.started_at,
            log: invocation.result.log.clone(),
            closing: false,
        };
        tracker.session = Some(session.clone());
        session
    };

    emit_game_status(&app, &state);
    spawn_game_monitor(app, invocation.child, session);
    Ok(invocation.result)
}

#[tauri::command]
fn game_status(state: State<AppState>) -> GameStatus {
    let tracker = match state.game.lock() {
        Ok(tracker) => tracker,
        Err(_) => return GameStatus::idle(),
    };
    match tracker.session.as_ref() {
        Some(session) if process_group_alive(session.pgid) => {
            GameStatus::from_session(Some(session))
        }
        _ => GameStatus::idle(),
    }
}

#[tauri::command]
async fn close_game(app: AppHandle, state: State<'_, AppState>) -> Result<GameStatus, String> {
    let (session_id, pgid) = {
        let mut tracker = state.game.lock().map_err(|e| e.to_string())?;
        if tracker.starting {
            return Err("游戏正在启动，请稍候。".to_string());
        }
        match tracker.session.as_mut() {
            Some(session) if session.closing => {
                return Err("游戏正在关闭，请稍候。".to_string());
            }
            Some(session) => {
                if !process_group_alive(session.pgid) {
                    tracker.session = None;
                    drop(tracker);
                    let status = GameStatus::idle();
                    let _ = app.emit("game-state", status.clone());
                    return Ok(status);
                }
                session.closing = true;
                (session.id, session.pgid)
            }
            None => return Ok(GameStatus::idle()),
        }
    };

    emit_game_status(&app, &state);

    let result =
        match tauri::async_runtime::spawn_blocking(move || terminate_process_group(pgid)).await {
            Ok(result) => result,
            Err(error) => Err(format!("关闭任务失败：{error}")),
        };

    {
        let mut tracker = state.game.lock().map_err(|e| e.to_string())?;
        if tracker.session.as_ref().map(|session| session.id) == Some(session_id) {
            if result.is_ok() {
                tracker.session = None;
            } else if let Some(session) = tracker.session.as_mut() {
                session.closing = false;
            }
        }
    }

    let status = state
        .game
        .lock()
        .map(|tracker| GameStatus::from_session(tracker.session.as_ref()))
        .unwrap_or_else(|_| GameStatus::idle());
    let _ = app.emit("game-state", status.clone());
    result?;
    Ok(status)
}

#[tauri::command]
fn win_minimize(window: WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
fn win_toggle_maximize(window: WebviewWindow) -> Result<bool, String> {
    let maximized = window.is_maximized().map_err(|e| e.to_string())?;
    if maximized {
        window.unmaximize().map_err(|e| e.to_string())?;
    } else {
        window.maximize().map_err(|e| e.to_string())?;
    }
    Ok(!maximized)
}

#[tauri::command]
fn win_close(window: WebviewWindow) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

#[tauri::command]
fn win_hide(window: WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
fn win_drag(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
fn play_hero_video(window: tauri::WebviewWindow) -> Result<(), String> {
    // 由原生窗口启动解码, 避免被遮挡的视频源受到自动播放限制。
    window.eval("{ const video = document.getElementById('hero-video'); if (video instanceof HTMLVideoElement && video.autoplay) void video.play().catch(() => {}); }")
        .map_err(|error| error.to_string())
}

fn nvidia_driver_loaded() -> bool {
    Path::new("/sys/module/nvidia").exists()
}

fn is_wayland_session() -> bool {
    // GTK 后端可与桌面会话不同, 尤其是 Wayland 桌面中的 XWayland 窗口。
    match std::env::var("GDK_BACKEND").as_deref() {
        Ok("x11") => return false,
        Ok("wayland") => return true,
        _ => {}
    }
    std::env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

fn configure_graphics() {
    // 画布从不可见的视频源取帧, 使用内存视频帧避免 GL 视频层的拷贝与挂起。
    std::env::set_var("WEBKIT_GST_DISABLE_GL_SINK", "1");
    std::env::set_var("WEBKIT_GST_ALLOW_PLAYBACK_OF_INVISIBLE_VIDEOS", "1");
    // Hardware acceleration is the default. WebKitGTK's DMABUF renderer is the
    // accelerated path; keep an explicit escape hatch for broken drivers.
    if std::env::var_os("NTE_SOFTWARE_RENDER").is_some() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        eprintln!("graphics: WebKitGTK software renderer requested");
        return;
    }

    std::env::remove_var("WEBKIT_DMABUF_RENDERER_FORCE_SHM");
    std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER");
    std::env::remove_var("WEBKIT_DISABLE_COMPOSITING_MODE");
    std::env::remove_var("LIBGL_ALWAYS_SOFTWARE");

    if nvidia_driver_loaded() && is_wayland_session() {
        // WebKitGTK + proprietary NVIDIA + Wayland dies during startup with the
        // DMABUF renderer unless explicit sync is disabled (WebKit bug 280210).
        // This keeps the accelerated DMABUF path; it is not a software fallback.
        std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
        eprintln!("graphics: WebKitGTK hardware acceleration enabled (NVIDIA Wayland explicit-sync workaround)");
    } else if nvidia_driver_loaded() {
        // NVIDIA/X11 通过共享内存传帧, 避免 GBM 分配失败与旧渲染器的透明区域缺失。
        std::env::set_var("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1");
        eprintln!("graphics: WebKitGTK X11 NVIDIA workaround (shared-memory buffers)");
    } else {
        eprintln!("graphics: WebKitGTK hardware acceleration enabled");
    }
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn install_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主界面", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "隐藏到后台", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 Oddity", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &hide, &separator, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(tauri::include_image!("icons/tray-64.png"))
        .tooltip("Oddity")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn main() {
    if attendance_commands::headless_entry() {
        return;
    }
    migrate_legacy_dirs();
    configure_graphics();
    let settings = load_settings();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .manage(AppState {
            settings: Mutex::new(settings),
            game: Mutex::new(GameTracker::default()),
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let minimize = window
                    .try_state::<AppState>()
                    .and_then(|state| {
                        state
                            .settings
                            .lock()
                            .ok()
                            .map(|s| s.minimize_to_tray_on_close)
                    })
                    .unwrap_or(true);
                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            attendance_commands::on_launch();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_icon(tauri::include_image!("icons/icon.png"));
            }
            if let Err(error) = install_tray(app.handle()) {
                eprintln!("tray unavailable: {error}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            attendance_commands::attendance_status,
            attendance_commands::attendance_login,
            attendance_commands::attendance_send_code,
            attendance_commands::attendance_login_sms,
            attendance_commands::attendance_configure,
            attendance_commands::attendance_run,
            attendance_commands::attendance_logout,
            attendance_commands::attendance_keyring_check,
            play_hero_video,
            bootstrap,
            check_update,
            fetch_bulletins,
            save_settings,
            open_url,
            launch_official,
            game_status,
            close_game,
            win_minimize,
            win_toggle_maximize,
            win_close,
            win_hide,
            win_drag,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Oddity");
    let _ = APP_IDENTIFIER;
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
