//! 资源自动下载调度.
//!
//! 只在启动器运行期间工作: 发现更新后按用户设置的时段、计费网络策略
//! 启动现有的下载任务. 关闭启动器后的无人值守不在本模块范围内.

use crate::service::ServiceHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nte_core::settings::Settings;
use nte_core::updates::{self, Metered, MeteredPolicy, Options};

use crate::download_commands::{self, DownloadStatus};
use crate::{selected_install_root, session_alive};

/// 调度线程的检查周期.
#[cfg(not(test))]
const TICK: Duration = Duration::from_secs(15);
#[cfg(test)]
const TICK: Duration = Duration::from_millis(10);
/// 两次资源检查之间的最小间隔; 启动时的静默检查也计入这个时间.
const CHECK_INTERVAL_MS: u64 = 30 * 60 * 1000;
/// 用户手动暂停自动下载后, 这段时间内不再自动重启.
const CANCEL_SNOOZE_MS: u64 = 60 * 60 * 1000;
/// 任务失败后的重试间隔.
const FAILURE_BACKOFF_MS: u64 = 10 * 60 * 1000;
/// 计费状态缓存时间.
const METERED_CACHE_MS: u64 = 60 * 1000;
/// 下载过程中的状态采样间隔.
#[cfg(not(test))]
const MONITOR_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(test)]
const MONITOR_INTERVAL: Duration = Duration::from_millis(10);

pub fn start(app: ServiceHandle) {
    start_with_stop(app, Arc::new(AtomicBool::new(false)));
}

fn start_with_stop(app: ServiceHandle, stop: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new()
        .name("oddity-auto-update".into())
        .spawn(move || run(app, &stop));
}

#[cfg(test)]
fn start_for_test(app: ServiceHandle, stop: Arc<AtomicBool>) {
    start_with_stop(app, stop);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn settings(app: &ServiceHandle) -> Option<Settings> {
    let state = &app.state;
    state.settings.lock().ok().map(|settings| settings.clone())
}

fn game_busy(app: &ServiceHandle) -> bool {
    let state = &app.state;
    let Ok(game) = state.game.lock() else {
        return true;
    };
    game.environment_busy || game.starting || game.session.as_ref().is_some_and(session_alive)
}

/// 与下载任务一致: 未配置目录时使用环境探测结果.
fn install_root(settings: &Settings) -> std::path::PathBuf {
    if crate::configured_install_root(settings).is_some() {
        return selected_install_root(settings);
    }
    crate::detect_launcher_root(settings)
        .and_then(|root| root.parent().map(|parent| parent.to_path_buf()))
        .unwrap_or_else(|| selected_install_root(settings))
}

fn installed(settings: &Settings) -> bool {
    install_root(settings)
        .join(settings.distribution.launcher_dir())
        .join("UserData/Patcher/PatcherSDK/ResList.xml")
        .is_file()
}

fn snapshot(app: &ServiceHandle) -> Option<DownloadStatus> {
    download_commands::snapshot(app)
}

/// 查询计费状态; 相同结果在一分钟内复用.
fn current_metered(options: &Options, cache: &mut Option<(u64, Metered)>, now: u64) -> Metered {
    if options.metered == MeteredPolicy::Allow {
        return Metered::No;
    }
    if let Some((at, value)) = *cache {
        if now.saturating_sub(at) < METERED_CACHE_MS {
            return value;
        }
    }
    let value = updates::metered();
    *cache = Some((now, value));
    value
}

fn wait_idle(app: &ServiceHandle, timeout: Duration) {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        match snapshot(app) {
            Some(status) if download_commands::busy(&status.phase) => {
                std::thread::sleep(Duration::from_millis(500));
            }
            _ => return,
        }
    }
}

