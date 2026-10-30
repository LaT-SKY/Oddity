//! Oddity native Linux launcher shell.
//!
//! Tauri 2 宿主，内嵌 Vue 界面。nte-core 负责官方文件安装和资源更新，
//! 官方 Windows 组件通过 Proton 完成登录与游戏启动。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Runtime, State, WebviewWindow, WindowEvent};

use launch_progress::{LaunchProgress, LaunchStage};
use native_session::NativeSession;
use nte_core::bulletin::{self, BulletinFeed};
use nte_core::config::{Ini, LauncherConfig};
use nte_core::distribution::Distribution;
use nte_core::launch::background::{validated_click_profile, ClickProfile};
use nte_core::manifest::Manifest;
use nte_core::settings::{Settings, UserPaths};
use nte_core::{update, version};

mod attendance_commands;
mod background_server;
mod cli;
mod download_commands;
mod environment_commands;
mod file_commands;
mod game_mode_commands;
mod gamepad_commands;
mod launch_progress;
mod log_commands;
mod native_session;
mod playtime_commands;
mod snapshot_commands;
mod update_scheduler;

const APP_IDENTIFIER: &str = "io.github.oddity.Launcher";
const LAUNCH_CANCELLED: &str = "启动已暂停。";

struct AppState {
    settings: Mutex<Settings>,
    game: Mutex<GameTracker>,
}

#[derive(Default)]
struct GameTracker {
    environment_busy: bool,
    session: Option<GameSession>,
    starting: bool,
    next_id: u64,
    last_error: Option<String>,
    launch: Option<LaunchProgress>,
    launch_cancel: Option<Arc<AtomicBool>>,
}

/// 本轮启动及其专属后台会话.
#[derive(Debug, Clone)]
struct GameSession {
    id: u64,
    pid: u32,
    pgid: i32,
    exe: String,
    started_at: u64,
    log: String,
    mode: String,
    distribution: Distribution,
    closing: bool,
    exit_notified: bool,
    game_alive: Arc<AtomicBool>,
    monitor_done: Arc<AtomicBool>,
    finalized: Arc<AtomicBool>,
    _install_lock: Option<Arc<nte_core::install_lock::InstallLock>>,
    _prefix_lock: Option<Arc<nte_core::environment::PrefixLock>>,
    background: Option<Arc<NativeSession>>,
    game_mode: Option<Arc<Mutex<nte_core::game_mode::Session>>>,
    snapshot: Option<String>,
    snapshot_success_after_ms: u64,
    launcher_root: Option<PathBuf>,
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
    runtime_installations: Vec<environment_commands::Installation>,
    settings: Settings,
    app_version: String,
    os: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherBackgroundMedia {
    video: Option<String>,
    image: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckResult {
    installed: bool,
    distribution: String,
    install_root: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    launch: Option<LaunchProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
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
            launch: None,
            error: None,
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
                launch: None,
                error: None,
            },
            None => Self::idle(),
        }
    }
}

struct LaunchInvocation {
    prefix_lock: Arc<nte_core::environment::PrefixLock>,
    install_lock: Arc<nte_core::install_lock::InstallLock>,
    result: LaunchResult,
    child: Child,
    pgid: i32,
    background: Option<Arc<NativeSession>>,
    game_mode: Option<Arc<Mutex<nte_core::game_mode::Session>>>,
    snapshot: Option<String>,
    launcher_root: Option<PathBuf>,
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

/// 游戏进程结束后是否按设置显示主界面。
fn should_show_launcher_after_exit<R: tauri::Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<AppState>()
        .and_then(|state| {
            state
                .settings
                .lock()
                .ok()
                .map(|settings| settings.show_launcher_after_game_exit)
        })
        .unwrap_or(false)
}

/// 进程名是否指向游戏主进程。Windows 路径和 Wine 的 comm 都可能保留
/// HTGame 或 GameThread，其他参数里的同名文本不算。
fn is_game_process_name(text: &str) -> bool {
    let name = text
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(text)
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "htgame" | "htgame.exe" | "gamethread" | "gamethread.exe"
    )
}

/// 读取进程的父进程与状态，用于沿进程树寻找游戏主进程。
#[derive(Clone, Copy)]
struct ProcessStat {
    parent: u32,
    group: u32,
    state: char,
    started: u64,
}

fn process_stat(pid: u32) -> Option<ProcessStat> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, rest) = text.rsplit_once(')')?;
    let fields: Vec<_> = rest.split_whitespace().collect();
    Some(ProcessStat {
        state: fields.first()?.chars().next()?,
        parent: fields.get(1)?.parse().ok()?,
        group: fields.get(2)?.parse().ok()?,
        started: fields.get(19)?.parse().ok()?,
    })
}

fn is_game_process(pid: u32, state: char) -> bool {
    if state == 'Z' {
        return false;
    }
    if let Ok(comm) = fs::read(format!("/proc/{pid}/comm")) {
        if is_game_process_name(&String::from_utf8_lossy(&comm)) {
            return true;
        }
    }
    let Ok(cmdline) = fs::read(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    cmdline
        .split(|byte| *byte == 0)
        .filter(|argument| !argument.is_empty())
        .any(|argument| is_game_process_name(&String::from_utf8_lossy(argument)))
}

/// 记录进程身份, 父进程退出或进程组变化后仍跟踪已发现的游戏。
struct GameProcessWatch {
    root: u32,
    known: HashMap<u32, u64>,
}

impl GameProcessWatch {
    fn new(root: u32) -> Self {
        Self {
            root,
            known: HashMap::new(),
        }
    }

    fn alive(&mut self) -> bool {
        if self.root == 0 {
            return false;
        }
        let Ok(entries) = fs::read_dir("/proc") else {
            return true;
        };
        let states: HashMap<_, _> = entries
            .flatten()
            .filter_map(|entry| {
                let pid = entry.file_name().to_str()?.parse::<u32>().ok()?;
                Some((pid, process_stat(pid)?))
            })
            .collect();
        self.known.retain(|pid, started| {
            states
                .get(pid)
                .is_some_and(|s| s.state != 'Z' && s.started == *started)
        });
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut stack = vec![self.root];
        for (&pid, stat) in &states {
            children.entry(stat.parent).or_default().push(pid);
            if stat.group == self.root {
                stack.push(pid);
            }
        }
        let mut seen = HashSet::new();
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            if let Some(stat) = states.get(&pid) {
                if is_game_process(pid, stat.state) {
                    self.known.insert(pid, stat.started);
                }
            }
            if let Some(descendants) = children.get(&pid) {
                stack.extend(descendants);
            }
        }
        !self.known.is_empty()
    }
}

#[cfg(test)]
fn game_main_process_alive(root_pid: u32) -> bool {
    GameProcessWatch::new(root_pid).alive()
}

fn finish_game_early<R: tauri::Runtime>(app: &AppHandle<R>, session: &GameSession) {
    if let Some(mode) = &session.game_mode {
        if let Ok(mut mode) = mode.lock() {
            mode.release();
        }
    }
    let cleared = {
        let state = app.state::<AppState>();
        let mut tracker = match state.game.lock() {
            Ok(tracker) => tracker,
            Err(_) => return,
        };
        if let Some(current) = tracker
            .session
            .as_mut()
            .filter(|current| current.id == session.id && !current.exit_notified)
        {
            current.exit_notified = true;
            Some(tracker_status(&tracker))
        } else {
            None
        }
    };
    if let Some(status) = cleared {
        let _ = app.emit("game-state", status);
        if should_show_launcher_after_exit(app) {
            show_main(app);
        }
    }
}

