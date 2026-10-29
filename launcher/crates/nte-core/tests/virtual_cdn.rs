//! 虚拟 CDN 端到端测试.
//!
//! 在回环地址上启动一个最小的 PatcherSDK 兼容服务, 用真实的 ureq 下载链路
//! 验证检查, 并发下载, 共享限速与断点续传.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use nte_core::game::check::{check, CheckOptions};
use nte_core::game::download::{apply_plan_observed_full, ApplyOptions};
use nte_core::game::pxml;
use nte_core::game::GameUpdateConfig;
use nte_core::update::md5_bytes;

const APP_ID: &str = "1289";
const BRANCH: &str = "publish_ob";
const VERSION: &str = "2.0.0";
const CDN_ROOT: &str = "/clientRes";

#[derive(Default)]
struct CdnState {
    routes: BTreeMap<String, Vec<u8>>,
    targets: Vec<String>,
    ranges: Vec<String>,
}

/// 最小 HTTP 服务: 仅支持 GET, 返回注册的字节并处理 Range.
struct MockCdn {
    addr: SocketAddr,
    state: Arc<Mutex<CdnState>>,
    stop: Arc<AtomicBool>,
}

impl MockCdn {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定虚拟 CDN");
        listener.set_nonblocking(true).expect("设置非阻塞");
        let addr = listener.local_addr().expect("读取监听地址");
        let state = Arc::new(Mutex::new(CdnState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::clone(&state);
        let stopped = Arc::clone(&stop);
        thread::spawn(move || loop {
            if stopped.load(Ordering::Relaxed) {
                return;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let state = Arc::clone(&shared);
                    thread::spawn(move || {
                        let _ = serve(stream, &state);
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(_) => return,
            }
        });
        Self { addr, state, stop }
    }

    fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn route(&self, path: impl Into<String>, body: impl AsRef<[u8]>) {
        self.state
            .lock()
            .expect("锁定虚拟 CDN")
            .routes
            .insert(path.into(), body.as_ref().to_vec());
    }

    fn hits(&self, needle: &str) -> usize {
        self.state
            .lock()
            .expect("锁定虚拟 CDN")
            .targets
            .iter()
            .filter(|target| target.contains(needle))
            .count()
    }

    fn ranges(&self) -> Vec<String> {
        self.state.lock().expect("锁定虚拟 CDN").ranges.clone()
    }
}

impl Drop for MockCdn {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn serve(mut stream: TcpStream, state: &Arc<Mutex<CdnState>>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    if reader.read_line(&mut request)? == 0 {
        return Ok(());
    }
    let method = request
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    let target = request
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    let mut range = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("range") {
                range = Some(value.trim().to_string());
            }
        }
    }
    let path = target
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_string();
    if std::env::var_os("VIRTUAL_CDN_LOG").is_some() {
        eprintln!("virtual-cdn: {method} {path}");
    }
    let body = {
        let mut state = state.lock().expect("锁定虚拟 CDN");
        state.targets.push(target);
        state.ranges.push(range.clone().unwrap_or_default());
        state.routes.get(&path).cloned()
    };
    let Some(body) = body else {
        return write_simple(&mut stream, 404, b"not found");
    };
    if method != "GET" && method != "HEAD" {
        return write_simple(&mut stream, 405, b"method not allowed");
    }
    let Some((start, end)) = range
        .as_deref()
        .and_then(|value| parse_range(value, body.len()))
    else {
        if method == "HEAD" {
            return write_head(&mut stream, 200, body.len());
        }
        return write_body(&mut stream, 200, &body);
    };
    if start >= body.len() {
        let response = format!(
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes())?;
        return stream.flush();
    }
    let end = end.min(body.len() - 1);
    let slice = &body[start..=end];
    if method == "HEAD" {
        return write_head(&mut stream, 206, slice.len());
    }
    let header = format!(
        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
        slice.len(),
        start,
        end,
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(slice)?;
    stream.flush()
}

fn parse_range(value: &str, total: usize) -> Option<(usize, usize)> {
    let spec = value.trim().strip_prefix("bytes=")?;
    let (start, end) = spec.split_once('-')?;
    let start: usize = start.trim().parse().ok()?;
    let end = if end.trim().is_empty() {
        total.saturating_sub(1)
    } else {
        end.trim().parse().ok()?
    };
    Some((start, end))
}

fn write_head(stream: &mut TcpStream, status: u16, length: usize) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status} {}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n",
        reason(status)
    );
    stream.write_all(header.as_bytes())?;
    stream.flush()
}