fn run(app: ServiceHandle, stop: &AtomicBool) {
    let mut snooze_until = 0u64;
    let mut metered_cache: Option<(u64, Metered)> = None;
    let mut last_failure = 0u64;
    let mut announced = false;

    loop {
        std::thread::sleep(TICK);
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let Some(settings) = settings(&app) else {
            continue;
        };
        let options = &settings.updates;
        if !options.auto_download {
            announced = false;
            // 关闭自动下载后清掉残留的等待提示.
            if let Some(status) = snapshot(&app) {
                if !status.auto_waiting.is_empty() {
                    download_commands::set_auto_waiting(&app, "");
                }
            }
            continue;
        }
        if !announced {
            eprintln!("自动下载: 已启用, 等待检查周期");
            announced = true;
        }
        let now = now_ms();
        if snooze_until > now {
            continue;
        }
        if last_failure > 0 && now.saturating_sub(last_failure) < FAILURE_BACKOFF_MS {
            continue;
        }

        let Some(status) = snapshot(&app) else {
            continue;
        };

        if download_commands::busy(&status.phase) {
            monitor(&app, &status, options, &mut metered_cache);
            continue;
        }

        // 手动或自动下载被暂停后都先冷却一段时间; 因时段或计费网络
        // 暂停的任务带有等待原因, 条件恢复后可以继续.
        if status.phase == "cancelled" && status.mode == "apply" && status.auto_waiting.is_empty() {
            if snooze_until == 0 {
                snooze_until = now_ms().saturating_add(CANCEL_SNOOZE_MS);
                continue;
            }
            // 冷却结束, 允许重新检查; 新一轮取消会重新计时.
            snooze_until = 0;
        }

        let root = install_root(&settings).display().to_string();
        let same_target =
            status.distribution == settings.distribution.as_str() && status.install_root == root;
        let recent = matches!(
            status.phase.as_str(),
            "ready" | "needs-install" | "needs-update" | "needs-verify"
        ) && same_target
            && status.checked_at > 0
            && now.saturating_sub(status.checked_at) < CHECK_INTERVAL_MS;

        if game_busy(&app) {
            // 最近一次检查已确认就绪时不再提示等待下载.
            let waiting = if recent && status.phase == "ready" {
                ""
            } else {
                "game"
            };
            download_commands::set_auto_waiting(&app, waiting);
            continue;
        }
        if !installed(&settings) {
            // 首次安装必须由用户发起; 目录未就绪时保持静默.
            download_commands::set_auto_waiting(&app, "");
            continue;
        }

        // 检查不写入游戏文件, 时段外也照常进行, 以便提前告知有待下载的更新.
        let status = if recent {
            status
        } else {
            eprintln!("自动下载: 开始检查资源更新");
            if let Err(error) = download_commands::start_task(&app, "check", false, "auto") {
                // 已有任务在进行, 下一轮再试.
                eprintln!("自动下载: 检查未启动: {error}");
                continue;
            }
            wait_idle(&app, Duration::from_secs(10 * 60));
            let Some(status) = snapshot(&app) else {
                continue;
            };
            if status.phase == "failed" {
                last_failure = now_ms();
                continue;
            }
            if status.phase == "cancelled" {
                continue;
            }
            last_failure = 0;
            status
        };

        if status.phase == "ready" {
            download_commands::set_auto_waiting(&app, "");
            continue;
        }
        if !matches!(
            status.phase.as_str(),
            "needs-install" | "needs-update" | "needs-verify"
        ) {
            continue;
        }

        if options
            .window
            .as_ref()
            .is_some_and(|window| !window.contains(updates::local_minute()))
        {
            download_commands::set_auto_waiting(&app, "window");
            continue;
        }
        let metered = current_metered(options, &mut metered_cache, now);
        if metered.is_metered() && options.metered != MeteredPolicy::Allow {
            download_commands::set_auto_waiting(&app, "metered");
            continue;
        }
        if status
            .disk_available_bytes
            .is_some_and(|available| status.disk_required_bytes > available)
        {
            download_commands::set_auto_waiting(&app, "disk");
            continue;
        }
        download_commands::set_auto_waiting(&app, "");

        snooze_until = 0;
        eprintln!("自动下载: 发现待下载资源, 开始下载");
        let started = match download_commands::start_task(&app, "apply", false, "auto") {
            Ok(status) => status,
            Err(error) => {
                eprintln!("自动下载: 无法开始下载: {error}");
                continue;
            }
        };
        let finished = monitor_until_idle(&app, started, &mut metered_cache);
        match finished.phase.as_str() {
            "ready" => {
                last_failure = 0;
                download_commands::set_auto_waiting(&app, "");
            }
            "failed" => last_failure = now_ms(),
            "cancelled" if finished.auto_waiting.is_empty() => {
                snooze_until = now_ms().saturating_add(CANCEL_SNOOZE_MS);
            }
            _ => {}
        }
    }
}

/// 自动任务执行期间持续检查时段与计费网络, 违反策略时暂停.
fn monitor_until_idle(
    app: &ServiceHandle,
    mut status: DownloadStatus,
    metered_cache: &mut Option<(u64, Metered)>,
) -> DownloadStatus {
    loop {
        if !download_commands::busy(&status.phase) {
            return status;
        }
        std::thread::sleep(MONITOR_INTERVAL);
        // 下载期间设置可能被修改, 每次都读取最新值.
        if let Some(settings) = settings(app) {
            monitor(app, &status, &settings.updates, metered_cache);
        }
        match snapshot(app) {
            Some(next) => status = next,
            None => return status,
        }
    }
}

