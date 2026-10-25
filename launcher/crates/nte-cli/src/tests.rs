//! nte-cli 的命令实现测试。
//!
//! 这些函数只往标准输出打印进度，因此断言集中在返回值与文件系统结果上，
//! 网络请求交给回环 HTTP 服务。

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("nte-cli-{tag}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&path).expect("创建临时目录");
    path
}

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_file(root: &Path, rel: &str, contents: impl AsRef<[u8]>) -> PathBuf {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("创建父目录");
    }
    fs::write(&path, contents).expect("写入文件");
    path
}

fn launcher_config(root: &Path, version_url: &str) -> LauncherConfig {
    LauncherConfig {
        root: root.to_path_buf(),
        launcher: "NTELauncher.exe".to_string(),
        patcher: "NTEUpdate.exe".to_string(),
        client: "NTEGame.exe".to_string(),
        game_id: 1289,
        launch_type: 0,
        version: "1.4.16".to_string(),
        build: "20240101".to_string(),
        version_url: version_url.to_string(),
        backup_version_url: "https://cdn.test/Backup.ini".to_string(),
        branch: "publish_ob".to_string(),
        patcher_ignored_sections: Vec::new(),
        mutex_name: "nte-mutex".to_string(),
    }
}

/// 一个最小可用的 Patcher 配置目录。
fn game_fixture(root: &Path, app_id: &str) {
    write_file(
        root,
        "ResFilesM/1289/PatcherConfig/PatcherConfig.json",
        format!(
            "{{\"gameResUrl\":[\"{{cdn}}\"],\"branchName\":\"publish_ob\",\"appId\":\"{app_id}\"}}"
        ),
    );
}

fn rewrite_cdn(root: &Path, cdn: &str) {
    write_file(
        root,
        "ResFilesM/1289/PatcherConfig/PatcherConfig.json",
        format!("{{\"gameResUrl\":[\"{cdn}\"],\"branchName\":\"publish_ob\",\"appId\":\"1289\"}}"),
    );
}

fn zip_one(name: &str, contents: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file(name, zip::write::SimpleFileOptions::default())
        .expect("start zip entry");
    writer.write_all(contents).expect("write zip entry");
    writer.finish().expect("finish zip").into_inner()
}

fn res_zip(res_list: &str, diff: Option<&str>) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("ResList.bin", zip::write::SimpleFileOptions::default())
        .expect("start entry");
    writer
        .write_all(&nte_core::game::pxml::encode(res_list.as_bytes(), "1289").expect("encode"))
        .expect("write entry");
    if let Some(diff) = diff {
        writer
            .start_file("LastDiff.bin", zip::write::SimpleFileOptions::default())
            .expect("start entry");
        writer
            .write_all(&nte_core::game::pxml::encode(diff.as_bytes(), "1289").expect("encode"))
            .expect("write entry");
    }
    writer.finish().expect("finish zip").into_inner()
}

fn game_config_xml(res_version: &str, res_list: &str, diff_hash: Option<&str>) -> String {
    let mut xml = format!(
        "<Config><ResVersion>{res_version}</ResVersion><Hash>HASH</Hash><Tag>baseTag</Tag><listHash>{}</listHash>",
        nte_core::update::md5_bytes(res_list.as_bytes())
    );
    if let Some(hash) = diff_hash {
        xml.push_str(&format!("<diffHash>{hash}</diffHash>"));
    }
    xml.push_str("</Config>");
    xml
}

// ---------------------------------------------------------------------------
// 回环 HTTP 服务
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ServerState {
    routes: HashMap<String, Vec<u8>>,
    targets: Vec<String>,
}

struct HttpServer {
    addr: SocketAddr,
    state: Arc<Mutex<ServerState>>,
    stop: Arc<AtomicBool>,
}

