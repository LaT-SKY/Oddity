//! 游戏会话的临时桌面请求. 连接和请求随会话释放, 不写入全局桌面配置.
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use zbus::blocking::{connection::Builder, Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, Value};

mod config;
pub mod gamescope;
pub mod gpu;
pub mod tuning;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const GAMEMODE: &str = "com.feralinteractive.GameMode";
const GAMEMODE_PATH: &str = "/com/feralinteractive/GameMode";
const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

#[derive(Debug)]
pub struct IdleInhibitor {
    bus: Connection,
    handle: OwnedObjectPath,
    screensaver: Option<u32>,
    power: Option<u32>,
}
impl IdleInhibitor {
    pub fn start(reason: &str) -> Result<Self, String> {
        Self::start_on(connection().map_err(|error| error.to_string())?, reason)
    }
    fn start_on(bus: Connection, reason: &str) -> Result<Self, String> {
        let options = HashMap::from([("reason", Value::from(reason))]);
        let handle = Proxy::new(&bus, PORTAL, PORTAL_PATH, "org.freedesktop.portal.Inhibit")
            .and_then(|proxy| proxy.call("Inhibit", &("", 12u32, options)))
            .map_err(|error| error.to_string())?;
        // KDE 的屏幕空闲策略由 ScreenSaver 接口接管, Portal 单独返回句柄不足以抑制熄屏.
        let screensaver = Proxy::new(
            &bus,
            "org.freedesktop.ScreenSaver",
            "/org/freedesktop/ScreenSaver",
            "org.freedesktop.ScreenSaver",
        )
        .and_then(|proxy| proxy.call("Inhibit", &("Oddity", reason)))
        .ok();
        let power = Proxy::new(
            &bus,
            "org.freedesktop.PowerManagement.Inhibit",
            "/org/freedesktop/PowerManagement/Inhibit",
            "org.freedesktop.PowerManagement.Inhibit",
        )
        .and_then(|proxy| proxy.call("Inhibit", &("Oddity", reason)))
        .ok();
        Ok(Self {
            bus,
            handle,
            screensaver,
            power,
        })
    }
}
impl Drop for IdleInhibitor {
    fn drop(&mut self) {
        if let Some(cookie) = self.power {
            let _ = Proxy::new(
                &self.bus,
                "org.freedesktop.PowerManagement.Inhibit",
                "/org/freedesktop/PowerManagement/Inhibit",
                "org.freedesktop.PowerManagement.Inhibit",
            )
            .and_then(|proxy| proxy.call::<_, _, ()>("UnInhibit", &cookie));
        }
        if let Some(cookie) = self.screensaver {
            let _ = Proxy::new(
                &self.bus,
                "org.freedesktop.ScreenSaver",
                "/org/freedesktop/ScreenSaver",
                "org.freedesktop.ScreenSaver",
            )
            .and_then(|proxy| proxy.call::<_, _, ()>("UnInhibit", &cookie));
        }
        let result = Proxy::new(
            &self.bus,
            PORTAL,
            self.handle.as_str(),
            "org.freedesktop.portal.Request",
        )
        .and_then(|proxy| proxy.call::<_, _, ()>("Close", &()));
        if let Err(error) = result {
            eprintln!("释放防休眠请求失败: {error}");
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub performance: bool,
    pub inhibit: bool,
    pub notifications: bool,
    pub overlay: bool,
    pub gamescope: gamescope::Options,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Feature {
    Performance,
    Inhibit,
    Notifications,
    Overlay,
    Gamescope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    Ready,
    Unavailable,
    Disabled,
    Waiting,
    Active,
    Requested,
    Error,
    Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub id: Feature,
    pub available: bool,
    pub state: Effect,
    pub detail: String,
}

fn connection() -> zbus::Result<Connection> {
    Builder::session()?
        .method_timeout(Duration::from_secs(3))
        .build()
}

/// 只查询能力, 不注册游戏或获取桌面抑制请求.
pub fn probe() -> Vec<Status> {
    let bus = connection();
    probe_on(bus.as_ref().ok())
}

pub fn ensure_idle() -> anyhow::Result<()> {
    let bus = connection()?;
    let status: i32 = Proxy::new(&bus, GAMEMODE, GAMEMODE_PATH, GAMEMODE)?
        .call("QueryStatus", &(std::process::id() as i32))?;
    if status != 0 {
        anyhow::bail!("请先退出所有使用 GameMode 的游戏");
    }
    Ok(())
}

fn probe_on(bus: Option<&Connection>) -> Vec<Status> {
    let performance = bus.is_some_and(|bus| {
        Proxy::new(bus, GAMEMODE, GAMEMODE_PATH, GAMEMODE)
            .and_then(|proxy| proxy.call::<_, _, i32>("QueryStatus", &(std::process::id() as i32)))
            .is_ok_and(|status| status >= 0)
    });
    let inhibit = bus.is_some_and(|bus| {
        Proxy::new(bus, PORTAL, PORTAL_PATH, "org.freedesktop.portal.Inhibit")
            .and_then(|proxy| proxy.get_property::<u32>("version"))
            .is_ok()
    });
    let notifications = bus.is_some_and(|bus| {
        Proxy::new(bus, NOTIFICATIONS, NOTIFICATIONS_PATH, NOTIFICATIONS)
            .and_then(|proxy| proxy.call::<_, _, Vec<String>>("GetCapabilities", &()))
            .is_ok_and(|caps| caps.iter().any(|cap| cap == "inhibitions"))
    });
    let overlay = executable("mangohud").is_some();
    [
        (
            Feature::Performance,
            performance,
            "GameMode 服务可用",
            "未连接到 GameMode 服务, 请安装并检查 GameMode",
        ),
        (
            Feature::Inhibit,
            inhibit,
            "桌面提供防休眠接口, 启动时请求",
            "当前桌面未提供防休眠接口",
        ),
        (
            Feature::Notifications,
            notifications,
            "通知服务支持临时抑制",
            "当前通知服务不支持临时抑制, 首批支持 Plasma",
        ),
        (
            Feature::Overlay,
            overlay,
            "已找到 MangoHud, 游戏内显示需运行环境支持",
            "未找到 MangoHud, 请安装 MangoHud",
        ),
        (
            Feature::Gamescope,
            executable("gamescope").is_some(),
            "已找到 Gamescope",
            "未找到 Gamescope",
        ),
    ]
    .into_iter()
    .map(|(id, available, yes, no)| Status {
        id,
        available,
        state: if available {
            Effect::Ready
        } else {
            Effect::Unavailable
        },
        detail: if available { yes } else { no }.into(),
    })
    .collect()
}

pub(crate) fn executable(name: &str) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|path| path.join(name))
        .find(|path| {
            path.metadata()
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
}

#[derive(Debug)]
pub struct Session {
    bus: Option<Connection>,
    status: Vec<Status>,
    inhibit: Option<IdleInhibitor>,
    notifications: Option<u32>,
    registered: HashSet<i32>,
    attempted: HashSet<i32>,
    finished: bool,
}

impl Session {
    pub fn start(options: &Options, command: &mut Command) -> Self {
        let needed = options.performance || options.inhibit || options.notifications;
        let bus = needed.then(connection).and_then(Result::ok);
        Self::start_on(options, command, bus)
    }

    fn start_on(options: &Options, command: &mut Command, bus: Option<Connection>) -> Self {
        let mut session = Self {
            status: probe_on(bus.as_ref()),
            bus,
            inhibit: None,
            notifications: None,
            registered: HashSet::new(),
            attempted: HashSet::new(),
            finished: false,
        };
        for status in &mut session.status {
            let enabled = match status.id {
                Feature::Performance => options.performance,
                Feature::Inhibit => options.inhibit,
                Feature::Notifications => options.notifications,
                Feature::Overlay => options.overlay,
                Feature::Gamescope => options.gamescope.enabled,
            };
            if !enabled {
                status.state = Effect::Disabled;
                status.detail = "本轮未启用".into();
            } else if status.available {
                status.state = Effect::Waiting;
                status.detail = "等待游戏进程".into();
            }
        }
        if options.overlay && session.available(Feature::Overlay) && !options.gamescope.enabled {
            // NTE 使用 Vulkan 转译, 仅向本轮启动链传递浮层选项.
            command.env("MANGOHUD", "1");
            session.set(
                Feature::Overlay,
                Effect::Requested,
                "已传入启动环境, 请在游戏画面中确认浮层",
            );
        }
        if options.gamescope.enabled && session.available(Feature::Gamescope) {
            session.set(
                Feature::Gamescope,
                Effect::Requested,
                "已配置 Gamescope 启动参数",
            );
            if options.overlay {
                if executable("mangoapp").is_some() {
                    session.set(
                        Feature::Overlay,
                        Effect::Requested,
                        "已由 Gamescope 请求性能浮层",
                    );
                } else {
                    session.set(
                        Feature::Overlay,
                        Effect::Error,
                        "Gamescope 性能浮层需要 mangoapp",
                    );
                }
            }
        }
        if options.inhibit && session.available(Feature::Inhibit) {
            let result = session
                .bus
                .as_ref()
                .ok_or_else(|| "会话总线不可用".to_string())
                .and_then(|bus| IdleInhibitor::start_on(bus.clone(), "Oddity 游戏会话"));
            match result {
                Ok(handle) => {
                    session.inhibit = Some(handle);
                    session.set(
                        Feature::Inhibit,
                        Effect::Requested,
                        "已请求防止空闲休眠, 实际行为由桌面决定",
                    );
                }
                Err(error) => session.set(Feature::Inhibit, Effect::Error, &error),
            }
        }
        if options.notifications && session.available(Feature::Notifications) {
            let result = session
                .bus
                .as_ref()
                .ok_or_else(|| "会话总线不可用".to_string())
                .and_then(|bus| {
                    let hints: HashMap<&str, Value<'_>> = HashMap::new();
                    Proxy::new(bus, NOTIFICATIONS, NOTIFICATIONS_PATH, NOTIFICATIONS)
                        .and_then(|proxy| {
                            proxy.call(
                                "Inhibit",
                                &("io.github.oddity.Launcher", "Oddity 游戏会话", hints),
                            )
                        })
                        .map_err(|error| error.to_string())
                });
            match result {
                Ok(cookie) => {
                    session.notifications = Some(cookie);
                    session.set(Feature::Notifications, Effect::Active, "本轮通知抑制已启用");
                }
                Err(error) => session.set(Feature::Notifications, Effect::Error, &error),
            }
        }
        session
    }

    fn available(&self, feature: Feature) -> bool {
        self.status
            .iter()
            .any(|status| status.id == feature && status.available)
    }

    fn set(&mut self, feature: Feature, state: Effect, detail: &str) {
        if let Some(status) = self.status.iter_mut().find(|status| status.id == feature) {
            status.state = state;
            status.detail = detail.into();
        }
    }

    pub fn status(&self) -> Vec<Status> {
        self.status.clone()
    }

    /// 由应用传入本轮已经识别的游戏 PID, 不对启动器或共享 wineserver 注册优化.
    pub fn update_game_processes(&mut self, pids: &[u32]) {
        if !self.needs_game_processes() {
            return;
        }
        for &pid in pids {
            let Ok(pid) = i32::try_from(pid) else {
                continue;
            };
            if !self.attempted.insert(pid) {
                continue;
            }
            let result = self
                .bus
                .as_ref()
                .ok_or_else(|| "会话总线不可用".to_string())
                .and_then(|bus| {
                    Proxy::new(bus, GAMEMODE, GAMEMODE_PATH, GAMEMODE)
                        .and_then(|proxy| {
                            proxy.call::<_, _, i32>(
                                "RegisterGameByPID",
                                &(std::process::id() as i32, pid),
                            )
                        })
                        .map_err(|error| error.to_string())
                });
            match result {
                Ok(0) => {
                    self.registered.insert(pid);
                    self.set(
                        Feature::Performance,
                        Effect::Active,
                        "已为本轮游戏进程启用 GameMode",
                    );
                }
                Ok(code) => self.set(
                    Feature::Performance,
                    Effect::Error,
                    &format!("GameMode 拒绝请求, 返回值 {code}"),
                ),
                Err(error) => self.set(Feature::Performance, Effect::Error, &error),
            }
        }
    }

    pub fn needs_game_processes(&self) -> bool {
        !self.finished
            && self.status.iter().any(|status| {
                status.id == Feature::Performance
                    && matches!(
                        status.state,
                        Effect::Waiting | Effect::Active | Effect::Error
                    )
            })
    }

    pub fn release(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        drop(self.inhibit.take());
        if let Some(bus) = &self.bus {
            if let Some(cookie) = self.notifications.take() {
                let result = Proxy::new(bus, NOTIFICATIONS, NOTIFICATIONS_PATH, NOTIFICATIONS)
                    .and_then(|proxy| proxy.call::<_, _, ()>("UnInhibit", &cookie));
                if let Err(error) = result {
                    eprintln!("释放通知抑制失败: {error}");
                }
            }
            for pid in self.registered.drain() {
                // 已退出进程由 GameMode 自行清理, 无需再次注销.
                if !Path::new(&format!("/proc/{pid}")).exists() {
                    continue;
                }
                let result = Proxy::new(bus, GAMEMODE, GAMEMODE_PATH, GAMEMODE).and_then(|proxy| {
                    proxy
                        .call::<_, _, i32>("UnregisterGameByPID", &(std::process::id() as i32, pid))
                });
                if let Err(error) = result {
                    eprintln!("注销 GameMode 请求失败: {error}");
                }
            }
        }
        // 关闭专属连接也让桌面释放属于本轮连接的抑制请求.
        self.bus = None;
        for status in &mut self.status {
            if matches!(
                status.state,
                Effect::Waiting | Effect::Active | Effect::Requested
            ) {
                status.state = Effect::Finished;
                status.detail = "本轮会话已结束".into();
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Stdio};
    use std::sync::{Arc, Mutex};
    use zbus::zvariant::OwnedValue;

    #[derive(Default)]
    struct Calls {
        registered: Vec<(i32, i32)>,
        unregistered: Vec<(i32, i32)>,
        idle_requests: u32,
        idle_closed: u32,
        notification_requests: u32,
        notification_closed: u32,
        reject: bool,
    }
    struct GameMode(Arc<Mutex<Calls>>);
    #[zbus::interface(name = "com.feralinteractive.GameMode")]
    impl GameMode {
        fn query_status(&self, _pid: i32) -> i32 {
            0
        }
        #[zbus(name = "RegisterGameByPID")]
        fn register(&self, caller: i32, game: i32) -> i32 {
            let mut calls = self.0.lock().unwrap();
            calls.registered.push((caller, game));
            if calls.reject {
                -1
            } else {
                0
            }
        }
        #[zbus(name = "UnregisterGameByPID")]
        fn unregister(&self, caller: i32, game: i32) -> i32 {
            self.0.lock().unwrap().unregistered.push((caller, game));
            0
        }
    }
    struct Inhibit(Arc<Mutex<Calls>>);
    #[zbus::interface(name = "org.freedesktop.portal.Inhibit")]
    impl Inhibit {
        #[zbus(property, name = "version")]
        fn version(&self) -> u32 {
            3
        }
        fn inhibit(
            &self,
            _window: &str,
            flags: u32,
            _options: HashMap<String, OwnedValue>,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            assert_eq!(flags, 12);
            let mut calls = self.0.lock().unwrap();
            calls.idle_requests += 1;
            if calls.reject {
                return Err(zbus::fdo::Error::Failed("rejected".into()));
            }
            Ok(OwnedObjectPath::try_from("/org/freedesktop/portal/desktop/request/test").unwrap())
        }
    }
    struct Request(Arc<Mutex<Calls>>);
    #[zbus::interface(name = "org.freedesktop.portal.Request")]
    impl Request {
        fn close(&self) {
            self.0.lock().unwrap().idle_closed += 1;
        }
    }
    struct Notifications(Arc<Mutex<Calls>>);
    #[zbus::interface(name = "org.freedesktop.Notifications")]
    impl Notifications {
        fn get_capabilities(&self) -> Vec<String> {
            vec!["inhibitions".into()]
        }
        fn inhibit(
            &self,
            entry: &str,
            _reason: &str,
            _hints: HashMap<String, OwnedValue>,
        ) -> zbus::fdo::Result<u32> {
            assert_eq!(entry, "io.github.oddity.Launcher");
            let mut calls = self.0.lock().unwrap();
            calls.notification_requests += 1;
            if calls.reject {
                return Err(zbus::fdo::Error::Failed("rejected".into()));
            }
            Ok(42)
        }
        #[zbus(name = "UnInhibit")]
        fn uninhibit(&self, cookie: u32) {
            assert_eq!(cookie, 42);
            self.0.lock().unwrap().notification_closed += 1;
        }
    }
    struct Bus {
        _env: crate::testing::EnvGuard,
        daemon: Child,
        _server: Connection,
        client: Connection,
        calls: Arc<Mutex<Calls>>,
    }
    impl Bus {
        fn new() -> Self {
            let env = crate::testing::EnvGuard::new();
            let mut daemon = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .spawn()
                .expect("启动隔离测试总线");
            let mut address = String::new();
            BufReader::new(daemon.stdout.take().unwrap())
                .read_line(&mut address)
                .unwrap();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let server = Builder::address(address.trim())
                .unwrap()
                .name(GAMEMODE)
                .unwrap()
                .name(PORTAL)
                .unwrap()
                .name(NOTIFICATIONS)
                .unwrap()
                .serve_at(GAMEMODE_PATH, GameMode(calls.clone()))
                .unwrap()
                .serve_at(PORTAL_PATH, Inhibit(calls.clone()))
                .unwrap()
                .serve_at(
                    "/org/freedesktop/portal/desktop/request/test",
                    Request(calls.clone()),
                )
                .unwrap()
                .serve_at(NOTIFICATIONS_PATH, Notifications(calls.clone()))
                .unwrap()
                .build()
                .unwrap();
            let client = Builder::address(address.trim())
                .unwrap()
                .method_timeout(Duration::from_secs(2))
                .build()
                .unwrap();
            Self {
                _env: env,
                daemon,
                _server: server,
                client,
                calls,
            }
        }
        fn start(&self) -> Session {
            Session::start_on(
                &Options {
                    performance: true,
                    inhibit: true,
                    notifications: true,
                    overlay: false,
                    ..Default::default()
                },
                &mut Command::new("false"),
                Some(self.client.clone()),
            )
        }
    }
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.daemon.kill();
            let _ = self.daemon.wait();
        }
    }

    #[test]
    fn settings_load_existing_and_partial_game_mode_options() {
        let old: crate::settings::Settings =
            serde_json::from_str(r#"{"launchMode":"direct"}"#).unwrap();
        assert!(
            !old.game_mode.performance
                && !old.game_mode.inhibit
                && !old.game_mode.notifications
                && !old.game_mode.overlay
        );
        let partial: crate::settings::Settings =
            serde_json::from_str(r#"{"gameMode":{"inhibit":true}}"#).unwrap();
        assert!(partial.game_mode.inhibit);
        assert!(
            !partial.game_mode.performance
                && !partial.game_mode.notifications
                && !partial.game_mode.overlay
        );
        let saved = serde_json::to_value(&partial).unwrap();
        assert_eq!(saved["gameMode"]["inhibit"], true);
    }

    #[test]
    fn gamescope_overlay_status_tracks_mangoapp_without_injecting_mangohud() {
        use std::os::unix::fs::PermissionsExt;
        let mut env = crate::testing::EnvGuard::new();
        let dir = crate::testing::TempDir::new("gamescope-overlay-status");
        env.set("PATH", dir.path());
        for name in ["gamescope", "mangohud"] {
            let path = dir.write(name, "#!/bin/sh\nexit 0\n");
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut options = Options {
            overlay: true,
            gamescope: gamescope::Options {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        for available in [false, true] {
            if available {
                let path = dir.write("mangoapp", "#!/bin/sh\nexit 0\n");
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            let mut command = Command::new("umu-run");
            let session = Session::start_on(&options, &mut command, None);
            assert_eq!(session.status[4].state, Effect::Requested);
            assert_eq!(
                session.status[3].state,
                if available {
                    Effect::Requested
                } else {
                    Effect::Error
                }
            );
            assert!(session.status[3].detail.contains(if available {
                "Gamescope"
            } else {
                "mangoapp"
            }));
            assert!(!command.get_envs().any(|(key, _)| key == "MANGOHUD"));
            assert!(!session.needs_game_processes());
        }
        options.overlay = false;
        let session = Session::start_on(&options, &mut Command::new("umu-run"), None);
        assert_eq!(session.status[3].state, Effect::Disabled);
    }

    #[test]
    fn mangohud_and_gamemode_can_be_enabled_together() {
        use std::os::unix::fs::PermissionsExt;
        let mut bus = Bus::new();
        let dir = crate::testing::TempDir::new("gamemode-mangohud");
        let executable = dir.write("mangohud", "#!/bin/sh\nexec \"$@\"\n");
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        bus._env.set("PATH", dir.path());
        let mut command = Command::new("umu-run");
        let mut session = Session::start_on(
            &Options {
                performance: true,
                overlay: true,
                ..Default::default()
            },
            &mut command,
            Some(bus.client.clone()),
        );
        assert!(command
            .get_envs()
            .any(|(key, value)| key == "MANGOHUD" && value == Some(std::ffi::OsStr::new("1"))));
        let pid = std::process::id();
        assert!(session.needs_game_processes());
        session.update_game_processes(&[pid]);
        assert_eq!(session.status[0].state, Effect::Active);
        assert_eq!(session.status[3].state, Effect::Requested);
        session.release();
        assert!(!session.needs_game_processes());
        let calls = bus.calls.lock().unwrap();
        assert_eq!(calls.registered, [(pid as i32, pid as i32)]);
        assert_eq!(calls.unregistered, calls.registered);
        drop(calls);
        let mut disabled = Session::start_on(
            &Options {
                overlay: true,
                ..Default::default()
            },
            &mut command,
            Some(bus.client.clone()),
        );
        disabled.update_game_processes(&[pid]);
        assert!(!disabled.needs_game_processes());
        assert_eq!(disabled.status[0].state, Effect::Disabled);
        assert_eq!(disabled.status[3].state, Effect::Requested);
        disabled.release();
        assert_eq!(bus.calls.lock().unwrap().registered.len(), 1);
    }

    #[test]
    fn requests_follow_the_game_process_and_release_only_owned_handles_once() {
        let bus = Bus::new();
        let mut session = bus.start();
        assert_eq!(session.status[0].state, Effect::Waiting);
        assert_eq!(session.status[1].state, Effect::Requested);
        assert_eq!(session.status[2].state, Effect::Active);
        let pid = std::process::id();
        session.update_game_processes(&[pid]);
        session.update_game_processes(&[pid]);
        assert_eq!(session.status[0].state, Effect::Active);
        session.release();
        session.release();
        session.update_game_processes(&[pid]);
        drop(session);
        let calls = bus.calls.lock().unwrap();
        assert_eq!(calls.registered, [(pid as i32, pid as i32)]);
        assert_eq!(calls.unregistered, calls.registered);
        assert_eq!((calls.idle_requests, calls.idle_closed), (1, 1));
        assert_eq!(
            (calls.notification_requests, calls.notification_closed),
            (1, 1)
        );
    }

    #[test]
    fn launch_failure_drops_desktop_requests() {
        let bus = Bus::new();
        let result = (|| -> std::io::Result<()> {
            let _session = bus.start();
            Command::new("/nonexistent/oddity-test-game").spawn()?;
            Ok(())
        })();
        assert!(result.is_err());
        let calls = bus.calls.lock().unwrap();
        assert_eq!(calls.idle_closed, 1);
        assert_eq!(calls.notification_closed, 1);
        assert!(calls.registered.is_empty());
    }

    #[test]
    fn rejected_requests_are_reported_and_not_repeated_each_poll() {
        let bus = Bus::new();
        bus.calls.lock().unwrap().reject = true;
        let mut session = bus.start();
        session.update_game_processes(&[std::process::id()]);
        session.update_game_processes(&[std::process::id()]);
        assert!(session.status[..3]
            .iter()
            .all(|status| status.state == Effect::Error));
        drop(session);
        let calls = bus.calls.lock().unwrap();
        assert_eq!(calls.registered.len(), 1);
        assert!(calls.unregistered.is_empty());
        assert_eq!(calls.idle_closed + calls.notification_closed, 0);
    }

    #[test]
    fn disabled_options_and_missing_bus_do_not_claim_activation() {
        let bus = Bus::new();
        let session = Session::start_on(
            &Options::default(),
            &mut Command::new("true"),
            Some(bus.client.clone()),
        );
        assert!(session
            .status
            .iter()
            .all(|status| status.state == Effect::Disabled));
        drop(session);
        let calls = bus.calls.lock().unwrap();
        assert_eq!(calls.idle_requests + calls.notification_requests, 0);
        drop(calls);
        let session = Session::start_on(
            &Options {
                performance: true,
                inhibit: true,
                notifications: true,
                overlay: false,
                ..Default::default()
            },
            &mut Command::new("true"),
            None,
        );
        assert!(session.status[..3]
            .iter()
            .all(|status| status.state == Effect::Unavailable));
    }
}
