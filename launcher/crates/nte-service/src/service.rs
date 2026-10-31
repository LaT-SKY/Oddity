//! Electron 的私有 stdio 服务. 输入设备始终由进程主线程持有.
use crate::{AppState, GameTracker, Settings};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};

pub type ServiceHandle = Arc<Service>;
type InputTask = Box<dyn FnOnce() + Send>;
#[cfg(test)]
type Listener = (String, Box<dyn Fn(Value) + Send + 'static>);

pub struct Service {
    pub state: AppState,
    pub downloads: crate::download_commands::DownloadManager,
    pub playtime: crate::playtime_commands::Playtime,
    pub snapshots: crate::snapshot_commands::Snapshots,
    pub exports: crate::log_commands::LogExports,
    pub focused: AtomicBool,
    pub visible: AtomicBool,
    pub stopping: AtomicBool,
    output: mpsc::Sender<Value>,
    input: mpsc::Sender<InputTask>,
    #[cfg(test)]
    listeners: Mutex<Vec<Listener>>,
    #[cfg(test)]
    _directory: Option<crate::tests::TempDir>,
}
impl Service {
    fn new(
        settings: Settings,
        output: mpsc::Sender<Value>,
        input: mpsc::Sender<InputTask>,
        data: &std::path::Path,
    ) -> Self {
        Self {
            state: AppState {
                settings: Mutex::new(settings),
                game: Mutex::new(GameTracker::default()),
            },
            downloads: Default::default(),
            playtime: crate::playtime_commands::Playtime::load(data),
            snapshots: crate::snapshot_commands::Snapshots::load(data),
            exports: Default::default(),
            focused: AtomicBool::new(false),
            visible: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            output,
            input,
            #[cfg(test)]
            listeners: Mutex::new(Vec::new()),
            #[cfg(test)]
            _directory: None,
        }
    }
    pub fn emit(&self, event: &str, payload: impl Serialize) -> Result<(), String> {
        let payload = serde_json::to_value(payload).map_err(|e| e.to_string())?;
        #[cfg(test)]
        for (name, callback) in self.listeners.lock().unwrap().iter() {
            if name == event {
                callback(payload.clone());
            }
        }
        self.output
            .send(json!({"event":event,"payload":payload}))
            .map_err(|e| e.to_string())
    }
    pub fn input_task(&self, task: impl FnOnce() + Send + 'static) -> Result<(), String> {
        self.input.send(Box::new(task)).map_err(|e| e.to_string())
    }
    #[cfg(test)]
    pub fn subscribe(&self, event: &str, callback: impl Fn(Value) + Send + 'static) {
        self.listeners
            .lock()
            .unwrap()
            .push((event.into(), Box::new(callback)));
    }
}
fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("启动后台执行器失败")
    })
}
pub fn spawn_blocking<F, T>(job: F) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    runtime().spawn_blocking(job)
}
pub fn spawn<F>(job: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    runtime().spawn(job)
}
#[cfg(test)]
pub fn block_on<F: std::future::Future>(job: F) -> F::Output {
    runtime().block_on(job)
}
#[cfg(test)]
pub fn test_service() -> ServiceHandle {
    let (output, _) = mpsc::channel();
    let (input, _) = mpsc::channel();
    let directory = crate::tests::TempDir::new("service");
    let mut service = Service::new(Settings::default(), output, input, directory.path());
    service._directory = Some(directory);
    Arc::new(service)
}

pub fn run() {
    if crate::attendance_commands::headless_entry() {
        return;
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if crate::cli::help_requested(&args) {
        println!("{}", crate::cli::USAGE);
        return;
    }
    if !args.iter().any(|arg| arg == "--ipc") {
        eprintln!("请通过 oddity 启动图形界面.");
        std::process::exit(2);
    }
    crate::migrate_legacy_dirs();
    let paths = nte_core::settings::UserPaths::discover();
    let _instance = nte_core::recovery::LauncherLock::launcher(&paths).unwrap_or_else(|error| {
        eprintln!("{error:#}");
        std::process::exit(1)
    });
    let mut settings = crate::load_settings();
    for environment in &settings.runtime_environments {
        if let Err(error) = nte_core::environment::recover_pending(&crate::data_dir(), environment)
        {
            eprintln!("运行环境恢复失败: {error:#}");
        }
    }
    let compatible = args.iter().any(|arg| arg == "--recovery-compatible");
    if compatible {
        settings.hero_media = "photo".into();
        settings.reduce_motion = true;
        settings.minimize_to_tray_on_close = false;
    }
    let (output, events) = mpsc::channel();
    std::thread::spawn(move || {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        for message in events {
            if serde_json::to_writer(&mut out, &message).is_err()
                || out.write_all(b"\n").is_err()
                || out.flush().is_err()
            {
                break;
            }
        }
    });
    let (input, jobs) = mpsc::channel();
    let app = Arc::new(Service::new(settings, output, input, &crate::data_dir()));
    if !compatible
        && app.state.settings.lock().unwrap().distribution
            == nte_core::distribution::Distribution::Cn
    {
        crate::attendance_commands::on_launch();
    }
    crate::update_scheduler::start(app.clone());
    let reader = app.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            let request = serde_json::from_str::<crate::protocol::Request>(&line);
            let app = reader.clone();
            match request {
                Ok(request) => {
                    if request.command == "host_state" {
                        let result = runtime().block_on(crate::protocol::dispatch(
                            app.clone(),
                            request.command,
                            request.args,
                        ));
                        let reply = match result {
                            Ok(value) => json!({"id":request.id,"result":value}),
                            Err(error) => json!({"id":request.id,"error":error}),
                        };
                        let _ = app.output.send(reply);
                        continue;
                    }
                    spawn(async move {
                        let result =
                            crate::protocol::dispatch(app.clone(), request.command, request.args)
                                .await;
                        let reply = match result {
                            Ok(value) => json!({"id":request.id,"result":value}),
                            Err(error) => json!({"id":request.id,"error":error}),
                        };
                        let _ = app.output.send(reply);
                    });
                }
                Err(error) => {
                    eprintln!("无效宿主请求: {error}");
                    break;
                }
            }
        }
        reader.stopping.store(true, Ordering::Release);
        let _ = reader.input_task(|| {});
    });
    while !app.stopping.load(Ordering::Acquire) {
        match jobs.recv_timeout(std::time::Duration::from_millis(20)) {
            Ok(job) => job(),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
    }
    crate::gamepad_commands::stop();
    crate::download_commands::cancel_task(&app);
    if let Ok(game) = app.state.game.lock() {
        if let Some(cancel) = &game.launch_cancel {
            cancel.store(true, Ordering::Release);
        }
    };
}
