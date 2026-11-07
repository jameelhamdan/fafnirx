//! The settings every module reads from its environment, the start and stop of the service, and how the module makes itself known to
//! the master: it announces where it is (`POST <master>/api/plugins/register`, with the shared secret) every 30 s, so that starting
//! the container is all it takes to connect it.
//!
//! - `FAFNIRX_PLUGIN_SECRET`: the secret shared with the master (`FAFNIRX_PLUGIN_SECRET` there too): the master calls the module with
//!   it, and the module joins with it (required, 16+ characters)
//! - `FAFNIRX_MASTER_URL`: where the master's web API is (`http://master:8080`); without it the module does not announce itself
//! - `FAFNIRX_PLUGIN_URL`: where the master reaches this module (`http://video:8090`); required with `FAFNIRX_MASTER_URL`
//! - `FAFNIRX_PLUGIN_LISTEN`: listen address (the module's default)
//! - `FAFNIRX_PLUGIN_UI`: the built script and style of the module's part of the web app (default: none)
//! - `FAFNIRX_DATA`: the master's data directory (default `./data`); files are written to `tmp/` in it
//! - `FAFNIRX_<PREFIX>_JOBS`: files made at the same time (default 2)
//! - `FAFNIRX_<PREFIX>_IDLE_SECS`: work that makes no progress for this long is given up on (default 600, at least 10)

use crate::fetch::{router, Fetcher};
use anyhow::{bail, Context, Result};
use axum::Router;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// How often a module tells the master where it is (the master forgets one that is silent for three times this).
const ANNOUNCE_EVERY: Duration = Duration::from_secs(30);
/// How soon it tries again when the master did not take the announcement (it may still be starting).
const ANNOUNCE_RETRY: Duration = Duration::from_secs(5);

/// An environment variable that is set and not empty.
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// An environment variable that is a number, or `default`.
pub fn number(name: &str, default: u64) -> Result<u64> {
    match var(name) {
        Some(v) => v.parse().with_context(|| format!("{name} must be a number")),
        None => Ok(default),
    }
}

/// The settings every module has.
#[derive(Debug, Clone)]
pub struct Common {
    /// The master's data directory: files are written to `<data>/tmp/…`.
    pub data: PathBuf,
    /// The secret shared with the master: what it sends as `Authorization: Bearer`, and what this module joins with.
    pub token: String,
    /// Files made at the same time, and listings made at the same time.
    pub jobs: usize,
    /// Work that makes no progress for this long is given up on.
    pub idle: Duration,
    /// The built script and style of the module's part of the web app, if it has been built.
    pub ui: Option<PathBuf>,
    pub listen: SocketAddr,
    /// Where to tell the master that this module is; none when `FAFNIRX_MASTER_URL` is not set.
    pub announce: Option<Announce>,
}

/// Where the master is, and where it reaches this module.
#[derive(Debug, Clone)]
pub struct Announce {
    pub master: String,
    pub address: String,
}

/// Starts logging, refuses arguments (a module is configured through the environment only) and reads the settings. `name` is the
/// program's (`fafnirx-fetch`), `prefix` the one of its own variables (`FETCH`), `listen` its default address.
pub fn start(name: &str, prefix: &str, listen: &str) -> Result<Common> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    if let Some(arg) = std::env::args().nth(1) {
        bail!("unknown argument {arg:?}: {name} is configured through environment variables only");
    }
    let Some(token) = var("FAFNIRX_PLUGIN_SECRET") else {
        bail!("FAFNIRX_PLUGIN_SECRET must be set (the same value the master has)");
    };
    if token.len() < 16 {
        bail!("FAFNIRX_PLUGIN_SECRET is too short: use at least 16 characters, for example the output of `openssl rand -hex 24`");
    }
    let announce = match (var("FAFNIRX_MASTER_URL"), var("FAFNIRX_PLUGIN_URL")) {
        (Some(master), Some(address)) => Some(Announce { master, address }),
        (Some(_), None) => bail!("FAFNIRX_PLUGIN_URL must be set: where the master reaches this module, for example http://{name}:8090"),
        (None, _) => {
            tracing::warn!("FAFNIRX_MASTER_URL is not set: this module does not announce itself, so the master will not find it");
            None
        }
    };
    Ok(Common {
        data: PathBuf::from(var("FAFNIRX_DATA").unwrap_or_else(|| "data".into())),
        token,
        jobs: number(&format!("FAFNIRX_{prefix}_JOBS"), 2)? as usize,
        idle: Duration::from_secs(number(&format!("FAFNIRX_{prefix}_IDLE_SECS"), 600)?.max(10)),
        ui: var("FAFNIRX_PLUGIN_UI").map(PathBuf::from),
        listen: var("FAFNIRX_PLUGIN_LISTEN")
            .unwrap_or_else(|| listen.into())
            .parse()
            .context("invalid FAFNIRX_PLUGIN_LISTEN")?,
        announce,
    })
}

/// Serves a module that reads sources until it is told to stop.
pub async fn serve<M: Fetcher>(common: Common, module: M) -> Result<()> {
    let name = module.info().id;
    let app = router(&common, module);
    run(name, &common, app).await
}

/// Serves `app` until it is told to stop, and keeps telling the master where it is. `name` is the module's id. For a module that has
/// a service of its own.
pub async fn run(name: &str, common: &Common, app: Router) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(common.listen)
        .await
        .with_context(|| format!("listen on {}", common.listen))?;
    tracing::info!("fafnirx-{name} listening on {}", common.listen);
    if let Some(to) = &common.announce {
        tokio::spawn(announce_forever(name.to_string(), common.token.clone(), to.clone()));
    }
    tokio::select! {
        r = axum::serve(listener, app) => r.context("server")?,
        _ = fafnirx_common::shutdown_signal() => tracing::info!("shutting down"),
    }
    Ok(())
}

/// One announcement: what the master answered when it was not taken.
async fn announce_once(client: &reqwest::Client, name: &str, secret: &str, to: &Announce) -> Result<(), String> {
    let url = format!("{}/api/plugins/register", to.master.trim_end_matches('/'));
    let reply = client
        .post(&url)
        .bearer_auth(secret)
        .json(&serde_json::json!({ "id": name, "address": to.address }))
        .send()
        .await
        .map_err(|e| format!("{url} cannot be reached: {e:#}"))?;
    if reply.status().is_success() {
        return Ok(());
    }
    let status = reply.status();
    let said = reply.text().await.unwrap_or_default();
    Err(format!("{url} answered {status}: {}", said.chars().take(200).collect::<String>()))
}

/// Tells the master where this module is, now and every [`ANNOUNCE_EVERY`] (every [`ANNOUNCE_RETRY`] while it is refused). The log says when it starts working and when it stops
/// (once each, not every half minute).
async fn announce_forever(name: String, secret: String, to: Announce) {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("a client");
    let mut working: Option<bool> = None;
    loop {
        let result = announce_once(&client, &name, &secret, &to).await;
        match (&result, working) {
            (Ok(()), Some(true)) | (Err(_), Some(false)) => {}
            (Ok(()), _) => tracing::info!("announced to the master at {}", to.master),
            (Err(e), _) => tracing::warn!("the master does not know this module yet: {e}"),
        }
        working = Some(result.is_ok());
        tokio::time::sleep(if result.is_ok() { ANNOUNCE_EVERY } else { ANNOUNCE_RETRY }).await;
    }
}

#[cfg(test)]
#[path = "../tests/unit/env.rs"]
mod tests;