/// 游戏退出后立即显示界面, 安装保护保留到后台清理结束。
fn watch_game_exit<R: tauri::Runtime>(
    app: AppHandle<R>,
    session: GameSession,
    done: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut seen = false;
        let mut watch = GameProcessWatch::new(session.pid);
        let playtime = app.try_state::<playtime_commands::Playtime>();
        let snapshots = app.try_state::<snapshot_commands::Snapshots>();
        let mut playtime_started = false;
        let mut snapshot_running_at: Option<u64> = None;
        let mut snapshot_success = false;
        let mut start_retry_at = Instant::now();
        let mut last_touch = Instant::now();
        while !done.load(Ordering::Acquire) {
            let game_running = watch.alive();
            let running = game_running
                || session
                    .background
                    .as_ref()
                    .is_some_and(|background| background.game_alive().unwrap_or(true));
            if let Some(mode) = &session.game_mode {
                if let Ok(mut mode) = mode.lock() {
                    if mode.needs_game_processes() {
                        let mut pids: Vec<_> = watch.known.keys().copied().collect();
                        if let Some(background) = &session.background {
                            if let Ok(background_pids) = background.game_pids() {
                                pids.extend(background_pids);
                            }
                        }
                        mode.update_game_processes(&pids);
                    }
                }
            }
            if game_running {
                if snapshot_running_at.is_none() {
                    let now = now_millis();
                    if let (Some(store), Some(id)) =
                        (snapshots.as_ref(), session.snapshot.as_deref())
                    {
                        store.mark_running(id, now);
                    }
                    snapshot_running_at = Some(now);
                }
                if let (Some(start), false) = (snapshot_running_at, snapshot_success) {
                    let now = now_millis();
                    if now.saturating_sub(start) >= session.snapshot_success_after_ms {
                        if let (Some(store), Some(id)) =
                            (snapshots.as_ref(), session.snapshot.as_deref())
                        {
                            let version = session
                                .launcher_root
                                .as_ref()
                                .map(|root| game_version_for(root))
                                .filter(|version| !version.is_empty());
                            snapshot_success = store.mark_success(
                                id,
                                now,
                                version,
                                session.snapshot_success_after_ms,
                            );
                        } else {
                            snapshot_success = true;
                        }
                    }
                }
            } else {
                snapshot_running_at = None;
            }
            if game_running && !playtime_started && Instant::now() >= start_retry_at {
                let started = if let Some(playtime) = &playtime {
                    match playtime.start(
                        playtime_commands::now_millis(),
                        &session.mode,
                        session.distribution.as_str(),
                    ) {
                        Ok(()) => true,
                        Err(error) => {
                            eprintln!("开始记录游玩时长失败: {error}");
                            start_retry_at = Instant::now() + Duration::from_secs(30);
                            false
                        }
                    }
                } else {
                    true
                };
                if started {
                    playtime_started = true;
                    last_touch = Instant::now();
                    // 会话创建晚于 running, 通知前端补读记录。
                    let _ = app.emit("playtime-started", ());
                }
            } else if playtime_started && !game_running {
                if let Some(playtime) = &playtime {
                    if let Err(error) = playtime.finish(playtime_commands::now_millis()) {
                        eprintln!("结束游玩记录失败: {error}");
                    }
                }
                playtime_started = false;
            } else if playtime_started && last_touch.elapsed() >= Duration::from_secs(30) {
                if let Some(playtime) = &playtime {
                    if let Err(error) = playtime.touch(playtime_commands::now_millis()) {
                        eprintln!("更新游玩记录心跳失败: {error}");
                    }
                }
                last_touch = Instant::now();
            }
            alive.store(running, Ordering::Release);
            if running {
                seen = true;
            } else if seen {
                if let (Some(store), Some(id)) = (snapshots.as_ref(), session.snapshot.as_deref()) {
                    store.finish(id, None);
                }
                finish_game_early(&app, &session);
                return;
            }
            thread::sleep(Duration::from_millis(500));
        }
        if playtime_started {
            if let Some(playtime) = &playtime {
                if let Err(error) = playtime.finish(playtime_commands::now_millis()) {
                    eprintln!("结束游玩记录失败: {error}");
                }
            }
        }
    });
}

fn session_alive(session: &GameSession) -> bool {
    session.background.as_ref().map_or_else(
        || process_group_alive(session.pgid) || session.game_alive.load(Ordering::Acquire),
        |background| !background.finished(),
    )
}

/// 会话只收尾一次: 停止监控, 完成快照、游玩记录和游戏模式清理.
fn finalize_game_session<R: Runtime>(
    app: &AppHandle<R>,
    session: &GameSession,
    failure: Option<nte_core::snapshots::Failure>,
) {
    if session.finalized.swap(true, Ordering::AcqRel) {
        return;
    }
    session.monitor_done.store(true, Ordering::Release);
    if let Some(snapshots) = app.try_state::<snapshot_commands::Snapshots>() {
        if let Some(id) = session.snapshot.as_deref() {
            snapshots.finish(id, failure);
        }
    }
    if let Some(playtime) = app.try_state::<playtime_commands::Playtime>() {
        if let Err(error) = playtime.finish(playtime_commands::now_millis()) {
            eprintln!("结束游玩记录失败: {error}");
        }
    }
    if let Some(mode) = &session.game_mode {
        if let Ok(mut mode) = mode.lock() {
            mode.release();
        }
    }
}

/// 后台会话按实际游戏生命周期结束, 无需等待共享 wineserver 退出.
fn spawn_game_monitor<R: Runtime>(app: AppHandle<R>, mut child: Child, session: GameSession) {
    let stage_app = app.clone();
    let session_id = session.id;
    thread::spawn(move || {
        let done = Arc::clone(&session.monitor_done);
        let alive = Arc::clone(&session.game_alive);
        watch_game_exit(
            app.clone(),
            session.clone(),
            Arc::clone(&done),
            Arc::clone(&alive),
        );
        let on_stage = |stage: LaunchStage| publish_launch_stage(&stage_app, session_id, stage);
        let (result, exit_code) = if let Some(background) = &session.background {
            if session.closing {
                // 取消请求可能早于监控线程启动, 这里补上会话关闭信号。
                (background.close(), None)
            } else {
                (background.monitor(&mut child, on_stage), None)
            }
        } else {
            let (result, exit_code) = match child.wait() {
                Ok(status) => (Ok(()), status.code()),
                Err(error) => (Err(error.to_string()), None),
            };
            while process_group_alive(session.pgid) || alive.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(120));
            }
            (result, exit_code)
        };
        if result.is_err()
            && session
                .background
                .as_ref()
                .is_some_and(|background| background.closing() && !background.finished())
        {
            // 清理失败时保留会话和监控, 由 close_game 重试并完成统一收尾。
            {
                let state = app.state::<AppState>();
                if let Ok(mut tracker) = state.game.lock() {
                    if tracker.session.as_ref().map(|current| current.id) == Some(session.id) {
                        if let Some(current) = tracker.session.as_mut() {
                            current.closing = false;
                        }
                        if let Some(launch) = tracker.launch.as_mut() {
                            launch.cancelling = false;
                        }
                    }
                }
                emit_game_status(&app, &state);
            }
            if child.try_wait().ok().flatten().is_none() {
                thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            return;
        }
        let mut failure = nte_core::snapshots::Failure::early_exit(exit_code);
        if let Some(error) = result.as_ref().err() {
            failure.detail = Some(error.clone());
        }
        finalize_game_session(&app, &session, Some(failure));

        if child.try_wait().ok().flatten().is_none() {
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
        let (status, exit_notified) = {
            let state = app.state::<AppState>();
            let mut tracker = match state.game.lock() {
                Ok(tracker) => tracker,
                Err(_) => return,
            };
            if tracker.session.as_ref().map(|current| current.id) != Some(session.id) {
                return;
            }
            let exit_notified = tracker
                .session
                .as_ref()
                .is_some_and(|session| session.exit_notified);
            tracker.last_error = result.err();
            if session
                .background
                .as_ref()
                .is_none_or(|background| background.finished())
            {
                tracker.session = None;
            } else if let Some(current) = tracker.session.as_mut() {
                current.closing = false;
            }
            clear_launch(&mut tracker);
            (tracker_status(&tracker), exit_notified)
        };
        let finished = !status.running;
        let _ = app.emit("game-state", status);
        if finished && !exit_notified && should_show_launcher_after_exit(&app) {
            show_main(&app);
        }
    });
}

