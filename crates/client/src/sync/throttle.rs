use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Synced upload rate limiter: every sender reserves a slot for its bytes.
pub struct Limiter {
    kib_per_sec: AtomicU64,
    next: Mutex<Instant>,
}

impl Limiter {
    pub fn new(kib_per_sec: u64) -> Self {
        Self { kib_per_sec: AtomicU64::new(kib_per_sec), next: Mutex::new(Instant::now()) }
    }

    pub fn set(&self, kib_per_sec: u64) {
        self.kib_per_sec.store(kib_per_sec, Ordering::Relaxed);
    }

    pub async fn acquire(&self, bytes: usize) {
        let rate = self.kib_per_sec.load(Ordering::Relaxed);
        if rate == 0 {
            return;
        }
        let cost = Duration::from_secs_f64(bytes as f64 / (rate as f64 * 1024.0));
        let start = {
            let mut next = self.next.lock().await;
            let start = (*next).max(Instant::now());
            *next = start + cost;
            start
        };
        tokio::time::sleep_until(start).await;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/sync/throttle.rs"]
mod tests;
