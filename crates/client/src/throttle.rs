use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Shared upload rate limiter: every sender reserves a slot for its bytes.
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
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn paces_to_the_configured_rate() {
        let l = Limiter::new(100); // 100 KiB/s
        let t0 = Instant::now();
        for _ in 0..10 {
            l.acquire(10 * 1024).await; // 100 KiB in total
        }
        // The last reservation starts after the previous nine have been paid for.
        assert!(t0.elapsed() >= Duration::from_millis(850), "{:?}", t0.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn zero_means_unlimited() {
        let l = Limiter::new(0);
        let t0 = Instant::now();
        l.acquire(1 << 30).await;
        assert!(t0.elapsed() < Duration::from_millis(1));
    }
}