fn tracker_status(tracker: &GameTracker) -> GameStatus {
    let mut status = GameStatus::from_session(tracker.session.as_ref());
    status.error = tracker.last_error.clone();
    status.launch = tracker.launch.clone();
    status
}

fn clear_launch(tracker: &mut GameTracker) {
    tracker.launch = None;
    tracker.launch_cancel = None;
}

fn emit_game_status<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let status = state
        .game
        .lock()
        .map(|tracker| tracker_status(&tracker))
        .unwrap_or_else(|_| GameStatus::idle());
    let _ = app.emit("game-state", status);
}

fn advance_launch(tracker: &mut GameTracker, session_id: u64, stage: LaunchStage) -> bool {
    if tracker.session.as_ref().map(|session| session.id) != Some(session_id) {
        return false;
    }
    tracker
        .launch
        .as_mut()
        .is_some_and(|launch| launch.advance(stage))
}

/// 监控线程只更新本轮会话的阶段, 防止迟到的旧回调覆盖新启动。
fn publish_launch_stage<R: Runtime>(app: &AppHandle<R>, session_id: u64, stage: LaunchStage) {
    let state = app.state::<AppState>();
    let changed = match state.game.lock() {
        Ok(mut tracker) => advance_launch(&mut tracker, session_id, stage),
        Err(_) => false,
    };
    if changed {
        emit_game_status(app, &state);
    }
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

fn install_seeds(base: &Path, distribution: Distribution, seeds: &mut Vec<PathBuf>) {
    for name in [
        distribution.default_install_name(),
        "Neverness To Everness",
        "Neverness to Everness",
    ] {
        for prefix in [
            base.join("Program Files (x86)"),
            base.join("Program Files"),
            base.to_path_buf(),
        ] {
            let candidate = prefix.join(name).join(distribution.launcher_dir());
            if !seeds.contains(&candidate) {
                seeds.push(candidate);
            }
        }
    }
    seeds.push(base.join(distribution.launcher_dir()));
}

fn data_base() -> PathBuf {
    UserPaths::discover().data_base
}

fn data_dir() -> PathBuf {
    UserPaths::discover().data_dir()
}

fn legacy_data_dir() -> PathBuf {
    data_base().join("TheBetterNTE")
}

fn config_base() -> PathBuf {
    UserPaths::discover().config_base
}

fn config_dir() -> PathBuf {
    UserPaths::discover().config_dir()
}

fn legacy_config_dir() -> PathBuf {
    config_base().join("TheBetterNTE")
}

/// One-time rename from the old project name. The old data directory is kept
/// as a symlink so absolute paths embedded in the Wine prefix and the official
/// launcher configs keep resolving.
///
/// Settings `prefix` is intentionally **not** rewritten: the official OneSDK
/// auto-login is sensitive to the exact Wine prefix path, so upgraded installs
/// keep using the legacy path through the symlink while new installs use the
/// canonical `oddity` path.
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
    UserPaths::discover().settings()
}

fn load_settings() -> Settings {
    let path = settings_path();
    match fs::read_to_string(&path) {
        Ok(text) => {
            let mut settings: Settings = serde_json::from_str(&text).unwrap_or_default();
            if settings.gamepad.validate().is_err() {
                settings.gamepad = Default::default();
            }
            settings.updates.normalize();
            settings.snapshot_success_minutes =
                nte_core::snapshots::normalize_success_minutes(settings.snapshot_success_minutes);
            settings
        }
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

fn configured_install_root(settings: &Settings) -> Option<&str> {
    match settings.distribution {
        Distribution::Cn => settings.install_root.as_deref(),
        Distribution::Global => settings.global_install_root.as_deref(),
    }
}

fn default_install_root(distribution: Distribution) -> PathBuf {
    home_dir()
        .join("Games")
        .join(distribution.default_install_name())
}

fn selected_install_root(settings: &Settings) -> PathBuf {
    let path = configured_install_root(settings)
        .map(expand_path)
        .unwrap_or_else(|| default_install_root(settings.distribution));
    if path
        .file_name()
        .is_some_and(|name| name == settings.distribution.launcher_dir())
    {
        path.parent().unwrap_or(&path).to_path_buf()
    } else {
        path
    }
}

/// 界面展示、检查和下载任务必须使用同一个安装根目录。
fn effective_install_root(settings: &Settings, launcher_root: Option<&Path>) -> PathBuf {
    if configured_install_root(settings).is_some() {
        return selected_install_root(settings);
    }
    launcher_root
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| selected_install_root(settings))
}

fn normalize_launcher_root_for(path: &Path, distribution: Distribution) -> Option<PathBuf> {
    let root = if path.join("Config").join("Config.ini").is_file() {
        path.to_path_buf()
    } else {
        let nested = path.join(distribution.launcher_dir());
        if !nested.join("Config").join("Config.ini").is_file() {
            return None;
        }
        nested
    };
    if root.file_name().is_some_and(|name| {
        [Distribution::Cn, Distribution::Global]
            .into_iter()
            .any(|other| other != distribution && name == other.launcher_dir())
    }) {
        return None;
    }
    let game_id = match LauncherConfig::load(&root) {
        Ok(config) => config.game_id,
        Err(_) => return Some(root),
    };
    let expected = distribution.game_id();
    (game_id == 0 || game_id == expected).then_some(root)
}

fn detect_launcher_root(settings: &Settings) -> Option<PathBuf> {
    // 已配置安装目录时只认该目录. 找不到就是未安装, 不能静默改用其他副本.
    if let Some(raw) = configured_install_root(settings) {
        return normalize_launcher_root_for(&expand_path(raw), settings.distribution);
    }

    for key in ["NTE_ROOT", "NTE_INSTALL"] {
        if let Some(raw) = std::env::var_os(key) {
            if let Some(root) = normalize_launcher_root_for(
                &expand_path(&raw.to_string_lossy()),
                settings.distribution,
            ) {
                return Some(root);
            }
        }
    }

    let mut seeds: Vec<PathBuf> = vec![
        default_install_root(settings.distribution).join(settings.distribution.launcher_dir())
    ];
    install_seeds(&home_dir().join("Games"), settings.distribution, &mut seeds);
    if settings.distribution == Distribution::Cn {
        seeds.push(
            home_dir()
                .join("Games/异环")
                .join(settings.distribution.launcher_dir()),
        );
    }

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            install_seeds(ancestor, settings.distribution, &mut seeds);
        }
    }

    // Removable/extra drives: /run/media and /media hold per-user directories,
    // while /mnt usually holds mount points directly. Probe both levels so the
    // install is discovered regardless of the mount convention.
    for mount_root in ["/run/media", "/media", "/mnt"] {
        if let Ok(entries) = fs::read_dir(mount_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                install_seeds(&path, settings.distribution, &mut seeds);
                if let Ok(children) = fs::read_dir(&path) {
                    for child in children.flatten() {
                        install_seeds(&child.path(), settings.distribution, &mut seeds);
                    }
                }
            }
        }
    }

    for seed in seeds {
        if let Some(root) = normalize_launcher_root_for(&seed, settings.distribution) {
            return Some(root);
        }
    }
    None
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
    environment_commands::proton_path(settings).and_then(proton_install)
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