fn write_simple(stream: &mut TcpStream, status: u16, body: &[u8]) -> std::io::Result<()> {
    write_body(stream, status, body)
}

fn write_body(stream: &mut TcpStream, status: u16, body: &[u8]) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reason(status),
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        405 => "Method Not Allowed",
        416 => "Range Not Satisfiable",
        _ => "Status",
    }
}

fn zip_single(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(name, zip::write::SimpleFileOptions::default())
        .expect("写入 ZIP 条目");
    writer.write_all(bytes).expect("压缩文件");
    writer.finish().expect("完成 ZIP").into_inner()
}

fn payload(size: usize, seed: u8) -> Vec<u8> {
    (0..size)
        .map(|index| (index as u8).wrapping_add(seed))
        .collect()
}

struct Fixture {
    _dir: tempfile::TempDir,
    install: PathBuf,
    config: GameUpdateConfig,
    server: MockCdn,
    contents: Vec<(String, Vec<u8>)>,
}

fn fixture(files: &[(&str, usize, u8)]) -> Fixture {
    let dir = tempfile::tempdir().expect("创建临时目录");
    let install = dir.path().join("install");
    let launcher = install.join("NTELauncher");
    let config_dir = launcher
        .join("ResFilesM")
        .join(APP_ID)
        .join("PatcherConfig");
    fs::create_dir_all(&config_dir).expect("创建配置目录");
    fs::create_dir_all(launcher.join("Config")).expect("创建启动器配置目录");

    let server = MockCdn::start();
    let launcher_base = format!("{}/launcher", server.base());
    server.route(
        "/launcher/Version.ini",
        format!("[VERSION]\nVersion=1.0.0\nBuild=test\nFileListURL={launcher_base}/AllFiles.xml\n"),
    );
    let launcher_file = b"launcher fixture";
    let launcher_zip = zip_single("Config/test.txt", launcher_file);
    server.route("/launcher/Config/test.txt.zip", &launcher_zip);
    server.route(
        "/launcher/AllFiles.xml",
        format!(
            "<Manifest><ProductVersion Version=\"1.0.0\"/><Url BaseUrl=\"{launcher_base}\"/><File Path=\"Config/test.txt\" Checksum=\"{}\" Size=\"{}\" ZipChecksum=\"{}\" ZipSize=\"{}\"/></Manifest>",
            md5_bytes(launcher_file),
            launcher_file.len(),
            md5_bytes(&launcher_zip),
            launcher_zip.len()
        ),
    );
    fs::write(
        launcher.join("Config/Config.ini"),
        format!(
            "[General]\nGameID={APP_ID}\n\n[VERSION]\nVersion=0.0.0\nVersionInfoFileURL={launcher_base}/Version.ini\nBranch={BRANCH}\n"
        ),
    )
    .expect("写入 Config.ini");
    let patcher_dir = launcher.join("UserData/Patcher/PatcherSDK");
    fs::create_dir_all(&patcher_dir).expect("创建本地账本目录");
    fs::write(
        patcher_dir.join("ResList.xml"),
        pxml::encode(
            b"<ResList version=\"0.0.0\" tag=\"baseTag\"></ResList>",
            APP_ID,
        )
        .expect("加密本地账本"),
    )
    .expect("写入本地账本");
    let patcher = serde_json::json!({
        "gameResUrl": [format!("{}{CDN_ROOT}", server.base())],
        "branchName": BRANCH,
        "appId": APP_ID,
    });
    fs::write(
        config_dir.join("PatcherConfig.json"),
        serde_json::to_vec(&patcher).unwrap(),
    )
    .expect("写入 PatcherConfig.json");

    let contents: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(path, size, seed)| ((*path).to_string(), payload(*size, *seed)))
        .collect();
    let mut xml = format!("<ResList version=\"{VERSION}\" tag=\"baseTag\">");
    for (path, bytes) in &contents {
        xml.push_str(&format!(
            "<Res filename=\"{path}\" md5=\"{}\" filesize=\"{}\" />",
            md5_bytes(bytes),
            bytes.len()
        ));
        let md5 = md5_bytes(bytes).to_ascii_lowercase();
        server.route(
            format!(
                "{CDN_ROOT}/{BRANCH}/Res/{}/{md5}.{}",
                md5.chars().next().unwrap_or('0'),
                bytes.len()
            ),
            bytes,
        );
    }
    xml.push_str("</ResList>");

    let encoded = pxml::encode(xml.as_bytes(), APP_ID).expect("加密 ResList");
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("ResList.bin", zip::write::SimpleFileOptions::default())
        .expect("写入 ZIP 条目");
    writer.write_all(&encoded).expect("压缩 ResList");
    let zip = writer.finish().expect("完成 ZIP").into_inner();
    let base = format!("{CDN_ROOT}/{BRANCH}/Version/Windows");
    server.route(
        format!("{base}/config.xml"),
        format!(
            "<Config><ResVersion>{VERSION}</ResVersion><Hash>test-hash</Hash><listHash>{}</listHash><maxThreadCnt>4</maxThreadCnt></Config>",
            md5_bytes(xml.as_bytes())
        ),
    );
    server.route(format!("{base}/version/{VERSION}/ResList.bin.zip"), zip);

    let config = GameUpdateConfig::load(&launcher, APP_ID.parse().unwrap()).expect("加载游戏配置");
    Fixture {
        _dir: dir,
        install,
        config,
        server,
        contents,
    }
}