fn monitor(
    app: &ServiceHandle,
    status: &DownloadStatus,
    options: &Options,
    metered_cache: &mut Option<(u64, Metered)>,
) {
    // 检查阶段很短, 只有真正写入文件的下载才需要按策略暂停.
    if status.origin != "auto" || status.mode != "apply" {
        return;
    }
    if options
        .window
        .as_ref()
        .is_some_and(|window| !window.contains(updates::local_minute()))
    {
        download_commands::cancel_task(app);
        download_commands::set_auto_waiting(app, "window");
        return;
    }
    let metered = current_metered(options, metered_cache, now_ms());
    if metered.is_metered() && options.metered != MeteredPolicy::Allow {
        download_commands::cancel_task(app);
        download_commands::set_auto_waiting(app, "metered");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download_commands::{snapshot, DownloadManager};
    use crate::{AppState, GameTracker};
    use nte_core::distribution::Distribution;
    use nte_core::updates::Window;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicU64;
    use std::sync::Mutex;
    use std::time::Instant;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "oddity-scheduler-{}-{}-{}",
                std::process::id(),
                tag,
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn installed_root(dir: &TempDir) -> PathBuf {
        let root = dir.path().join("install");
        let ledger = root.join("NTELauncher/UserData/Patcher/PatcherSDK/ResList.xml");
        fs::create_dir_all(ledger.parent().expect("ledger parent")).expect("create ledger dir");
        fs::write(ledger, b"ledger").expect("write ledger");
        root
    }

    fn test_settings(root: &Path) -> Settings {
        Settings {
            distribution: Distribution::Cn,
            install_root: Some(root.display().to_string()),
            updates: Options {
                auto_download: true,
                metered: MeteredPolicy::Allow,
                ..Options::default()
            },
            ..Settings::default()
        }
    }

    fn test_app(settings: Settings) -> ServiceHandle {
        let mut app = crate::service::test_service();
        std::sync::Arc::get_mut(&mut app).unwrap().downloads = DownloadManager::default();
        std::sync::Arc::get_mut(&mut app).unwrap().state = AppState {
            settings: Mutex::new(settings),
            game: Mutex::new(GameTracker::default()),
        };
        app
    }

    fn set_status(app: &ServiceHandle, update: impl FnOnce(&mut DownloadStatus)) {
        let manager = &app.downloads;
        let mut inner = manager.inner.lock().expect("download manager");
        update(&mut inner.status);
    }

    fn ready_status(app: &ServiceHandle, root: &Path) {
        let root = root.display().to_string();
        set_status(app, |status| {
            status.phase = "ready".into();
            status.mode = "check".into();
            status.origin = "manual".into();
            status.auto_waiting.clear();
            status.distribution = "cn".into();
            status.install_root = root.clone();
            status.checked_at = now_ms();
        });
    }

    fn download_status(app: &ServiceHandle) -> DownloadStatus {
        snapshot(app).expect("download status")
    }

    fn wait_until(mut check: impl FnMut() -> bool, timeout: Duration) -> bool {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if check() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        check()
    }

    fn window_outside_now() -> Window {
        let minute = updates::local_minute();
        let start = (minute + 60) % (24 * 60);
        let end = (start + 60) % (24 * 60);
        Window {
            start: format!("{:02}:{:02}", start / 60, start % 60),
            end: format!("{:02}:{:02}", end / 60, end % 60),
        }
    }

    fn start(app: &ServiceHandle) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        start_for_test(app.clone(), Arc::clone(&stop));
        stop
    }

    fn halt(stop: &AtomicBool) {
        stop.store(true, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(50));
    }

    #[test]
    fn disabled_auto_download_does_nothing() {
        let dir = TempDir::new("disabled");
        let root = installed_root(&dir);
        let mut settings = test_settings(&root);
        settings.updates.auto_download = false;
        let app = test_app(settings);
        ready_status(&app, &root);
        let stop = start(&app);
        std::thread::sleep(Duration::from_millis(80));
        assert!(download_status(&app).auto_waiting.is_empty());
        halt(&stop);
    }

    #[test]
    fn waits_for_game_and_clears_when_ready() {
        let dir = TempDir::new("game");
        let root = installed_root(&dir);
        let app = test_app(test_settings(&root));
        let root_text = root.display().to_string();
        set_status(&app, |status| {
            status.phase = "idle".into();
            status.mode = "check".into();
            status.distribution = "cn".into();
            status.install_root = root_text;
            status.checked_at = 0;
            status.auto_waiting.clear();
        });
        app.state.game.lock().unwrap().starting = true;
        let stop = start(&app);
        assert!(wait_until(
            || download_status(&app).auto_waiting == "game",
            Duration::from_secs(2)
        ));

        // 游戏仍在运行, 但最近一次检查已确认就绪, 不再提示等待下载.
        ready_status(&app, &root);
        assert!(wait_until(
            || download_status(&app).auto_waiting.is_empty(),
            Duration::from_secs(2)
        ));

        app.state.game.lock().unwrap().starting = false;
        assert!(download_status(&app).auto_waiting.is_empty());
        halt(&stop);
    }

    #[test]
    fn waits_for_window_then_disk() {
        let dir = TempDir::new("gates");
        let root = installed_root(&dir);
        let mut settings = test_settings(&root);
        settings.updates.window = Some(window_outside_now());
        let app = test_app(settings);
        let root_text = root.display().to_string();
        set_status(&app, |status| {
            status.phase = "needs-update".into();
            status.mode = "check".into();
            status.origin = "manual".into();
            status.auto_waiting.clear();
            status.distribution = "cn".into();
            status.install_root = root_text.clone();
            status.checked_at = now_ms();
            status.disk_available_bytes = Some(u64::MAX);
            status.disk_required_bytes = 0;
        });
        let stop = start(&app);
        assert!(wait_until(
            || download_status(&app).auto_waiting == "window",
            Duration::from_secs(2)
        ));
        set_status(&app, |status| {
            status.disk_available_bytes = Some(0);
            status.disk_required_bytes = 1;
        });
        app.state.settings.lock().unwrap().updates.window = None;
        assert!(wait_until(
            || download_status(&app).auto_waiting == "disk",
            Duration::from_secs(2)
        ));
        halt(&stop);
    }

    #[test]
    fn monitors_auto_download_and_pauses_at_window_end() {
        let dir = TempDir::new("monitor");
        let root = installed_root(&dir);
        let mut settings = test_settings(&root);
        settings.updates.window = Some(window_outside_now());
        let app = test_app(settings);
        let root_text = root.display().to_string();
        set_status(&app, |status| {
            status.phase = "game".into();
            status.mode = "apply".into();
            status.origin = "auto".into();
            status.distribution = "cn".into();
            status.install_root = root_text.clone();
            status.checked_at = now_ms();
        });
        let stop = start(&app);
        assert!(wait_until(
            || download_status(&app).auto_waiting == "window",
            Duration::from_secs(2)
        ));
        halt(&stop);
    }

    #[test]
    fn cancelled_apply_starts_a_snooze() {
        let dir = TempDir::new("snooze");
        let root = installed_root(&dir);
        let app = test_app(test_settings(&root));
        let root_text = root.display().to_string();
        set_status(&app, |status| {
            status.phase = "cancelled".into();
            status.mode = "apply".into();
            status.origin = "auto".into();
            status.auto_waiting.clear();
            status.distribution = "cn".into();
            status.install_root = root_text.clone();
            status.checked_at = now_ms();
        });
        let stop = start(&app);
        std::thread::sleep(Duration::from_millis(80));
        let status = download_status(&app);
        assert_eq!(status.phase, "cancelled");
        assert_eq!(status.mode, "apply");
        halt(&stop);
    }

    #[test]
    fn missing_installation_stays_silent() {
        let dir = TempDir::new("missing");
        let root = dir.path().join("install");
        let app = test_app(test_settings(&root));
        ready_status(&app, &root);
        let stop = start(&app);
        std::thread::sleep(Duration::from_millis(80));
        assert!(download_status(&app).auto_waiting.is_empty());
        halt(&stop);
    }

    #[test]
    fn wait_idle_returns_after_the_task_finishes() {
        let dir = TempDir::new("idle");
        let root = installed_root(&dir);
        let app = test_app(test_settings(&root));
        set_status(&app, |status| status.phase = "checking".into());
        let handle = app.clone();
        let helper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            set_status(&handle, |status| status.phase = "ready".into());
        });
        let started = Instant::now();
        wait_idle(&app, Duration::from_secs(2));
        assert!(started.elapsed() >= Duration::from_millis(80));
        helper.join().expect("join idle helper");
    }

    #[test]
    fn helper_decisions_are_stable() {
        let dir = TempDir::new("helpers");
        let root = installed_root(&dir);
        let settings = test_settings(&root);
        assert_eq!(install_root(&settings), root);
        assert!(installed(&settings));
        let mut cache = None;
        assert_eq!(
            current_metered(&settings.updates, &mut cache, 0),
            Metered::No
        );
        assert!(now_ms() > 0);
    }
}