fn check_game_write_access(game_exe: &Path) -> Result<(), String> {
    let bin = game_exe
        .parent()
        .ok_or_else(|| "游戏可执行文件目录异常".to_string())?;
    for (path, mode) in [
        (bin.to_path_buf(), libc::W_OK | libc::X_OK),
        (bin.join("GameLauncher.config"), libc::W_OK),
        (bin.join("patchersdk_serverlist"), libc::W_OK | libc::X_OK),
    ] {
        match fs::metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("读取游戏目录权限失败: {}: {error}", path.display())),
        }
        let raw = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| "游戏路径含有无效字符".to_string())?;
        // 官方组件需要写入 SDK 路径和服务器缓存, 只读文件会让游戏在内部失败.
        if unsafe { libc::access(raw.as_ptr(), mode) } != 0 {
            return Err(format!(
                "游戏运行配置不可写: {}. 请检查此路径的所有者和写入权限后重试.",
                path.display()
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn bootstrap_impl(settings: &Settings) -> Bootstrap {
    let mut error: Option<String> = None;
    let launcher_root = detect_launcher_root(settings);
    let install_root = effective_install_root(settings, launcher_root.as_deref());
    let prefix = match environment_commands::prefix(settings) {
        Ok(prefix) => prefix,
        Err(message) => {
            error = Some(message);
            PathBuf::new()
        }
    };
    let prefix_exists = !prefix.as_os_str().is_empty() && prefix.join("drive_c").is_dir();

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
    } else if configured_install_root(settings).is_some() {
        error = Some(format!(
            "所选安装目录尚未安装启动器文件: {}. 可在游戏更新页开始安装.",
            install_root.display()
        ));
    } else {
        error = Some("尚未找到所选游戏地区的游戏安装. 可在游戏更新页开始安装.".to_string());
    }

    let game_version = launcher_root
        .as_ref()
        .map(|p| game_version_for(p))
        .unwrap_or_default();
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
    if let Err(message) = environment_commands::verify_proton(settings) {
        error.get_or_insert(message);
    }
    let proton = detect_proton(settings);
    let umu = which("umu-run");
    let official_exe = launcher_root
        .as_ref()
        .map(|root| root.join(settings.distribution.client_exe()));
    let official_ready = official_exe.as_ref().is_some_and(|path| path.is_file());

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
        label: "官方启动器".into(),
        ok: official_ready,
        detail: official_exe
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "未找到".into()),
    });

    Bootstrap {
        ok: error.is_none() && launcher_root.is_some(),
        error,
        install_root: install_root.display().to_string(),
        launcher_root: launcher_root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
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
        runtime_installations: environment_commands::installations(settings),
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

fn check_impl(
    launcher_root: &Path,
    install_root: &Path,
    distribution: Distribution,
    full: bool,
) -> Result<CheckResult, String> {
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
        installed: true,
        distribution: distribution.as_str().into(),
        install_root: install_root.display().to_string(),
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

fn launcher_background_paths(
    settings: &Settings,
) -> Option<nte_core::background::LauncherBackground> {
    let root = detect_launcher_root(settings)?;
    nte_core::background::resolve(&root)
}

#[tauri::command]
fn launcher_background(
    state: State<AppState>,
    server: State<background_server::BackgroundServer>,
) -> LauncherBackgroundMedia {
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let Some(background) = launcher_background_paths(&settings) else {
        let _ = server.update(None, None);
        return LauncherBackgroundMedia::default();
    };
    let video = background
        .video
        .and_then(|path| fs::canonicalize(path).ok());
    let image = background
        .image
        .and_then(|path| fs::canonicalize(path).ok());
    let (video, image) = server.update(video, image);
    LauncherBackgroundMedia { video, image }
}

#[tauri::command]
async fn check_update(state: State<'_, AppState>, full: bool) -> Result<CheckResult, String> {
    let settings = state
        .settings
        .lock()
        .map(|s| s.clone())
        .map_err(|e| e.to_string())?;
    let detected = detect_launcher_root(&settings);
    let install_root = effective_install_root(&settings, detected.as_deref());
    let Some(root) = detected else {
        return Ok(CheckResult {
            installed: false,
            distribution: settings.distribution.as_str().into(),
            install_root: install_root.display().to_string(),
            remote_version: String::new(),
            local_version: String::new(),
            product_version: String::new(),
            files: 0,
            ok: 0,
            missing: 0,
            changed: 0,
            bytes: 0,
            duration_ms: 0,
            missing_preview: Vec::new(),
            changed_preview: Vec::new(),
        });
    };
    let distribution = settings.distribution;
    tauri::async_runtime::spawn_blocking(move || {
        check_impl(&root, &install_root, distribution, full)
    })
    .await
    .map_err(|e| format!("校验任务失败：{e}"))?
}

#[tauri::command]
async fn fetch_bulletins(distribution: Distribution) -> Result<BulletinFeed, String> {
    tauri::async_runtime::spawn_blocking(move || bulletin::fetch_for(distribution))
        .await
        .map_err(|error| format!("公告抓取任务失败：{error}"))
}

/// 下载任务只依赖游戏地区和两个地区的安装目录.
fn download_paths_changed(current: &Settings, next: &Settings) -> bool {
    current.distribution != next.distribution
        || current.install_root != next.install_root
        || current.global_install_root != next.global_install_root
}

#[tauri::command]
fn save_settings(
    state: State<AppState>,
    downloads: State<download_commands::DownloadManager>,
    settings: Settings,
) -> Result<Bootstrap, String> {
    let guard = downloads.inner.lock().map_err(|error| error.to_string())?;
    let mut next = settings;
    next.snapshot_success_minutes =
        nte_core::snapshots::normalize_success_minutes(next.snapshot_success_minutes);
    next.gamepad.validate()?;
    next.game_mode
        .gamescope
        .validate()
        .map_err(|error| error.to_string())?;
    next.updates.validate()?;
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
    let mut current = state.settings.lock().map_err(|error| error.to_string())?;
    let game = state.game.lock().map_err(|error| error.to_string())?;
    let busy = game.environment_busy
        || game.starting
        || game
            .session
            .as_ref()
            .is_some_and(|session| session.closing || session_alive(session));
    environment_commands::validate_settings_change(&current, &next, busy)?;
    if download_commands::blocks_changes(&guard.status) && download_paths_changed(&current, &next) {
        return Err("游戏资源任务正在进行, 暂时无法修改游戏地区或安装目录".into());
    }
    persist_settings(&next)?;
    *current = next.clone();
    drop(game);
    drop(current);
    drop(guard);
    Ok(bootstrap_impl(&next))
}

const WINDOW_GUARD_BYTES: &[u8] = include_bytes!("../../../helpers/nte-window-guard.dll");
const LAUNCH_HELPER_BYTES: &[u8] = include_bytes!("../../../helpers/nte-launch-helper.exe");
const SESSION_HELPER_BYTES: &[u8] = include_bytes!("../../../helpers/nte-session-helper.exe");

fn ensure_launch_helper() -> Result<PathBuf, String> {
    ensure_helper("nte-window-guard.dll", WINDOW_GUARD_BYTES)?;
    ensure_helper("nte-launch-helper.exe", LAUNCH_HELPER_BYTES)
}

fn ensure_helper(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = data_dir().join("helpers");
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join(name);
    let current = fs::read(&path).ok();
    if current.as_deref() != Some(bytes) {
        let tmp = dir.join(format!("{name}.new"));
        fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
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

/// 保留官方资源初始化, 游戏退出后关闭官方后台.
const PRESENCE_SETTINGS: [(&str, &str); 4] = [
    ("autoUpdate", "1"),
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

fn open_directory_impl(path: &Path) -> Result<(), String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("读取文件夹失败: {error}"))?;
    if !path.is_dir() {
        return Err("指定路径不是文件夹".into());
    }
    let status = Command::new("xdg-open")
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("打开文件夹失败: {error}"))?;
    if !status.success() {
        return Err(format!("系统默认文件管理器返回 {status}"));
    }
    Ok(())
}

#[tauri::command]
async fn open_directory(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_directory_impl(Path::new(&path)))
        .await
        .map_err(|error| format!("打开文件夹任务失败: {error}"))?
}

#[cfg(test)]
fn launch_impl(settings: &Settings) -> Result<LaunchInvocation, String> {
    launch_impl_with_profile(settings, |path| {
        validated_click_profile(path).map_err(|error| error.to_string())
    })
}

#[cfg(test)]
fn launch_impl_with_cancel(
    settings: &Settings,
    cancel: &AtomicBool,
) -> Result<LaunchInvocation, String> {
    launch_impl_inner(
        settings,
        |path| validated_click_profile(path).map_err(|error| error.to_string()),
        None,
        cancel,
    )
}

fn launch_impl_with_snapshots(
    settings: &Settings,
    snapshots: &snapshot_commands::Snapshots,
    cancel: &AtomicBool,
) -> Result<LaunchInvocation, String> {
    launch_impl_inner(
        settings,
        |path| validated_click_profile(path).map_err(|error| error.to_string()),
        Some(snapshots),
        cancel,
    )
}

#[cfg(test)]
fn launch_impl_with_profile(
    settings: &Settings,
    profile_provider: impl FnOnce(&Path) -> Result<ClickProfile, String>,
) -> Result<LaunchInvocation, String> {
    launch_impl_inner(settings, profile_provider, None, &AtomicBool::new(false))
}

fn launch_impl_inner(
    settings: &Settings,
    profile_provider: impl FnOnce(&Path) -> Result<ClickProfile, String>,
    snapshots: Option<&snapshot_commands::Snapshots>,
    cancel: &AtomicBool,
) -> Result<LaunchInvocation, String> {
    if cancel.load(Ordering::Acquire) {
        return Err(LAUNCH_CANCELLED.into());
    }
    let launcher_root = detect_launcher_root(settings).ok_or_else(|| {
        if configured_install_root(settings).is_some() {
            format!(
                "所选安装目录尚未安装启动器文件: {}. 可在游戏更新页开始安装.",
                selected_install_root(settings).display()
            )
        } else {
            "尚未找到《异环》启动器安装目录".to_string()
        }
    })?;
    let install_dir = effective_install_root(settings, Some(&launcher_root));
    let install_lock = Arc::new(
        nte_core::install_lock::InstallLock::acquire(&install_dir)
            .map_err(|error| error.to_string())?,
    );
    let official_exe = launcher_root.join(settings.distribution.client_exe());
    let direct_exe = game_exe_for(&launcher_root);
    let prefix = environment_commands::prefix(settings)?;
    environment_commands::verify_proton(settings)?;
    let umu = which("umu-run").ok_or_else(|| "PATH 中未找到 umu-run".to_string())?;
    let proton = detect_proton(settings)
        .ok_or_else(|| "未找到可用的 Proton（需要 proton 与 toolmanifest.vdf）".to_string())?;
    if !prefix.join("drive_c").is_dir() {
        return Err(format!("Wine prefix 尚未初始化：{}", prefix.display()));
    }
    let prefix_lock = Arc::new(
        nte_core::environment::lock_prefix(&prefix).map_err(|error| format!("{error:#}"))?,
    );
    if let Some(game_exe) = &direct_exe {
        check_game_write_access(game_exe)?;
    }
    ensure_sdk_link(&prefix, &install_dir)?;

    let (mode, exe, args, display_exe, helper_env, official_env) =
        if settings.launch_mode == "direct" {
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
            let official = official_exe;
            if !official.is_file() {
                return Err(format!("官方启动器不存在：{}", official.display()));
            }
            let helper = ensure_helper("nte-session-helper.exe", SESSION_HELPER_BYTES)?;
            let win_exe = windows_path(&official);
            let win_cwd = windows_path(
                official
                    .parent()
                    .ok_or_else(|| "官方启动器没有父目录".to_string())?,
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
            let exe = official_exe;
            if !exe.is_file() {
                return Err(format!("官方启动器不存在：{}", exe.display()));
            }
            (
                "official".to_string(),
                exe.clone(),
                vec!["/launcher".to_string(), "/directly".to_string()],
                exe,
                None,
                None,
            )
        };

    let log_dir = environment_commands::log_dir(settings)?;
    fs::create_dir_all(&log_dir).map_err(|e| format!("create log dir: {e}"))?;
    let background = if mode == "background" {
        let profile = profile_provider(&display_exe)?;
        let native_log = display_exe
            .parent()
            .ok_or("官方启动器没有父目录")?
            .join("UserData/Log")
            .join(format!(
                "{}.log",
                settings.distribution.client_exe().trim_end_matches(".exe")
            ));
        Some(NativeSession::prepare(
            &log_dir,
            &native_log,
            profile,
            settings.game_mode.gamescope.enabled,
        )?)
    } else {
        None
    };
    // 探测可能调用 glxinfo, 只在需要写入快照时执行.
    let snapshot_facts = snapshots.map(|_| {
        let mut inputs = snapshot_commands::current_inputs(settings);
        inputs.install_root = Some(install_dir.clone());
        inputs.launcher_root = Some(launcher_root.clone());
        inputs.prefix = Some(prefix.clone());
        inputs.prefix_ok = true;
        inputs.proton_name = Some(proton.name.clone());
        inputs.proton_ok = true;
        inputs.umu = Some(umu.clone());
        nte_core::snapshots::capture(inputs)
    });
    let log_path = log_dir.join("nte-app-launch.log");
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("open {}: {e}", log_path.display()))?;
    let mut log_clone = log.try_clone().map_err(|e| e.to_string())?;
    writeln!(
        log_clone,
        "\n=== oddity launch {mode} ===\ntarget={} runner={} proton={} prefix={}\ninstall={}",
        display_exe.display(),
        exe.display(),
        proton.name,
        prefix.display(),
        install_dir.display()
    )
    .ok();
    if let Some(facts) = &snapshot_facts {
        writeln!(log_clone, "{}", facts.log_line()).ok();
    }

    if let Some(background) = &background {
        writeln!(
            log_clone,
            "session_status={}",
            background.status_path().display()
        )
        .ok();
    }
    if mode != "direct" && std::env::var_os("NTE_DISABLE_PRESENCE_TUNING").is_none() {
        if let Some(dir) = display_exe.parent() {
            match prepare_launcher_presence(dir) {
                Ok(()) => {
                    writeln!(
                        log_clone,
                        "presence settings: autoUpdate=1 quitWithGame=1 \
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
    if cancel.load(Ordering::Acquire) {
        return Err(LAUNCH_CANCELLED.into());
    }

    let mut cmd = settings
        .game_mode
        .gamescope
        .command(&umu, settings.game_mode.overlay)
        .map_err(|error| error.to_string())?;
    cmd.arg(&exe).args(&args);
    cmd.env("WINEPREFIX", &prefix)
        .env("PROTONPATH", &proton.root)
        .env("GAMEID", "0")
        .env("LauncherStarted", "1")
        .env("pgp_scale_value", "1")
        .env("UMU_LOG", "info")
        .env("PROTON_LOG", "0")
        .env("PROTON_VERB", "run")
        .env_remove("PROTON_ENABLE_WAYLAND");
    for key in [
        "NTE_GAME_EXE",
        "NTE_GAME_CWD",
        "NTE_GAME_ARGS",
        "NTE_BRIDGE_EXE",
        "NTE_BRIDGE_CWD",
        "NTE_BRIDGE_ARGS",
        "NTE_HELPER_LOG",
        "NTE_SESSION_HELPER",
        "NTE_SESSION_OUTPUT",
        "NTE_SESSION_SIGNAL",
        "NTE_SESSION_CANCEL",
        "NTE_SESSION_CLICK",
    ] {
        cmd.env_remove(key);
    }
    if let Some(background) = &background {
        background.configure(&mut cmd, &ensure_launch_helper()?);
    }
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
    if let Some((win_exe, win_cwd)) = &official_env {
        // NTE_BRIDGE_* 是 helper 的官方启动器交接协议, 与桥接目录设置无关.
        // 由官方正常启动按钮完成资源、登录和管道交接.
        cmd.env("NTE_BRIDGE_EXE", win_exe)
            .env("NTE_BRIDGE_CWD", win_cwd)
            .env("NTE_BRIDGE_ARGS", "/launcher /directly")
            .env(
                "NTE_HELPER_LOG",
                windows_path(&log_dir.join("helper-window.log")),
            );
    }
    cmd.stdout(Stdio::from(log_clone))
        .stderr(Stdio::from(log))
        .current_dir(install_dir);
    cmd.process_group(0);
    let game_mode = Arc::new(Mutex::new(nte_core::game_mode::Session::start(
        &settings.game_mode,
        &mut cmd,
    )));
    let snapshot_id = match (snapshots, &snapshot_facts) {
        (Some(store), Some(facts)) => match store.begin(facts.clone()) {
            Ok(id) => Some(id),
            Err(error) => {
                eprintln!("记录环境快照失败: {error}");
                None
            }
        },
        _ => None,
    };
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => {
            if let (Some(store), Some(id)) = (snapshots, snapshot_id.as_deref()) {
                store.fail(id, nte_core::snapshots::Failure::spawn(error.to_string()));
            }
            return Err(format!("启动游戏失败：{error}"));
        }
    };
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
                Err(_) if background.is_some() => break None,
                Err(error) => return Err(format!("检查启动器状态失败：{error}")),
            }
            if Instant::now() >= deadline {
                break None;
            }
            thread::sleep(Duration::from_millis(25));
        }
    };
    // 后台包装进程早退时仍交由会话监控清理已分离的 Windows 子进程.
    if let Some(status) = immediate_exit.filter(|_| background.is_none()) {
        let exit_code = status.code();
        if let (Some(store), Some(id)) = (snapshots, snapshot_id.as_deref()) {
            store.fail(id, nte_core::snapshots::Failure::early_exit(exit_code));
        }
        let code = exit_code
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
        prefix_lock,
        install_lock,
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
        background,
        game_mode: Some(game_mode),
        snapshot: snapshot_id,
        launcher_root: Some(launcher_root),
    })
}

fn launch_settings(state: &AppState) -> Result<Settings, String> {
    state
        .settings
        .lock()
        .map_err(|error| error.to_string())
        .map(|settings| settings.clone())
}

/// 初始化本轮启动的进度与取消令牌, 只有后台模式需要进度。
fn begin_launch(tracker: &mut GameTracker, settings: &Settings) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    tracker.starting = true;
    let background = settings.launch_mode == "background";
    tracker.launch = background.then(|| LaunchProgress::new(now_millis()));
    tracker.launch_cancel = background.then(|| Arc::clone(&cancel));
    cancel
}

