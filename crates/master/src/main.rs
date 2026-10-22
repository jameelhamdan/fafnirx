//! fafnirx-master: collects files from devices, stores them as plain files, and serves the admin web app.
//!
//! One binary with roles, so that a small install is a single process and a large one can be split:
//!   api       the admin JSON API, live updates, the web app
//!   transfer  the device gRPC API, and downloads, uploads and zip streams of the admin API
//!   worker    the file watcher, scheduled jobs, sources, mirrors, metadata and previews
//! Choose with `--role api,transfer` or FAFNIRX_ROLE; the default is `all`. Split processes share the
//! database and the data directory, and need a reverse proxy that sends `/api/files/download`,
//! `/api/files/thumb`, `/api/files/zip`, `/api/uploads` and `/api/media/*` to the
//! `transfer` process.
//!
//! Configuration (environment):
//!   FAFNIRX_DATABASE_URL    PostgreSQL connection string, e.g. postgres://user:password@host/database (required)
//!   FAFNIRX_DATA            data directory (default ./data)
//!   FAFNIRX_LISTEN          gRPC listen address for devices (default 0.0.0.0:7443)
//!   FAFNIRX_HTTP            admin web listen address (default 0.0.0.0:8080)
//!   FAFNIRX_ADMIN_PASSWORD  password of the admin web app (required with the api or transfer role)
//!   FAFNIRX_RCLONE          rclone binary used by mirrors (default: rclone from PATH)
//!   FAFNIRX_TICK_SECS       how often sources and mirrors are checked for being due (default 15)
//!   FAFNIRX_GDRIVE_API      Google Drive API base URL (default https://www.googleapis.com; for tests)
//!   FAFNIRX_FFMPEG          ffmpeg binary for video posters and playable copies (default: ffmpeg from PATH)
//!   FAFNIRX_FFPROBE         ffprobe binary for video details (default: ffprobe from PATH)
//!   FAFNIRX_MAP_TILES       tile URL of the map, e.g. https://tile.openstreetmap.org/{z}/{x}/{y}.png (the default)
//!
//! There is no TLS in the master: run it on a trusted network or VPN, or put a TLS proxy (a Cloudflare
//! tunnel, a reverse proxy) in front.

#![allow(clippy::result_large_err)] // tonic::Status is large and is the error type of every RPC helper

mod devices;
mod files;
mod integrations;
mod library;
mod monitoring;
mod platform;
mod web;

use anyhow::{bail, Context, Result};
use fafnirx_common::pb::collector_server::CollectorServer;
use fafnirx_common::MAX_MESSAGE;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy)]
struct Roles {
    api: bool,
    transfer: bool,
    worker: bool,
}

impl Roles {
    fn parse(text: &str) -> Result<Self> {
        let mut roles = Roles { api: false, transfer: false, worker: false };
        for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            match part {
                "all" => (roles.api, roles.transfer, roles.worker) = (true, true, true),
                "api" => roles.api = true,
                "transfer" => roles.transfer = true,
                "worker" => roles.worker = true,
                other => bail!("unknown role {other:?}: use all, api, transfer or worker"),
            }
        }
        if !(roles.api || roles.transfer || roles.worker) {
            bail!("no role chosen");
        }
        Ok(roles)
    }

    /// From `--role X` or FAFNIRX_ROLE.
    fn from_environment() -> Result<Self> {
        let mut args = std::env::args().skip(1);
        let mut text = std::env::var("FAFNIRX_ROLE").unwrap_or_else(|_| "all".into());
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--role" => text = args.next().context("--role needs a value")?,
                other => bail!("unknown argument {other:?}; the only option is --role"),
            }
        }
        Self::parse(&text)
    }
}

fn address(var: &str, default: &str) -> Result<SocketAddr> {
    std::env::var(var).unwrap_or_else(|_| default.into()).parse().with_context(|| format!("invalid {var}"))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let roles = Roles::from_environment()?;
    let data_dir = PathBuf::from(std::env::var("FAFNIRX_DATA").unwrap_or_else(|_| "data".into()));
    std::fs::create_dir_all(&data_dir)?;
    let url = std::env::var("FAFNIRX_DATABASE_URL")
        .context("FAFNIRX_DATABASE_URL must be set (postgres://user:password@host/database)")?;
    let db = {
        let url = url.clone();
        tokio::task::spawn_blocking(move || platform::db::open(&url)).await?.context("open database")?
    };
    // Pooled connections own a small runtime each, and dropping one on an async thread panics. Keeping a
    // reference alive for the life of the process means the pool is never dropped (not even on an error exit).
    std::mem::forget(db.clone());
    let store = Arc::new(platform::store::Store::open(&data_dir)?);

    if roles.worker {
        library::meta::run_queue(db.clone(), store.clone(), data_dir.join("cache"));
        let (db, store, data_dir, url) = (db.clone(), store.clone(), data_dir.clone(), url.clone());
        tokio::spawn(async move {
            // Scheduled work must run in one process only.
            if let Err(e) = platform::db::lead(&url).await {
                return tracing::error!("cannot take the leader lock: {e:#}");
            }
            monitoring::jobs::spawn(db.clone(), store.clone(), data_dir.clone());
            integrations::spawn(db, store, data_dir).await;
        });
    }

    let mut servers = tokio::task::JoinSet::new();
    if roles.api || roles.transfer {
        let password = std::env::var("FAFNIRX_ADMIN_PASSWORD").unwrap_or_default();
        if password.is_empty() {
            bail!("FAFNIRX_ADMIN_PASSWORD must be set for the api and transfer roles");
        }
        let state = Arc::new(web::AppState::new(db.clone(), store.clone(), data_dir.clone(), password)?);
        if roles.api {
            web::listen(state.clone(), url.clone());
        }
        let addr = address("FAFNIRX_HTTP", "0.0.0.0:8080")?;
        let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("listen on {addr}"))?;
        tracing::info!("admin web app on http://{addr}");
        let app = web::router(state, roles.api, roles.transfer).into_make_service_with_connect_info::<SocketAddr>();
        servers.spawn(async move { axum::serve(listener, app).await.context("admin web server") });
    }
    if roles.transfer {
        let svc =
            devices::grpc::Service { db: db.clone(), store: store.clone(), updates_dir: data_dir.join("updates") };
        let addr = address("FAFNIRX_LISTEN", "0.0.0.0:7443")?;
        tracing::info!("fafnirx-master listening for devices on {addr}");
        servers.spawn(async move {
            tonic::transport::Server::builder()
                .add_service(
                    CollectorServer::new(svc)
                        .max_decoding_message_size(MAX_MESSAGE)
                        .max_encoding_message_size(MAX_MESSAGE),
                )
                .serve(addr)
                .await
                .context("device server")
        });
    }

    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("shutting down"),
        Some(done) = servers.join_next() => {
            // A server that stops is a failure: the others cannot do their job without it.
            done.context("server task")??;
        }
    }
    Ok(())
}
