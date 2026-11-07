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
