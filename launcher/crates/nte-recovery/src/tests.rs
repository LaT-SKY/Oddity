use super::*;
use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

struct Process(std::process::Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn native_window_actions_work_without_a_webview() {
    let root = std::env::temp_dir().join(format!("oddity-recovery-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut display = Process(
        Command::new("Xvfb")
            .args([
                "-displayfd",
                "1",
                "-screen",
                "0",
                "1024x768x24",
                "-nolisten",
                "tcp",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("安装 Xvfb 后运行原生窗口测试"),
    );
    let mut number = String::new();
    std::io::BufReader::new(display.0.stdout.take().unwrap())
        .read_line(&mut number)
        .unwrap();
    assert!(!number.trim().is_empty());
    let mut fixture = Process(
        Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "tests::native_window_fixture", "--nocapture"])
            .env("ODDITY_RECOVERY_UI_FIXTURE", &root)
            .env("HOME", &root)
            .env("GIO_USE_VFS", "local")
            .env("GSETTINGS_BACKEND", "memory")
            .env("DISPLAY", format!(":{}", number.trim()))
            .env_remove("WAYLAND_DISPLAY")
            .env("GDK_BACKEND", "x11")
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let result = loop {
        if let Some(status) = fixture.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "原生窗口测试超时");
        std::thread::sleep(Duration::from_millis(50));
    };
    std::fs::remove_dir_all(root).unwrap();
    assert!(result.success());
}

fn widgets(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut all = vec![widget.clone()];
    if let Some(container) = widget.downcast_ref::<gtk::Container>() {
        for child in container.children() {
            all.extend(widgets(&child));
        }
    }
    all
}

fn click(window: &gtk::Window, label: &str) {
    let button = widgets(window.upcast_ref())
        .into_iter()
        .find_map(|w| {
            w.downcast::<gtk::Button>()
                .ok()
                .filter(|b| b.label().as_deref() == Some(label))
        })
        .unwrap();
    button.emit_clicked();
    drain();
}

fn drain() {
    let context = gtk::glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

fn wait_for(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() && Instant::now() < deadline {
        drain();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(condition(), "窗口操作未在预期时间完成");
}

fn answer(response: gtk::ResponseType, directory: Option<PathBuf>) {
    gtk::glib::timeout_add_local_once(Duration::from_millis(100), move || {
        let dialog = gtk::Window::list_toplevels()
            .into_iter()
            .find_map(|w| w.downcast::<gtk::Dialog>().ok())
            .unwrap();
        if let Some(directory) = directory {
            let chooser = dialog.clone().downcast::<gtk::FileChooserDialog>().unwrap();
            chooser.set_current_folder(directory);
            gtk::glib::timeout_add_local_once(Duration::from_millis(200), move || {
                chooser.response(response)
            });
        } else {
            dialog.response(response);
        }
    });
}

#[test]
fn native_window_fixture() {
    let Some(root) = std::env::var_os("ODDITY_RECOVERY_UI_FIXTURE").map(PathBuf::from) else {
        return;
    };
    gtk::init().unwrap();
    let window = build_window();
    let buffer = widgets(window.upcast_ref())
        .into_iter()
        .find_map(|w| w.downcast::<gtk::TextView>().ok())
        .unwrap()
        .buffer()
        .unwrap();
    let text = || {
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .unwrap()
            .to_string()
    };
    assert!(text().contains("尚未创建"));
    click(&window, "刷新诊断");
    answer(gtk::ResponseType::Cancel, None);
    click(&window, "恢复界面设置");
    let paths = UserPaths::discover();
    assert!(!paths.settings().exists());
    answer(gtk::ResponseType::Ok, None);
    click(&window, "恢复界面设置");
    wait_for(|| text().contains("界面设置已恢复"));
    let valid = std::fs::read(paths.settings()).unwrap();
    assert!(String::from_utf8_lossy(&valid).contains("photo"));
    // 运行中的主程序使原生窗口拒绝恢复, 不得覆盖设置.
    let lock = recovery::LauncherLock::launcher(&paths).unwrap();
    answer(gtk::ResponseType::Ok, None);
    click(&window, "恢复界面设置");
    wait_for(|| text().contains("操作失败"));
    assert_eq!(std::fs::read(paths.settings()).unwrap(), valid);
    drop(lock);
    std::fs::write(paths.settings(), "{invalid").unwrap();
    answer(gtk::ResponseType::Ok, None);
    click(&window, "重建损坏配置");
    wait_for(|| text().contains("界面设置已恢复"));
    answer(gtk::ResponseType::Accept, Some(root.clone()));
    click(&window, "导出诊断包");
    wait_for(|| text().contains("诊断包已保存"));
    assert!(std::fs::read_dir(&root).unwrap().any(|entry| entry
        .unwrap()
        .path()
        .extension()
        .is_some_and(|ext| ext == "zip")));
    // 没有运行中的主程序时结束进程是可重复的操作.
    answer(gtk::ResponseType::Ok, None);
    click(&window, "结束启动器进程");
    wait_for(|| text().contains("启动器已退出"));
    click(&window, "兼容启动");
    wait_for(|| text().contains("操作失败"));
    click(&window, "刷新诊断");
    std::thread::sleep(Duration::from_millis(100));
    drain();
    if let Some(screenshot) = std::env::var_os("ODDITY_RECOVERY_SCREENSHOT") {
        assert!(Command::new("import")
            .args(["-window", "root"])
            .arg(screenshot)
            .status()
            .unwrap()
            .success());
    }
    unsafe {
        window.destroy();
    }
}
