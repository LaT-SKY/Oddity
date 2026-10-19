//! 测试用的临时目录与本地 HTTP 服务。
//!
//! 网络相关的代码（Version.ini、config.xml、ResList.bin.zip、资源下载）需要
//! 真实走一遍请求和响应，这里用一个只监听回环地址的最小 HTTP 服务代替外网，
//! 测试因此完全离线，也不依赖真实 CDN。

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// 修改进程级环境变量的测试必须持有这把锁，否则并行测试会互相干扰。
pub static ENV_LOCK: Mutex<()> = Mutex::new(());

/// 获取环境变量锁；即使之前的测试 panic 过也继续执行。
pub fn lock_env() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

/// 析构时自动删除的临时目录。
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("nte-test-{tag}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).expect("创建临时目录");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn join(&self, rel: impl AsRef<Path>) -> PathBuf {
        self.path.join(rel)
    }

    pub fn write(&self, rel: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> PathBuf {
        let full = self.path.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("创建父目录");
        }
        fs::write(&full, contents).expect("写入文件");
        full
    }

    pub fn read(&self, rel: impl AsRef<Path>) -> Vec<u8> {
        fs::read(self.path.join(rel)).expect("读取文件")
    }

    pub fn read_text(&self, rel: impl AsRef<Path>) -> String {
        fs::read_to_string(self.path.join(rel)).expect("读取文本文件")
    }

    pub fn is_file(&self, rel: impl AsRef<Path>) -> bool {
        self.path.join(rel).is_file()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Clone)]
struct Route {
    body: Vec<u8>,
}

#[derive(Default)]
struct ServerState {
    routes: HashMap<String, Route>,
    targets: Vec<String>,
}

/// 只服务已注册路径的回环 HTTP 服务。
pub struct HttpServer {
    addr: SocketAddr,
    state: Arc<Mutex<ServerState>>,
    stop: Arc<AtomicBool>,
}

impl HttpServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定测试服务端口");
        let addr = listener.local_addr().expect("读取监听地址");
        listener.set_nonblocking(true).expect("设置非阻塞");
        let state = Arc::new(Mutex::new(ServerState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_state = Arc::clone(&state);
        let worker_stop = Arc::clone(&stop);
        thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = Arc::clone(&worker_state);
                        thread::spawn(move || {
                            let _ = serve(stream, &state);
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self { addr, state, stop }
    }

    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base(), path)
    }

    /// 注册一个 200 响应，按 Range 头返回部分内容或完整内容。
    pub fn route(&self, path: impl AsRef<str>, body: impl AsRef<[u8]>) {
        let mut state = self.state.lock().expect("锁定服务状态");
        state.routes.insert(
            path.as_ref().to_string(),
            Route {
                body: body.as_ref().to_vec(),
            },
        );
    }

    /// 记录到的请求目标（含查询串），按到达顺序排列。
    pub fn targets(&self) -> Vec<String> {
        self.state.lock().expect("锁定服务状态").targets.clone()
    }

    /// 某个路径被请求的次数。
    pub fn hits(&self, path: &str) -> usize {
        self.targets()
            .iter()
            .filter(|target| {
                target
                    .split(['?', '#'])
                    .next()
                    .map(|value| value == path)
                    .unwrap_or(false)
            })
            .count()
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
    let mut range = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
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
    let route = {
        let mut state = state.lock().expect("锁定服务状态");
        state.targets.push(target);
        state.routes.get(&path).cloned()
    };

    match route {
        Some(route) => write_route(&mut stream, &route, range.as_deref()),
        None => write_simple(&mut stream, 404, b"not found"),
    }
}

fn write_route(stream: &mut TcpStream, route: &Route, range: Option<&str>) -> std::io::Result<()> {
    let Some((start, end)) = range.and_then(|value| parse_range(value, route.body.len())) else {
        return write_simple(stream, 200, &route.body);
    };
    if start >= route.body.len() {
        let response = format!(
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            route.body.len()
        );
        stream.write_all(response.as_bytes())?;
        return stream.flush();
    }
    let end = end.min(route.body.len() - 1);
    let slice = &route.body[start..=end];
    let response = format!(
        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
        slice.len(),
        start,
        end,
        route.body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.write_all(slice)?;
    stream.flush()
}

/// 解析 `bytes=start-end` 或 `bytes=start-`，返回闭区间端点。
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

fn write_simple(stream: &mut TcpStream, status: u16, body: &[u8]) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        416 => "Range Not Satisfiable",
        500 => "Internal Server Error",
        _ => "Status",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// 把可执行脚本写到临时目录，用于替代外部命令（例如 hpatchz）。
pub fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    fs::write(&path, body).expect("写入脚本");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("设置脚本权限");
    path
}
