use super::*;
use crate::*;
use std::time::Instant;

#[test]
fn detects_wayland_sessions() {
    let mut env = EnvGuard::new();
    env.remove("GDK_BACKEND");
    env.set("XDG_SESSION_TYPE", "wayland");
    env.remove("WAYLAND_DISPLAY");
    assert!(is_wayland_session());

    env.set("XDG_SESSION_TYPE", "WAYLAND");
    assert!(is_wayland_session());

    env.set("XDG_SESSION_TYPE", "x11");
    assert!(!is_wayland_session());

    env.remove("XDG_SESSION_TYPE");
    env.set("WAYLAND_DISPLAY", "wayland-0");
    assert!(is_wayland_session());

    env.remove("WAYLAND_DISPLAY");
    assert!(!is_wayland_session());
}

#[test]
fn explicit_gtk_backend_takes_precedence_over_the_desktop_session() {
    let mut env = EnvGuard::new();
    env.set("XDG_SESSION_TYPE", "wayland");
    env.set("WAYLAND_DISPLAY", "wayland-0");
    env.set("GDK_BACKEND", "x11");
    assert!(!is_wayland_session());

    env.set("XDG_SESSION_TYPE", "x11");
    env.remove("WAYLAND_DISPLAY");
    env.set("GDK_BACKEND", "wayland");
    assert!(is_wayland_session());
}

#[test]
fn nvidia_xwayland_uses_the_x11_renderer_configuration() {
    let mut env = EnvGuard::new();
    env.remove("WEBKIT_DMABUF_RENDERER_FORCE_SHM");
    env.remove("WEBKIT_GST_DISABLE_GL_SINK");
    env.remove("WEBKIT_GST_ALLOW_PLAYBACK_OF_INVISIBLE_VIDEOS");
    env.remove("NTE_SOFTWARE_RENDER");
    env.set("GDK_BACKEND", "x11");
    env.set("XDG_SESSION_TYPE", "wayland");
    configure_graphics();
    assert_eq!(
        std::env::var_os("WEBKIT_DMABUF_RENDERER_FORCE_SHM").is_some(),
        nvidia_driver_loaded()
    );
}

#[test]
fn configure_graphics_honours_the_software_render_switch() {
    let mut env = EnvGuard::new();
    env.remove("WEBKIT_DMABUF_RENDERER_FORCE_SHM");
    env.remove("WEBKIT_GST_DISABLE_GL_SINK");
    env.remove("WEBKIT_GST_ALLOW_PLAYBACK_OF_INVISIBLE_VIDEOS");
    env.remove("WEBKIT_DISABLE_DMABUF_RENDERER");
    env.remove("WEBKIT_DISABLE_COMPOSITING_MODE");
    env.remove("LIBGL_ALWAYS_SOFTWARE");

    env.set("NTE_SOFTWARE_RENDER", "1");
    configure_graphics();
    assert_eq!(
        std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").as_deref(),
        Ok("1")
    );
}

#[test]
fn configure_graphics_keeps_hardware_acceleration_by_default() {
    let mut env = EnvGuard::new();
    env.remove("WEBKIT_DMABUF_RENDERER_FORCE_SHM");
    env.remove("WEBKIT_GST_DISABLE_GL_SINK");
    env.remove("WEBKIT_GST_ALLOW_PLAYBACK_OF_INVISIBLE_VIDEOS");
    env.remove("NTE_SOFTWARE_RENDER");
    env.set("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    env.set("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    env.set("LIBGL_ALWAYS_SOFTWARE", "1");
    env.set("XDG_SESSION_TYPE", "wayland");
    env.set("GDK_BACKEND", "wayland");
    env.remove("WAYLAND_DISPLAY");

    configure_graphics();

    assert!(std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none());
    assert!(std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none());
    assert!(std::env::var_os("LIBGL_ALWAYS_SOFTWARE").is_none());
    // 没有 NVIDIA 驱动时不应该设置显式同步开关
    if !nvidia_driver_loaded() {
        assert!(std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none());
    }
}

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
    while !log.is_file() && Instant::now() < deadline {
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
fn standalone_build_embeds_frontend_assets() {
    assert!(!tauri::is_dev(), "独立构建必须使用内嵌资源");
    let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
    for path in [
        "index.html",
        "assets/app-icon.svg",
        "assets/hero-poster.jpg",
    ] {
        assert!(
            context.assets().get(&path.into()).is_some(),
            "内嵌资源缺失: {path}"
        );
    }
    let assets: Vec<_> = context.assets().iter().map(|(path, _)| path).collect();
    assert!(assets.iter().any(|path| path.ends_with(".js")));
    assert!(assets.iter().any(|path| path.ends_with(".css")));
}

#[test]
fn launcher_builder_registers_native_dialogs() {
    let app = launcher_builder::<tauri::test::MockRuntime>()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("launcher initialization");
    assert!(app
        .try_state::<tauri_plugin_dialog::Dialog<tauri::test::MockRuntime>>()
        .is_some());
}