fn reset_launch(tracker: &mut GameTracker) {
    tracker.starting = false;
    clear_launch(tracker);
}

#[tauri::command]
async fn launch_official(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<LaunchResult, String> {
    let settings = launch_settings(&state)?;
    start_launch(app, settings).await
}

fn prepare_launch(
    tracker: &mut GameTracker,
    status: &download_commands::DownloadStatus,
) -> Result<(), String> {
    if download_commands::blocks_changes(status) {
        return Err("游戏资源任务正在进行，暂时无法启动游戏".into());
    }
    if tracker.environment_busy {
        return Err("运行环境任务正在进行, 请完成后再启动游戏".into());
    }
    if tracker.starting {
        return Err("游戏正在启动，请稍候。".to_string());
    }
    if let Some(session) = tracker.session.as_ref() {
        if session.closing {
            return Err("游戏正在关闭，请稍候。".to_string());
        }
        if session_alive(session) {
            return Err("游戏已在运行。".to_string());
        }
    }
    // 已结束的会话允许重新启动.
    tracker.session = None;
    tracker.last_error = None;
    tracker.starting = true;
    Ok(())
}

/// 启动会话的公共入口. settings 可以带外部启动的临时模式覆盖, 不会写回设置.
async fn start_launch(app: AppHandle, settings: Settings) -> Result<LaunchResult, String> {
    let cancel;
    {
        let state = app.state::<AppState>();
        let downloads = app.state::<download_commands::DownloadManager>();
        let inner = downloads.inner.lock().map_err(|error| error.to_string())?;
        let mut tracker = state.game.lock().map_err(|e| e.to_string())?;
        prepare_launch(&mut tracker, &inner.status)?;
        cancel = begin_launch(&mut tracker, &settings);
    }
    {
        let state = app.state::<AppState>();
        emit_game_status(&app, &state);
    }
    let distribution = settings.distribution;
    let snapshot_success_after_ms =
        nte_core::snapshots::success_after_ms(settings.snapshot_success_minutes);
    let snapshots = app.state::<snapshot_commands::Snapshots>().inner().clone();

    let invocation = match tauri::async_runtime::spawn_blocking(move || {
        launch_impl_with_snapshots(&settings, &snapshots, &cancel)
    })
    .await
    {
        Ok(Ok(invocation)) => invocation,
        Ok(Err(error)) => {
            if let Ok(mut tracker) = app.state::<AppState>().game.lock() {
                reset_launch(&mut tracker);
            }
            let state = app.state::<AppState>();
            emit_game_status(&app, &state);
            return Err(error);
        }
        Err(error) => {
            if let Ok(mut tracker) = app.state::<AppState>().game.lock() {
                reset_launch(&mut tracker);
            }
            let state = app.state::<AppState>();
            emit_game_status(&app, &state);
            return Err(format!("启动任务失败：{error}"));
        }
    };

    let session = {
        let state = app.state::<AppState>();
        let mut tracker = state
            .game
            .lock()
            .map_err(|e| format!("记录游戏进程失败：{e}"))?;
        // 取消请求和会话创建在同一把锁下判定, 避免取消后仍进入会话.
        let cancelled = tracker
            .launch_cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Acquire));
        tracker.starting = false;
        tracker.launch_cancel = None;
        tracker.next_id = tracker.next_id.wrapping_add(1);
        let session = GameSession {
            id: tracker.next_id,
            pid: invocation.result.pid,
            pgid: invocation.pgid,
            exe: invocation.result.exe.clone(),
            started_at: invocation.result.started_at,
            log: invocation.result.log.clone(),
            mode: invocation.result.mode.clone(),
            distribution,
            closing: cancelled,
            exit_notified: false,
            game_alive: Arc::new(AtomicBool::new(true)),
            monitor_done: Arc::new(AtomicBool::new(false)),
            finalized: Arc::new(AtomicBool::new(false)),
            _install_lock: Some(Arc::clone(&invocation.install_lock)),
            _prefix_lock: Some(Arc::clone(&invocation.prefix_lock)),
            background: invocation.background.clone(),
            game_mode: invocation.game_mode.clone(),
            snapshot: invocation.snapshot.clone(),
            snapshot_success_after_ms,
            launcher_root: invocation.launcher_root.clone(),
        };
        if let Some(launch) = tracker.launch.as_mut() {
            launch.advance(LaunchStage::Helper);
            if cancelled {
                launch.cancelling = true;
            }
        }
        tracker.session = Some(session.clone());
        session
    };

    let state = app.state::<AppState>();
    emit_game_status(&app, &state);
    spawn_game_monitor(app, invocation.child, session);
    Ok(invocation.result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchOrigin {
    ColdStart,
    Forwarded,
}

async fn external_launch_impl(
    app: &AppHandle,
    request: &cli::LaunchRequest,
) -> Result<LaunchResult, String> {
    let state = app.state::<AppState>();
    let downloads = app.state::<download_commands::DownloadManager>();
    let selection = environment_commands::select_for_launch(
        &state,
        downloads.inner(),
        request.region,
        request.environment.as_deref(),
    )?;
    if selection.changed {
        let bootstrap = bootstrap_impl(&selection.settings);
        let _ = app.emit("settings-changed", bootstrap);
    }
    let mut settings = selection.settings;
    settings.launch_mode = request.mode.as_str().to_string();
    let result = start_launch(app.clone(), settings).await?;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    Ok(result)
}

async fn handle_external_launch(app: AppHandle, request: cli::LaunchRequest, origin: LaunchOrigin) {
    let summary = launch_failure_summary(&app, &request);
    if let Err(error) = external_launch_impl(&app, &request).await {
        eprintln!("外部启动失败: {error}");
        let body = notification_body(&error);
        let notified = tauri::async_runtime::spawn_blocking(move || {
            nte_core::notifications::send(&summary, &body)
        })
        .await
        .map(|result| result.is_ok())
        .unwrap_or(false);
        if notified {
            if origin == LaunchOrigin::ColdStart {
                app.exit(1);
            }
            return;
        }
        // 通知服务不可用时退回主界面提示, 并保留错误供前端初始化时读取.
        if let Some(state) = app.try_state::<AppState>() {
            if let Ok(mut tracker) = state.game.lock() {
                tracker.last_error = Some(error);
            }
        }
        show_main(&app);
        // 冷启动时前端可能还没订阅事件, 由初始化阶段调用 take_launch_error 兜底.
        let state = app.state::<AppState>();
        emit_game_status(&app, &state);
    }
}

fn launch_failure_summary<R: tauri::Runtime>(
    app: &AppHandle<R>,
    request: &cli::LaunchRequest,
) -> String {
    let distribution = request.region.or_else(|| {
        let state = app.try_state::<AppState>()?;
        let settings = state.settings.lock().ok()?;
        if let Some(value) = request.environment.as_deref() {
            if let Some(environment) = settings
                .runtime_environments
                .iter()
                .find(|entry| entry.id == value || entry.name == value)
            {
                return Some(environment.distribution);
            }
        }
        Some(settings.distribution)
    });
    match distribution {
        Some(distribution) => format!("{}启动失败", distribution.game_name()),
        None => "Oddity 启动失败".to_string(),
    }
}

fn notification_body(message: &str) -> String {
    const LIMIT: usize = 800;
    let mut body: String = message.chars().take(LIMIT).collect();
    if message.chars().count() > LIMIT {
        body.push('…');
    }
    body
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GameModeStatus {
    capabilities: Vec<nte_core::game_mode::Status>,
    session: Option<Vec<nte_core::game_mode::Status>>,
}

#[tauri::command]
async fn game_mode_status(state: State<'_, AppState>) -> Result<GameModeStatus, String> {
    let mode = state
        .game
        .lock()
        .map_err(|error| error.to_string())?
        .session
        .as_ref()
        .and_then(|session| session.game_mode.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let session = mode
            .map(|mode| {
                mode.lock()
                    .map(|mode| mode.status())
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
        Ok(GameModeStatus {
            capabilities: nte_core::game_mode::probe(),
            session,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn game_status(state: State<AppState>) -> GameStatus {
    let tracker = match state.game.lock() {
        Ok(tracker) => tracker,
        Err(_) => return GameStatus::idle(),
    };
    let mut status = tracker_status(&tracker);
    if tracker
        .session
        .as_ref()
        .is_some_and(|session| !session_alive(session))
    {
        status.running = false;
        status.closing = false;
        status.launch = None;
    }
    status
}

/// 冷启动失败且桌面通知不可用时, 前端初始化后取走错误并提示.
#[tauri::command]
fn take_launch_error(state: State<AppState>) -> Option<String> {
    state
        .game
        .lock()
        .ok()
        .and_then(|mut tracker| tracker.last_error.take())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchCancelAction {
    Close,
    Wait,
    AlreadyClosing,
    Idle,
}

fn launch_cancel_action(tracker: &mut GameTracker) -> LaunchCancelAction {
    if let Some(session) = tracker.session.as_mut() {
        if let Some(launch) = tracker.launch.as_mut() {
            launch.cancelling = true;
        }
        return if session.closing {
            LaunchCancelAction::AlreadyClosing
        } else {
            LaunchCancelAction::Close
        };
    }
    if tracker.starting && tracker.launch_cancel.is_some() {
        if let Some(cancel) = tracker.launch_cancel.as_ref() {
            cancel.store(true, Ordering::Release);
        }
        if let Some(launch) = tracker.launch.as_mut() {
            launch.cancelling = true;
        }
        return LaunchCancelAction::Wait;
    }
    LaunchCancelAction::Idle
}

#[tauri::command]
async fn cancel_launch<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<GameStatus, String> {
    let action = {
        let mut tracker = state.game.lock().map_err(|error| error.to_string())?;
        launch_cancel_action(&mut tracker)
    };
    match action {
        LaunchCancelAction::Close => close_game(app, state).await,
        LaunchCancelAction::Wait | LaunchCancelAction::AlreadyClosing => {
            let status = state
                .game
                .lock()
                .map(|tracker| tracker_status(&tracker))
                .unwrap_or_else(|_| GameStatus::idle());
            let _ = app.emit("game-state", status.clone());
            Ok(status)
        }
        LaunchCancelAction::Idle => {
            if let Ok(mut tracker) = state.game.lock() {
                clear_launch(&mut tracker);
            }
            Ok(GameStatus::idle())
        }
    }
}

fn finish_game_close(tracker: &mut GameTracker, session_id: u64, success: bool) {
    if tracker.session.as_ref().map(|session| session.id) != Some(session_id) {
        return;
    }
    if success {
        tracker.last_error = None;
        tracker.session = None;
        clear_launch(tracker);
    } else if let Some(session) = tracker.session.as_mut() {
        session.closing = false;
        if let Some(launch) = tracker.launch.as_mut() {
            launch.cancelling = false;
        }
    }
}

#[tauri::command]
async fn close_game<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<GameStatus, String> {
    let (session, pgid, background, game_alive) = {
        let mut tracker = state.game.lock().map_err(|e| e.to_string())?;
        if tracker.starting {
            return Err("游戏正在启动，请稍候。".to_string());
        }
        match tracker.session.as_mut() {
            Some(session) if session.closing => {
                return Err("游戏正在关闭，请稍候。".to_string());
            }
            Some(session) => {
                if !session_alive(session) {
                    let expired = session.clone();
                    tracker.session = None;
                    clear_launch(&mut tracker);
                    drop(tracker);
                    finalize_game_session(
                        &app,
                        &expired,
                        Some(nte_core::snapshots::Failure::early_exit(None)),
                    );
                    let status = GameStatus::idle();
                    let _ = app.emit("game-state", status.clone());
                    return Ok(status);
                }
                session.closing = true;
                let current = session.clone();
                (
                    current,
                    session.pgid,
                    session.background.clone(),
                    Arc::clone(&session.game_alive),
                )
            }
            None => return Ok(GameStatus::idle()),
        }
    };

    emit_game_status(&app, &state);

    let result = match tauri::async_runtime::spawn_blocking(move || {
        if let Some(background) = background {
            background.close()
        } else {
            terminate_process_group(pgid)?;
            let deadline = Instant::now() + Duration::from_secs(2);
            while game_alive.load(Ordering::Acquire) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(50));
            }
            if game_alive.load(Ordering::Acquire) {
                Err("游戏进程仍在运行, 请退出游戏后重试".into())
            } else {
                Ok(())
            }
        }
    })
    .await
    {
        Ok(result) => result,
        Err(error) => Err(format!("关闭任务失败：{error}")),
    };

    if result.is_ok() {
        finalize_game_session(
            &app,
            &session,
            Some(nte_core::snapshots::Failure::early_exit(None)),
        );
    }

    {
        let mut tracker = state.game.lock().map_err(|e| e.to_string())?;
        finish_game_close(&mut tracker, session.id, result.is_ok());
    }

    let status = state
        .game
        .lock()
        .map(|tracker| tracker_status(&tracker))
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
    window.eval("{ document.querySelectorAll('video.hero__source').forEach((video) => { if (video instanceof HTMLVideoElement && video.autoplay) void video.play().catch(() => {}); }); }")
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

fn show_main<R: tauri::Runtime>(app: &AppHandle<R>) {
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

fn launcher_builder<R: tauri::Runtime>() -> tauri::Builder<R> {
    let builder = tauri::Builder::<R>::new();
    builder
        .plugin(tauri_plugin_dialog::init())
        .manage(log_commands::LogExports::default())
}

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.iter().any(|arg| arg == "--recovery") {
        let error =
            nte_core::recovery::sibling("oddity-recovery").map(|path| Command::new(path).exec());
        eprintln!("无法打开应急恢复: {error:?}");
        std::process::exit(1);
    }
    if attendance_commands::headless_entry() {
        return;
    }
    if cli::help_requested(&args) {
        println!("{}", cli::USAGE);
        return;
    }
    let launch_request = match cli::parse(&args) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{}", cli::USAGE);
            std::process::exit(2);
        }
    };
    let paths = UserPaths::discover();
    let instance = match nte_core::recovery::LauncherLock::launcher(&paths) {
        Ok(lock) => lock,
        Err(error) => {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
    };
    migrate_legacy_dirs();
    match nte_core::recovery::open_launcher_log(&paths) {
        Ok(log) => {
            use std::os::fd::AsRawFd;
            eprintln!("Oddity 启动日志目录: {}", paths.logs().display());
            if unsafe { libc::dup2(log.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
                eprintln!("无法记录启动日志: {}", std::io::Error::last_os_error());
            }
        }
        Err(error) => eprintln!("无法创建启动日志: {error:#}"),
    }
    let compatible = args.iter().any(|arg| arg == "--recovery-compatible");
    if compatible {
        std::env::set_var("NTE_SOFTWARE_RENDER", "1");
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
    configure_graphics();
    let mut settings = load_settings();
    for environment in &settings.runtime_environments {
        if let Err(error) = nte_core::environment::recover_pending(&data_dir(), environment) {
            eprintln!("运行环境恢复失败: {error:#}");
        }
    }
    if compatible {
        settings.hero_media = "photo".into();
        settings.reduce_motion = true;
        settings.minimize_to_tray_on_close = false;
    }
    let mut context = tauri::generate_context!();
    if compatible {
        for window in &mut context.config_mut().app.windows {
            window.transparent = false;
        }
    }
    // 外部启动不显示主界面, 成功后由托盘接管, 失败时再按需显示.
    if launch_request.is_some() {
        for window in &mut context.config_mut().app.windows {
            window.visible = false;
        }
    }
    let playtime = playtime_commands::Playtime::load(&data_dir());
    let snapshots = snapshot_commands::Snapshots::load(&data_dir());
    launcher_builder()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // 插件转发的是完整 argv, 首项是可执行文件路径.
            let forwarded: Vec<OsString> = argv.into_iter().skip(1).map(OsString::from).collect();
            match cli::parse(&forwarded) {
                Ok(Some(request)) => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        handle_external_launch(app, request, LaunchOrigin::Forwarded).await;
                    });
                }
                Ok(None) => show_main(app),
                Err(error) => {
                    eprintln!("外部启动参数无效: {error}");
                    show_main(app);
                }
            }
        }))
        .manage(AppState {
            settings: Mutex::new(settings),
            game: Mutex::new(GameTracker::default()),
        })
        .manage(playtime)
        .manage(snapshots)
        .manage(download_commands::DownloadManager::default())
        .manage(background_server::BackgroundServer::start())
        .on_window_event(|window, event| {
            if matches!(
                event,
                WindowEvent::Focused(false)
                    | WindowEvent::Destroyed
                    | WindowEvent::CloseRequested { .. }
            ) {
                gamepad_commands::stop();
            }
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
        .setup(move |app| {
            app.manage(instance);
            if !compatible && load_settings().distribution == Distribution::Cn {
                attendance_commands::on_launch();
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_icon(tauri::include_image!("icons/icon.png"));
            }
            if let Err(error) = install_tray(app.handle()) {
                eprintln!("tray unavailable: {error}");
            }
            update_scheduler::start(app.handle().clone());
            if let Some(request) = launch_request {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    handle_external_launch(handle, request, LaunchOrigin::ColdStart).await;
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            file_commands::browse_files,
            gamepad_commands::gamepad_poll,
            gamepad_commands::gamepad_release_idle,
            gamepad_commands::gamepad_feedback,
            gamepad_commands::gamepad_profile_validate,
            gamepad_commands::gamepad_profile_load,
            gamepad_commands::gamepad_profile_save,
            attendance_commands::attendance_status,
            attendance_commands::attendance_refresh_game_records,
            attendance_commands::attendance_login,
            attendance_commands::attendance_send_code,
            attendance_commands::attendance_login_sms,
            attendance_commands::attendance_configure,
            attendance_commands::attendance_run,
            attendance_commands::attendance_logout,
            attendance_commands::attendance_keyring_check,
            play_hero_video,
            launcher_background,
            bootstrap,
            check_update,
            download_commands::game_download_status,
            download_commands::game_download_start,
            download_commands::game_download_cancel,
            fetch_bulletins,
            environment_commands::environment_create,
            environment_commands::environment_action,
            save_settings,
            log_commands::prepare_log_export,
            log_commands::save_log_export,
            log_commands::discard_log_export,
            snapshot_commands::environment_snapshot_overview,
            snapshot_commands::environment_snapshot_compare,
            snapshot_commands::environment_snapshot_save_reference,
            snapshot_commands::environment_snapshot_delete_reference,
            snapshot_commands::environment_snapshot_clear_history,
            open_directory,
            open_url,
            launch_official,
            cancel_launch,
            game_status,
            take_launch_error,
            playtime_commands::playtime_history,
            playtime_commands::playtime_export_poster,
            game_mode_status,
            game_mode_commands::game_mode_gpu_info,
            game_mode_commands::game_mode_gpu_apply,
            game_mode_commands::game_mode_tuning_info,
            game_mode_commands::game_mode_tuning_apply,
            close_game,
            win_minimize,
            win_toggle_maximize,
            win_close,
            win_hide,
            win_drag,
        ])
        .run(context)
        .expect("error while running Oddity");
    let _ = APP_IDENTIFIER;
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
