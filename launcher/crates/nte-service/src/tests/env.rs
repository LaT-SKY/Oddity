use super::*;
use crate::*;
#[test]
fn open_url_only_accepts_http_links() {
    let error = open_url("file:///etc/passwd".to_string()).expect_err("rejected scheme");
    assert!(error.contains("只允许打开 http(s) 链接"), "{error}");
    assert!(open_url("javascript:alert(1)".to_string()).is_err());
    assert!(open_url("/usr/share/doc".to_string()).is_err());
}

#[test]
fn open_url_spawns_the_desktop_opener() {
    let mut env = EnvGuard::new();
    let dir = TempDir::new("open-url");
    let log = dir.join("xdg-open.log");
    write_script(
        dir.path(),
        "xdg-open",
        &format!("#!/bin/sh\necho \"$1\" > {}\n", log.display()),
    );
    env.set("PATH", dir.path());

    open_url("https://example.test/".to_string()).expect("open url");
    // xdg-open 是异步启动的，等它把参数写进日志
    let deadline = Instant::now() + Duration::from_secs(2);
    while std::fs::read_to_string(&log).unwrap_or_default().is_empty() && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        std::fs::read_to_string(&log).expect("read log").trim(),
        "https://example.test/"
    );

    // PATH 里没有 xdg-open 时报错
    env.set("PATH", dir.join("empty"));
    let error = open_url("https://example.test/".to_string()).expect_err("no opener");
    assert!(error.contains("打开链接失败"), "{error}");
}

#[test]
fn open_directory_uses_default_opener_and_reports_failures() {
    let mut env = EnvGuard::new();
    let dir = TempDir::new("open-directory");
    let destination = dir.join("日志 导出");
    std::fs::create_dir(&destination).unwrap();
    write_script(
        dir.path(),
        "xdg-open",
        "#!/bin/sh\nprintf '%s' \"$1\" > \"$1/opener-argument\"\n",
    );
    env.set("PATH", dir.path());
    open_directory_impl(&destination).unwrap();
    assert_eq!(
        std::fs::read_to_string(destination.join("opener-argument")).unwrap(),
        destination.canonicalize().unwrap().to_str().unwrap()
    );
    assert!(open_directory_impl(&destination.join("opener-argument")).is_err());
    assert!(open_directory_impl(&dir.join("missing")).is_err());
    write_script(dir.path(), "xdg-open", "#!/bin/sh\nexit 4\n");
    assert!(open_directory_impl(&destination)
        .unwrap_err()
        .contains("系统默认文件管理器"));
    env.set("PATH", dir.join("empty"));
    assert!(open_directory_impl(&destination)
        .unwrap_err()
        .contains("打开文件夹失败"));
}