impl Fixture {
    fn assert_installed(&self) {
        for (path, bytes) in &self.contents {
            let installed = self.install.join(path);
            assert_eq!(
                fs::read(&installed).expect("读取已安装文件"),
                *bytes,
                "{path} 内容不一致"
            );
        }
    }
}

/// 手动联调: 启动虚拟 CDN 并打印可直接执行的 nte-cli 命令.
///
/// `cargo test -p nte-core --test virtual_cdn manual_server -- --ignored --nocapture`
#[test]
#[ignore = "手动启动虚拟 CDN, 默认保留 600 秒"]
fn manual_server() {
    let fixture = fixture(&[
        (
            "Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe",
            4096,
            3,
        ),
        (
            "Client/WindowsNoEditor/HT/Content/data.pak",
            8 * 1024 * 1024,
            9,
        ),
    ]);
    let launcher = fixture.install.join("NTELauncher");
    println!("VIRTUAL_CDN_URL={}", fixture.server.base());
    println!("VIRTUAL_CDN_INSTALL={}", fixture.install.display());
    println!("VIRTUAL_CDN_LAUNCHER={}", launcher.display());
    println!(
        "ODDITY_LAUNCHER_VERSION_URL={}/launcher/Version.ini",
        fixture.server.base()
    );
    println!(
        "cargo run -p nte-cli -- game-check --root {}",
        launcher.display()
    );
    println!(
        "cargo run -p nte-cli -- game-apply --root {}",
        launcher.display()
    );
    let seconds = std::env::var("VIRTUAL_CDN_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(600);
    thread::sleep(Duration::from_secs(seconds));
}

#[test]
fn virtual_cdn_check_and_download_round_trip() {
    let fixture = fixture(&[
        (
            "Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe",
            1024,
            1,
        ),
        (
            "Client/WindowsNoEditor/HT/Content/data.pak",
            2 * 1024 * 1024,
            7,
        ),
    ]);

    let plan = check(
        &fixture.config,
        CheckOptions {
            full: false,
            include_optional: false,
        },
    )
    .expect("检查虚拟 CDN");
    assert_eq!(plan.items.len(), 2);
    assert_eq!(plan.remote_version, VERSION);
    assert_eq!(
        plan.total_bytes(),
        fixture
            .contents
            .iter()
            .map(|(_, bytes)| bytes.len() as u64)
            .sum::<u64>()
    );

    let report = apply_plan_observed_full(
        &fixture.config,
        &plan,
        &ApplyOptions::default(),
        0,
        &mut |_| {},
        &mut |_, _| {},
        &mut |_, _| {},
        &mut |_| {},
        &|| false,
    );
    assert_eq!(report.failed, 0, "{:?}", report.results);
    assert!(!report.cancelled);
    fixture.assert_installed();
    assert!(fixture.server.hits("/config.xml") >= 1);
    assert!(fixture.server.hits("/version/2.0.0/ResList.bin.zip") >= 1);
    assert!(fixture.server.hits("/Res/") >= 2);

    let plan = check(&fixture.config, CheckOptions::default()).expect("下载后再次检查");
    assert!(plan.items.is_empty(), "{:?}", plan.items);
}

#[test]
fn virtual_cdn_shares_the_bandwidth_limit_across_workers() {
    let paths: Vec<String> = (0..8)
        .map(|index| format!("Client/WindowsNoEditor/HT/Content/part-{index}.bin"))
        .collect();
    let files: Vec<(&str, usize, u8)> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| (path.as_str(), 1024 * 1024, index as u8))
        .collect();
    let fixture = fixture(&files);
    let plan = check(&fixture.config, CheckOptions::default()).expect("检查虚拟 CDN");
    assert_eq!(plan.items.len(), 8);
    assert_eq!(plan.max_threads, 4);

    let limit = 4 * 1024 * 1024;
    let started = Instant::now();
    let report = apply_plan_observed_full(
        &fixture.config,
        &plan,
        &ApplyOptions::default(),
        limit,
        &mut |_| {},
        &mut |_, _| {},
        &mut |_, _| {},
        &mut |_| {},
        &|| false,
    );
    let elapsed = started.elapsed();
    assert_eq!(report.failed, 0, "{:?}", report.results);
    assert_eq!(report.downloaded_bytes, 8 * 1024 * 1024);
    println!(
        "virtual cdn limit: {} bytes in {elapsed:?} (limit 4 MiB/s)",
        report.downloaded_bytes
    );
    // 8 MiB 在 4 MiB/s 的共享限速下至少需要约 1.75 秒; 只做宽松下界,
    // 避免机器负载造成误报.
    assert!(
        elapsed >= Duration::from_millis(1400),
        "共享限速未生效: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "下载耗时异常: {elapsed:?}"
    );
    fixture.assert_installed();
}

