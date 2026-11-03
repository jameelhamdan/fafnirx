//! The HTTP client of a module, and reading a bounded body.

use anyhow::{ensure, Context, Result};
use futures_util::StreamExt;
use std::time::Duration;

/// A client that says who it is (`agent`, for example `fafnirx-fetch/1.0`). It has no limit on the time of a request, since it may
/// download files; [`read_limited`] has one.
pub fn client(agent: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(agent)
        .connect_timeout(Duration::from_secs(20))
        .build()
        .expect("an HTTP client")
}

/// What is at a web address, at most `limit` bytes of it, in at most `timeout`.
pub async fn read_limited(http: &reqwest::Client, url: &str, limit: usize, timeout: Duration) -> Result<Vec<u8>> {
    let reply = http
        .get(url)
        .timeout(timeout)
        .send()
        .await
        .with_context(|| format!("reach {url}"))?
        .error_for_status()
        .with_context(|| format!("fetch {url}"))?;
    let mut body = Vec::new();
    let mut stream = reply.bytes_stream();
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk?);
        ensure!(body.len() <= limit, "{url} is larger than {} MB", limit >> 20);
    }
    Ok(body)
}