impl HttpServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定端口");
        let addr = listener.local_addr().expect("读取监听地址");
        listener.set_nonblocking(true).expect("设置非阻塞");
        let state = Arc::new(Mutex::new(ServerState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_state = Arc::clone(&state);
        let worker_stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = Arc::clone(&worker_state);
                        std::thread::spawn(move || {
                            let _ = serve(stream, &state);
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self { addr, state, stop }
    }

    fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base(), path)
    }

    fn route(&self, path: impl AsRef<str>, body: impl AsRef<[u8]>) {
        self.state
            .lock()
            .expect("锁定服务状态")
            .routes
            .insert(path.as_ref().to_string(), body.as_ref().to_vec());
    }

    fn targets(&self) -> Vec<String> {
        self.state.lock().expect("锁定服务状态").targets.clone()
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn serve(mut stream: TcpStream, state: &Arc<Mutex<ServerState>>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(());
    }
    let target = line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        if header.trim_end().is_empty() {
            break;
        }
    }
    let path = target
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_string();
    let route = {
        let mut state = state.lock().expect("锁定服务状态");
        state.targets.push(target);
        state.routes.get(&path).cloned()
    };
    let (status, body) = match route {
        Some(body) => (200, body),
        None => (404, b"not found".to_vec()),
    };
    let head = format!(
        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}

// ---------------------------------------------------------------------------
// 参数与本地检查
// ---------------------------------------------------------------------------

#[test]
fn flag_value_reads_the_following_argument() {
    let args: Vec<String> = ["--root", "/games", "--only", "a.bin", "--full"]
        .iter()
        .map(|value| value.to_string())
        .collect();
    assert_eq!(flag_value(&args, "--root").as_deref(), Some("/games"));
    assert_eq!(flag_value(&args, "--only").as_deref(), Some("a.bin"));
    assert_eq!(flag_value(&args, "--full"), None);
    assert_eq!(flag_value(&args, "--missing"), None);
    // 重复出现时取第一个
    let repeated: Vec<String> = ["--only", "a", "--only", "b"]
        .iter()
        .map(|value| value.to_string())
        .collect();
    assert_eq!(flag_value(&repeated, "--only").as_deref(), Some("a"));
    // 前缀相同不算命中
    assert_eq!(flag_value(&args, "--roo"), None);
}

#[test]
fn usage_always_succeeds() {
    usage().expect("usage");
}

#[test]
fn doctor_prints_the_launcher_layout() {
    let root = temp_dir("doctor");
    let _cleanup = Cleanup(root.clone());
    let config = launcher_config(&root, "https://cdn.test/Version.ini");
    doctor(&config).expect("doctor");
}

#[test]
fn game_config_reports_a_missing_patcher_directory() {
    let root = temp_dir("game-config");
    let _cleanup = Cleanup(root.clone());
    let config = launcher_config(&root, "https://cdn.test/Version.ini");
    let error = game_config(&config).expect_err("no PatcherConfig");
    assert!(
        format!("{error:#}").contains("load PatcherConfig"),
        "{error:#}"
    );
}

#[test]
fn check_reports_the_local_and_remote_versions() {
    let root = temp_dir("check");
    let _cleanup = Cleanup(root.clone());
    let server = HttpServer::start();
    write_file(&root, "data/ok.txt", b"hello");
    server.route(
        "/Version.ini",
        format!(
            "[VERSION]\nVersion=1.4.20\nBuild=20240102\nFileListURL={}\n",
            server.url("/files/AllFiles.xml")
        ),
    );
    server.route(
        "/files/AllFiles.xml",
        format!(
            "<AllFiles><ProductVersion Version=\"1.4.20\"/><Url BaseUrl=\"{}/files/\"/><File Path=\"/data/ok.txt\" Checksum=\"{}\" Size=\"5\"/><File Path=\"/data/gone.txt\" Checksum=\"00\" Size=\"3\"/></AllFiles>",
            server.base(),
            nte_core::update::md5_bytes(b"hello")
        ),
    );
    let config = launcher_config(&root, &server.url("/Version.ini"));

    check(&config, true).expect("quick check");
    check(&config, false).expect("full check");
    assert!(!root.join("data/gone.txt").exists());
}

#[test]
fn check_reports_fetch_failures() {
    let root = temp_dir("check-errors");
    let _cleanup = Cleanup(root.clone());
    let server = HttpServer::start();

    // Version.ini 取不到
    let config = launcher_config(&root, &server.url("/Version.ini"));
    let error = check(&config, true).expect_err("no version");
    assert!(
        format!("{error:#}").contains("fetch Version.ini"),
        "{error:#}"
    );

    // 远端缺少 FileListURL
    server.route("/Version.ini", "[VERSION]\nVersion=1.4.20\n");
    let error = check(&config, true).expect_err("no file list");
    assert!(
        format!("{error:#}").contains("has no FileListURL"),
        "{error:#}"
    );

    // AllFiles.xml 取不到
    server.route(
        "/Version.ini",
        format!(
            "[VERSION]\nVersion=1.4.20\nFileListURL={}\n",
            server.url("/AllFiles.xml")
        ),
    );
    let error = check(&config, true).expect_err("no manifest");
    assert!(
        format!("{error:#}").contains("fetch AllFiles.xml"),
        "{error:#}"
    );
}

#[test]
fn apply_downloads_missing_files() {
    let install = temp_dir("apply");
    let _cleanup = Cleanup(install.clone());
    let root = install.join("NTELauncher");
    fs::create_dir_all(&root).unwrap();
    let server = HttpServer::start();
    let content = b"new content";
    let zip = zip_one("data/new.txt", content);
    server.route("/files/data/new.txt.zip", &zip);
    server.route(
        "/Version.ini",
        format!(
            "[VERSION]\nVersion=1.4.20\nFileListURL={}\n",
            server.url("/files/AllFiles.xml")
        ),
    );
    server.route(
        "/files/AllFiles.xml",
        format!(
            "<AllFiles><File Path=\"/data/new.txt\" Checksum=\"{}\" Size=\"{}\"/><File Path=\"/data/skip.txt\" Checksum=\"{}\" Size=\"{}\"/></AllFiles>",
            nte_core::update::md5_bytes(content),
            content.len(),
            nte_core::update::md5_bytes(b"kept"),
            4
        ),
    );
    write_file(&root, "data/skip.txt", b"evil");
    server.route(
        "/files/data/skip.txt.zip",
        zip_one("data/skip.txt", b"kept"),
    );
    let config = launcher_config(&root, &server.url("/Version.ini"));

    apply(&config, None).expect("apply");
    assert_eq!(
        fs::read(root.join("data/new.txt")).expect("read file"),
        content
    );

    assert_eq!(fs::read(root.join("data/skip.txt")).unwrap(), b"kept");

    // --only 过滤掉不匹配的文件
    let filtered = launcher_config(&root, &server.url("/Version.ini"));
    apply(&filtered, Some("nothing-matches".to_string())).expect("apply with filter");
}

#[test]
fn apply_reports_version_failures() {
    let install = temp_dir("apply-errors");
    let _cleanup = Cleanup(install.clone());
    let root = install.join("NTELauncher");
    fs::create_dir_all(&root).unwrap();
    let server = HttpServer::start();
    let config = launcher_config(&root, &server.url("/Version.ini"));

    let error = apply(&config, None).expect_err("no version");
    assert!(
        format!("{error:#}").contains("fetch Version.ini"),
        "{error:#}"
    );

    server.route("/Version.ini", "[VERSION]\nVersion=1.4.20\n");
    let error = apply(&config, None).expect_err("no file list");
    assert!(
        format!("{error:#}").contains("has no FileListURL"),
        "{error:#}"
    );

    // 文件失败必须反馈给脚本调用方。
    server.route(
        "/Version.ini",
        format!(
            "[VERSION]\nVersion=1.4.20\nFileListURL={}\n",
            server.url("/files/AllFiles.xml")
        ),
    );
    server.route(
        "/files/AllFiles.xml",
        "<AllFiles><File Path=\"/data/gone.txt\" Checksum=\"00\" Size=\"3\"/></AllFiles>",
    );
    assert!(apply(&config, None)
        .unwrap_err()
        .to_string()
        .contains("1 launcher update file(s) failed"));
    assert!(!root.join("data/gone.txt").exists());
}

// ---------------------------------------------------------------------------
// 游戏资源更新
// ---------------------------------------------------------------------------

/// 游戏安装目录形态：<base>/install/NTELauncher，资源装在 <base>/install 下。
struct GameInstall {
    base: PathBuf,
    launcher_root: PathBuf,
    _cleanup: Cleanup,
}

fn game_install(tag: &str) -> GameInstall {
    let base = temp_dir(tag);
    let launcher_root = base.join("install/NTELauncher");
    fs::create_dir_all(&launcher_root).expect("创建启动器目录");
    game_fixture(&launcher_root, "1289");
    GameInstall {
        base: base.clone(),
        launcher_root,
        _cleanup: Cleanup(base),
    }
}

impl GameInstall {
    fn launcher(&self, version_url: &str) -> LauncherConfig {
        launcher_config(&self.launcher_root, version_url)
    }

    fn resource(&self, rel: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        write_file(&self.base.join("install"), rel, contents)
    }

    fn resource_path(&self, rel: &str) -> PathBuf {
        self.base.join("install").join(rel)
    }
}

#[test]
fn game_check_walks_the_remote_plan() {
    let server = HttpServer::start();
    let install = game_install("game-check");
    rewrite_cdn(&install.launcher_root, &server.url("/cdn"));
    install.resource("data/present.bin", b"hello");

    let res_list = "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/present.bin\" md5=\"00\" filesize=\"5\"/><Res filename=\"data/gone.bin\" md5=\"11\" filesize=\"7\"/></ResList>";
    server.route(
        "/cdn/publish_ob/Version/Windows/config.xml",
        game_config_xml("1.4.16", res_list, None),
    );
    server.route(
        "/cdn/publish_ob/Version/Windows/version/1.4.16/ResList.bin.zip",
        res_zip(res_list, None),
    );

    let launcher = install.launcher("https://cdn.test/Version.ini");
    game_check(&launcher, &[]).expect("game check");

    // --full 会重新哈希本地文件
    let full: Vec<String> = vec!["--full".to_string(), "--include-optional".to_string()];
    game_check(&launcher, &full).expect("full game check");

    // config.xml 请求带上了时间戳参数
    assert!(
        server
            .targets()
            .iter()
            .any(|target| target.starts_with("/cdn/publish_ob/Version/Windows/config.xml?tValue=")),
        "{:?}",
        server.targets()
    );
}

#[test]
fn game_check_reports_unverified_large_files() {
    let server = HttpServer::start();
    let install = game_install("game-check-unverified");
    rewrite_cdn(&install.launcher_root, &server.url("/cdn"));

    // 超过 20 MiB 且没有本地账本：只上报待确认，不哈希
    let big = install.resource_path("data/big.pak");
    fs::create_dir_all(big.parent().expect("parent")).expect("创建目录");
    fs::File::create(&big)
        .expect("创建文件")
        .set_len(21 * 1024 * 1024)
        .expect("设置长度");

    let res_list = format!(
        "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/big.pak\" md5=\"{}\" filesize=\"{}\"/></ResList>",
        "cc".repeat(16),
        21 * 1024 * 1024
    );
    server.route(
        "/cdn/publish_ob/Version/Windows/config.xml",
        game_config_xml("1.4.16", &res_list, None),
    );
    server.route(
        "/cdn/publish_ob/Version/Windows/version/1.4.16/ResList.bin.zip",
        res_zip(&res_list, None),
    );

    let launcher = install.launcher("https://cdn.test/Version.ini");
    game_check(&launcher, &[]).expect("game check");
    // 没有 --full 时大文件不会被下载
    assert!(!install.resource_path("data/big.pak.tmp").exists());
}

#[test]
fn game_check_fails_without_a_patcher_config() {
    let root = temp_dir("game-check-missing");
    let _cleanup = Cleanup(root.clone());
    let launcher = launcher_config(&root, "https://cdn.test/Version.ini");
    let error = game_check(&launcher, &[]).expect_err("no config");
    assert!(
        format!("{error:#}").contains("load PatcherConfig"),
        "{error:#}"
    );
}

#[test]
fn game_apply_dry_run_and_filters() {
    let server = HttpServer::start();
    let install = game_install("game-apply-dry");
    rewrite_cdn(&install.launcher_root, &server.url("/cdn"));

    let res_list = "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/gone.bin\" md5=\"11\" filesize=\"7\"/></ResList>";
    server.route(
        "/cdn/publish_ob/Version/Windows/config.xml",
        game_config_xml("1.4.16", res_list, None),
    );
    server.route(
        "/cdn/publish_ob/Version/Windows/version/1.4.16/ResList.bin.zip",
        res_zip(res_list, None),
    );
    let launcher = install.launcher("https://cdn.test/Version.ini");

    let dry_run: Vec<String> = vec!["--dry-run".to_string()];
    game_apply(&launcher, &dry_run).expect("dry run");
    assert!(!install.resource_path("data/gone.bin").exists());

    // 过滤条件没有命中任何文件
    let filtered: Vec<String> = vec!["--only".to_string(), "nothing".to_string()];
    game_apply(&launcher, &filtered).expect("filtered apply");
    assert!(!install.resource_path("data/gone.bin").exists());
}

#[test]
fn game_apply_reports_nothing_to_do() {
    let server = HttpServer::start();
    let install = game_install("game-apply-empty");
    rewrite_cdn(&install.launcher_root, &server.url("/cdn"));
    install.resource("data/present.bin", b"hello");

    let res_list = format!(
        "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/present.bin\" md5=\"{}\" filesize=\"5\"/></ResList>",
        nte_core::update::md5_bytes(b"hello")
    );
    server.route(
        "/cdn/publish_ob/Version/Windows/config.xml",
        game_config_xml("1.4.16", &res_list, None),
    );
    server.route(
        "/cdn/publish_ob/Version/Windows/version/1.4.16/ResList.bin.zip",
        res_zip(&res_list, None),
    );

    let launcher = install.launcher("https://cdn.test/Version.ini");
    game_apply(&launcher, &[]).expect("nothing to do");
    let cfg = game_config(&launcher).unwrap();
    assert!(game::check::load_local_ledger(&cfg).unwrap().is_some());
}

#[test]
fn game_apply_downloads_and_updates_the_ledger() {
    let server = HttpServer::start();
    let install = game_install("game-apply");
    rewrite_cdn(&install.launcher_root, &server.url("/cdn"));
    let content = b"fresh content";
    let md5 = nte_core::update::md5_bytes(content);
    server.route(
        format!("/cdn/publish_ob/Res/{}/{md5}.{}", &md5[..1], content.len()),
        content,
    );

    let res_list = format!(
        "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/fresh.bin\" md5=\"{md5}\" filesize=\"{}\"/></ResList>",
        content.len()
    );
    server.route(
        "/cdn/publish_ob/Version/Windows/config.xml",
        game_config_xml("1.4.16", &res_list, None),
    );
    server.route(
        "/cdn/publish_ob/Version/Windows/version/1.4.16/ResList.bin.zip",
        res_zip(&res_list, None),
    );

    let launcher = install.launcher("https://cdn.test/Version.ini");
    game_apply(&launcher, &[]).expect("apply");
    assert!(install.resource_path("data/fresh.bin").is_file());
    assert_eq!(
        fs::read(install.resource_path("data/fresh.bin")).expect("读取文件"),
        content
    );
    // 账本被写回，可以直接读出来
    assert!(install
        .launcher_root
        .join("UserData/Patcher/PatcherSDK/ResList.xml")
        .is_file());
}

#[test]
fn game_apply_fails_when_files_cannot_be_downloaded() {
    let server = HttpServer::start();
    let install = game_install("game-apply-failed");
    rewrite_cdn(&install.launcher_root, &server.url("/cdn"));

    let res_list = "<ResList version=\"1.4.16\" tag=\"baseTag\"><Res filename=\"data/gone.bin\" md5=\"11\" filesize=\"7\"/></ResList>";
    server.route(
        "/cdn/publish_ob/Version/Windows/config.xml",
        game_config_xml("1.4.16", res_list, None),
    );
    server.route(
        "/cdn/publish_ob/Version/Windows/version/1.4.16/ResList.bin.zip",
        res_zip(res_list, None),
    );

    let launcher = install.launcher("https://cdn.test/Version.ini");
    let error = game_apply(&launcher, &[]).expect_err("download failed");
    assert!(
        format!("{error:#}").contains("game update file(s) failed"),
        "{error:#}"
    );
}

#[test]
fn game_apply_reports_check_failures() {
    let server = HttpServer::start();
    let install = game_install("game-apply-check");
    rewrite_cdn(&install.launcher_root, &server.url("/cdn"));
    let launcher = install.launcher("https://cdn.test/Version.ini");

    let error = game_apply(&launcher, &[]).expect_err("no cdn data");
    assert!(
        format!("{error:#}").contains("check game resources"),
        "{error:#}"
    );
}

#[test]
fn game_apply_verifies_unknown_files_and_repairs_corruption() {
    let server = HttpServer::start();
    let install = game_install("game-apply-unverified");
    rewrite_cdn(&install.launcher_root, &server.url("/cdn"));
    let content = vec![7u8; 21 * 1024 * 1024];
    let md5 = update::md5_bytes(&content);
    let path = "data/big.pak";
    install.resource(path, &content);
    let res_list = format!("<ResList version=\"1.4.16\"><Res filename=\"{path}\" md5=\"{md5}\" filesize=\"{}\"/></ResList>", content.len());
    server.route(
        "/cdn/publish_ob/Version/Windows/config.xml",
        game_config_xml("1.4.16", &res_list, None),
    );
    server.route(
        "/cdn/publish_ob/Version/Windows/version/1.4.16/ResList.bin.zip",
        res_zip(&res_list, None),
    );
    let launcher = install.launcher("https://cdn.test/Version.ini");
    game_apply(&launcher, &[]).expect("verify without download");
    let cfg = game_config(&launcher).unwrap();
    assert!(game::check::load_local_ledger(&cfg).unwrap().is_some());
    assert!(!server
        .targets()
        .iter()
        .any(|target| target.contains("/Res/")));

    fs::remove_file(cfg.patcher_dir.join("ResList.xml")).unwrap();
    fs::write(install.resource_path(path), vec![0u8; content.len()]).unwrap();
    server.route(
        format!("/cdn/publish_ob/Res/{}/{md5}.{}", &md5[..1], content.len()),
        &content,
    );
    game_apply(&launcher, &[]).expect("repair unknown file");
    assert_eq!(update::md5_file(&install.resource_path(path)).unwrap(), md5);
    assert!(game::check::load_local_ledger(&cfg).unwrap().is_some());
}

#[test]
fn installation_lock_blocks_both_cli_writers_before_network_requests() {
    let install = temp_dir("lock-cli-writers");
    let _cleanup = Cleanup(install.clone());
    let root = install.join("NTELauncher");
    game_fixture(&root, "1289");
    let server = HttpServer::start();
    rewrite_cdn(&root, &server.base());
    let config = launcher_config(&root, &server.url("/Version.ini"));
    let _lock = nte_core::install_lock::InstallLock::acquire(&install).unwrap();
    assert!(apply(&config, None)
        .unwrap_err()
        .to_string()
        .contains("其他任务"));
    assert!(game_apply(&config, &[])
        .unwrap_err()
        .to_string()
        .contains("其他任务"));
}
