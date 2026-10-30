//! 桌面通知. 外部启动失败时由主程序调用.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Context;
use zbus::blocking::{connection::Builder, Connection, Proxy};
use zbus::zvariant::Value;

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const APP_NAME: &str = "Oddity";
const APP_ICON: &str = "oddity";
const EXPIRE_MS: i32 = 10_000;

/// 通过当前会话总线发送一条普通优先级通知.
pub fn send(summary: &str, body: &str) -> anyhow::Result<()> {
    let bus = Builder::session()?
        .method_timeout(Duration::from_secs(3))
        .build()
        .context("连接桌面通知服务失败")?;
    send_on(&bus, summary, body)
}

fn send_on(bus: &Connection, summary: &str, body: &str) -> anyhow::Result<()> {
    let proxy = Proxy::new(bus, SERVICE, PATH, SERVICE).context("连接桌面通知服务失败")?;
    let hints: HashMap<&str, Value<'_>> = HashMap::from([("urgency", Value::from(1u8))]);
    proxy
        .call::<_, _, u32>(
            "Notify",
            &(
                APP_NAME,
                0u32,
                APP_ICON,
                summary,
                body,
                Vec::<String>::new(),
                hints,
                EXPIRE_MS,
            ),
        )
        .context("发送桌面通知失败")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::sync::{Arc, Mutex};
    use zbus::zvariant::OwnedValue;

    #[derive(Default)]
    struct Calls {
        sent: Vec<Sent>,
    }

    struct Sent {
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        urgency: bool,
        expire_timeout: i32,
    }

    struct Server(Arc<Mutex<Calls>>);

    #[zbus::interface(name = "org.freedesktop.Notifications")]
    impl Server {
        #[allow(clippy::too_many_arguments)]
        fn notify(
            &self,
            app_name: String,
            replaces_id: u32,
            app_icon: String,
            summary: String,
            body: String,
            actions: Vec<String>,
            hints: HashMap<String, OwnedValue>,
            expire_timeout: i32,
        ) -> u32 {
            self.0.lock().unwrap().sent.push(Sent {
                app_name,
                replaces_id,
                app_icon,
                summary,
                body,
                actions,
                urgency: hints.contains_key("urgency"),
                expire_timeout,
            });
            1
        }
    }

    struct Bus {
        daemon: Child,
        _server: Connection,
        client: Connection,
        calls: Arc<Mutex<Calls>>,
    }

    impl Bus {
        fn new() -> Self {
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
                .name(SERVICE)
                .unwrap()
                .serve_at(PATH, Server(calls.clone()))
                .unwrap()
                .build()
                .unwrap();
            let client = Builder::address(address.trim())
                .unwrap()
                .method_timeout(Duration::from_secs(2))
                .build()
                .unwrap();
            Self {
                daemon,
                _server: server,
                client,
                calls,
            }
        }
    }

    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.daemon.kill();
            let _ = self.daemon.wait();
        }
    }

    #[test]
    fn sends_the_expected_notification_fields() {
        let bus = Bus::new();
        send_on(&bus.client, "异环启动失败", "尚未创建运行环境").unwrap();
        let calls = bus.calls.lock().unwrap();
        let sent = &calls.sent[0];
        assert_eq!(sent.app_name, APP_NAME);
        assert_eq!(sent.replaces_id, 0);
        assert_eq!(sent.app_icon, APP_ICON);
        assert_eq!(sent.summary, "异环启动失败");
        assert_eq!(sent.body, "尚未创建运行环境");
        assert!(sent.actions.is_empty());
        assert!(sent.urgency);
        assert_eq!(sent.expire_timeout, EXPIRE_MS);
    }
}
