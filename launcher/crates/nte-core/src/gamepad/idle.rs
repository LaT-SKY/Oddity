use crate::game_mode::IdleInhibitor;
use serde::Serialize;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Status {
    pub requested: bool,
    pub error: Option<String>,
}

pub struct ActivityInhibitor {
    sender: mpsc::Sender<Option<Instant>>,
    status: Arc<Mutex<Status>>,
}
impl ActivityInhibitor {
    pub fn new(timeout: Duration) -> Self {
        Self::with_request(timeout, || IdleInhibitor::start("Oddity 手柄操作"))
    }
    pub(crate) fn with_request(
        timeout: Duration,
        request: impl Fn() -> Result<IdleInhibitor, String> + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel::<Option<Instant>>();
        let status = Arc::new(Mutex::new(Status::default()));
        let report = status.clone();
        // 桌面总线请求不占用输入轮询或 Tauri 主线程.
        std::thread::spawn(move || {
            let mut inhibitor = None;
            let mut expires: Option<Instant> = None;
            loop {
                let message = if let Some(deadline) = expires {
                    match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    {
                        Ok(message) => Some(message),
                        Err(mpsc::RecvTimeoutError::Timeout) => Some(None),
                        Err(mpsc::RecvTimeoutError::Disconnected) => None,
                    }
                } else {
                    receiver.recv().ok()
                };
                let Some(message) = message else {
                    break;
                };
                if let Some(activity) = message {
                    if activity.elapsed() >= timeout {
                        continue;
                    }
                    expires = Some(activity + timeout);
                    if inhibitor.is_none() {
                        match request() {
                            Ok(request) => {
                                inhibitor = Some(request);
                                *report.lock().expect("手柄唤醒状态") = Status {
                                    requested: true,
                                    error: None,
                                };
                            }
                            Err(error) => {
                                *report.lock().expect("手柄唤醒状态") = Status {
                                    requested: false,
                                    error: Some(error),
                                };
                            }
                        }
                    }
                } else {
                    inhibitor = None;
                    expires = None;
                    *report.lock().expect("手柄唤醒状态") = Status::default();
                }
            }
        });
        Self { sender, status }
    }
    pub fn activity(&self) {
        let _ = self.sender.send(Some(Instant::now()));
    }
    pub fn clear(&self) {
        let _ = self.sender.send(None);
    }
    pub fn status(&self) -> Status {
        self.status.lock().expect("手柄唤醒状态").clone()
    }
}