#[test]
fn virtual_cdn_resumes_after_cancellation() {
    let fixture = fixture(&[
        (
            "Client/WindowsNoEditor/HT/Content/a.bin",
            4 * 1024 * 1024,
            1,
        ),
        (
            "Client/WindowsNoEditor/HT/Content/b.bin",
            4 * 1024 * 1024,
            2,
        ),
    ]);
    let plan = check(&fixture.config, CheckOptions::default()).expect("检查虚拟 CDN");
    assert_eq!(plan.items.len(), 2);

    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let timer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(700));
        flag.store(true, Ordering::Relaxed);
    });
    let report = apply_plan_observed_full(
        &fixture.config,
        &plan,
        &ApplyOptions::default(),
        2 * 1024 * 1024,
        &mut |_| {},
        &mut |_, _| {},
        &mut |_, _| {},
        &mut |_| {},
        &|| cancel.load(Ordering::Relaxed),
    );
    timer.join().expect("等待取消定时器");
    assert!(report.cancelled, "限速下载应被取消");
    assert!(fixture.server.hits("/Res/") >= 1, "取消前应已发起下载");

    // 再次检查后继续下载, 复用已下载的分片.
    let plan = check(&fixture.config, CheckOptions::default()).expect("重新检查");
    let report = apply_plan_observed_full(
        &fixture.config,
        &plan,
        &ApplyOptions::default(),
        0,
        &mut |_| {},
        &mut |_, _| {},
        &mut |_, _| {},
        &mut |_| {},
        &|| false,
    );
    assert_eq!(report.failed, 0, "{:?}", report.results);
    fixture.assert_installed();
    assert!(
        fixture
            .server
            .ranges()
            .iter()
            .any(|range| !range.is_empty()),
        "续传应使用 Range 请求"
    );
}
