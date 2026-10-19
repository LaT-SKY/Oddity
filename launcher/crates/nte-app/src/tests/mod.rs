//! nte-app 单元测试的公共设施。
//!
//! 这里的路径探测、Proton 检测都依赖进程环境变量，测试之间必须串行，
//! 所以所有会碰环境变量的测试都要先拿到 `EnvGuard`。

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

mod bootstrap;
mod env;
mod launch;
mod paths;
mod presence;
mod process;
mod proton;
mod settings;

/// 独占进程环境变量并在测试结束时恢复。
pub struct EnvGuard {
    saved: Vec<(&'static str, Option<OsString>)>,
    _lock: MutexGuard<'static, ()>,
}

static ENV_LOCK: Mutex<()> = Mutex::new(());

impl EnvGuard {
    pub fn new() -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        Self {
            saved: Vec::new(),
            _lock: lock,
        }
    }

    pub fn set(&mut self, key: &'static str, value: impl AsRef<OsStr>) {
        self.remember(key);
        std::env::set_var(key, value);
    }

    pub fn remove(&mut self, key: &'static str) {
        self.remember(key);
        std::env::remove_var(key);
    }

    fn remember(&mut self, key: &'static str) {
        if !self.saved.iter().any(|(saved, _)| *saved == key) {
            self.saved.push((key, std::env::var_os(key)));
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
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
            std::env::temp_dir().join(format!("nte-app-{tag}-{}-{unique}", std::process::id()));
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

    pub fn write_text(&self, rel: impl AsRef<Path>, contents: &str) -> PathBuf {
        self.write(rel, contents.as_bytes())
    }

    pub fn read(&self, rel: impl AsRef<Path>) -> Vec<u8> {
        fs::read(self.path.join(rel)).expect("读取文件")
    }

    pub fn read_text(&self, rel: impl AsRef<Path>) -> String {
        fs::read_to_string(self.path.join(rel)).expect("读取文本文件")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // 权限被测试改过时先放开，保证临时目录能被清掉
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&self.path);
    }
}

use std::os::unix::fs::PermissionsExt;

/// 写一个可执行脚本，用来替代外部命令。
pub fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    fs::create_dir_all(dir).expect("创建脚本目录");
    let path = dir.join(name);
    fs::write(&path, body).expect("写入脚本");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("设置脚本权限");
    path
}

/// 构造一个可用的 Proton 目录（包含 proton 与 toolmanifest.vdf）。
pub fn fake_proton(root: &Path, name: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).expect("创建 Proton 目录");
    fs::write(dir.join("proton"), b"").expect("写入 proton");
    fs::write(dir.join("toolmanifest.vdf"), b"").expect("写入 toolmanifest");
    dir
}

#[derive(Clone)]
struct Route {
    status: u16,
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
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定端口");
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

    pub fn route(&self, path: impl AsRef<str>, body: impl AsRef<[u8]>) {
        let mut state = self.state.lock().expect("锁定服务状态");
        state.routes.insert(
            path.as_ref().to_string(),
            Route {
                status: 200,
                body: body.as_ref().to_vec(),
            },
        );
    }

    pub fn targets(&self) -> Vec<String> {
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
        Some(route) => (route.status, route.body),
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
