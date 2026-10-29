//! 下载限速.
//!
//! 同一个任务内的所有连接共享一个令牌桶, 限速值才等于实际总带宽.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 令牌桶限速器; limit 为 0 时不限制.
#[derive(Debug)]
pub struct RateLimiter {
    limit: u64,
    capacity: u64,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    pub fn new(limit: u64) -> Self {
        // 允许约四分之一秒的突发, 避免固定大小的读缓冲把速率切成台阶.
        let capacity = (limit / 4).max(1024 * 1024);
        Self::with_capacity(limit, capacity)
    }

    fn with_capacity(limit: u64, capacity: u64) -> Self {
        Self {
            limit,
            capacity,
            state: Mutex::new(State {
                tokens: capacity as f64,
                last: Instant::now(),
            }),
        }
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// 为已传输的字节扣减令牌, 不足时等待.
    ///
    /// 返回 false 表示等待期间收到取消信号.
    pub fn acquire<F: Fn() -> bool + ?Sized>(&self, bytes: u64, cancelled: &F) -> bool {
        if self.limit == 0 {
            return !cancelled();
        }
        let mut remaining = bytes as f64;
        while remaining > 0.0 {
            if cancelled() {
                return false;
            }
            let wait = {
                let mut state = match self.state.lock() {
                    Ok(state) => state,
                    Err(error) => error.into_inner(),
                };
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(state.last).as_secs_f64();
                state.last = now;
                state.tokens =
                    (state.tokens + elapsed * self.limit as f64).min(self.capacity as f64);
                let taken = state.tokens.min(remaining);
                state.tokens -= taken;
                remaining -= taken;
                if remaining <= 0.0 {
                    return true;
                }
                // 按 200ms 切片等待, 让暂停操作及时生效.
                (remaining / self.limit as f64).min(0.2)
            };
            std::thread::sleep(Duration::from_secs_f64(wait.max(0.001)));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn unlimited_transfers_do_not_wait() {
        let limiter = RateLimiter::new(0);
        let started = Instant::now();
        assert!(limiter.acquire(1024 * 1024 * 1024, &|| false));
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(!limiter.acquire(1, &|| true));
    }

    #[test]
    fn limited_transfers_consume_the_bucket_and_wait_for_refill() {
        // 64 KiB/s, 桶容量 8 KiB: 第二次传输必须等待约 125ms.
        let limiter = RateLimiter::with_capacity(64 * 1024, 8 * 1024);
        assert!(limiter.acquire(8 * 1024, &|| false));
        let started = Instant::now();
        assert!(limiter.acquire(8 * 1024, &|| false));
        assert!(
            started.elapsed() >= Duration::from_millis(90),
            "elapsed {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn cancellation_interrupts_waiting() {
        let limiter = RateLimiter::with_capacity(1024, 1024);
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancelled);
        let waiter = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            flag.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        assert!(!limiter.acquire(1024 * 1024, &|| cancelled.load(Ordering::Relaxed)));
        assert!(started.elapsed() < Duration::from_secs(2));
        waiter.join().unwrap();
    }
}
