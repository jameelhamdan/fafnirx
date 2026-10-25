//! fafnirx-master: collects files from devices, stores them as plain files, and serves the web app.
//!
//! One process does everything: the device gRPC API, the JSON API of the web app, with live updates and the web
//! app, the file watcher, scheduled jobs, sources, mirrors, and the details and previews of photos and
//! videos. It serves several users (administrators, normal users and read-only users), each with a folder of their
//! own for their devices, and the master files that belong to nobody.
//!
//! Configuration (environment):
//!   FAFNIRX_DATABASE_URL    PostgreSQL connection string, e.g. postgres://user:password@host/database (required)
//!   FAFNIRX_DATA            data directory (default ./data)
//!   FAFNIRX_LISTEN          gRPC listen address for devices (default 0.0.0.0:7443)
//!   FAFNIRX_HTTP            web listen address (default 0.0.0.0:8080)
//!   FAFNIRX_ADMIN_PASSWORD  password of the user "admin", set once (required until the user has one; after that the
//!                           password is changed in the web app and this is not read)
//!   FAFNIRX_RCLONE          rclone binary used by mirrors (default: rclone from PATH)
//!   FAFNIRX_RCLONE_CONFIG   rclone config file for mirrors that name a remote (default <FAFNIRX_DATA>/rclone.conf)
//!   FAFNIRX_TICK_SECS       how often sources and mirrors are checked for being due (default 15)
//!   FAFNIRX_INSTANCE_LEASE_SECS  seconds a device must be silent before another computer may use its token (default 90)
//!   FAFNIRX_STREAM_URL      where the optional streaming service is (off without it); FAFNIRX_STREAM_TOKEN is the secret
//!                           shared with it (at least 16 characters)
//!   FAFNIRX_GDRIVE_API      Google Drive API base URL (default https://www.googleapis.com; for tests)
//!   FAFNIRX_FFMPEG          ffmpeg binary for video posters and playable copies (default: ffmpeg from PATH)
//!   FAFNIRX_FFPROBE         ffprobe binary for video details (default: ffprobe from PATH)
//!   FAFNIRX_TRUSTED_PROXY   addresses or ranges (comma separated) of reverse proxies in front of the master, whose
//!                           X-Forwarded-For is believed about the client's address (default: none)
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
mod repo;
mod web;

use anyhow::{bail, Context, Result};
use fafnirx_common::pb::collector_server::CollectorServer;
use fafnirx_common::MAX_MESSAGE;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

fn address(var: &str, default: &str) -> Result<SocketAddr> {
    std::env::var(var).unwrap_or_else(|_| default.into()).parse().with_context(|| format!("invalid {var}"))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    if let Some(arg) = std::env::args().nth(1) {
        bail!("unknown argument {arg:?}: the master is configured through environment variables only");
    }
    let password = std::env::var("FAFNIRX_ADMIN_PASSWORD").unwrap_or_default();
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

    // The user "admin" gets its password, and every user a folder, before anything else looks at them.
    platform::db::with(&db, move |c| web::auth::bootstrap(c, &password)).await?;
    {
        let store = store.clone();
        platform::db::with(&db, move |c| web::users::ensure_dirs(c, &store)).await?;
    }

    library::meta::run_queue(db.clone(), store.clone(), data_dir.join("cache"));
    monitoring::jobs::spawn(db.clone(), store.clone(), data_dir.clone());
    tokio::spawn(integrations::spawn(db.clone(), store.clone(), data_dir.clone()));

    let state = Arc::new(web::AppState::new(db.clone(), store.clone(), data_dir.clone())?);
    web::listen(state.clone(), url);

    let mut servers = tokio::task::JoinSet::new();
    let addr = address("FAFNIRX_HTTP", "0.0.0.0:8080")?;
    let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("listen on {addr}"))?;
    tracing::info!("web app on http://{addr}");
    let trusted = state.trusted.clone();
    let app = web::router(state).into_make_service_with_connect_info::<SocketAddr>();
    servers.spawn(async move { axum::serve(listener, app).await.context("web server") });

    let svc = devices::grpc::Service { db, store, updates_dir: data_dir.join("updates"), trusted };
    let addr = address("FAFNIRX_LISTEN", "0.0.0.0:7443")?;
    tracing::info!("fafnirx-master listening for devices on {addr}");
    servers.spawn(async move {
        tonic::transport::Server::builder()
            .add_service(
                CollectorServer::new(svc).max_decoding_message_size(MAX_MESSAGE).max_encoding_message_size(MAX_MESSAGE),
            )
            .serve(addr)
            .await
            .context("device server")
    });

    tokio::select! {
        _ = fafnirx_common::shutdown_signal() => tracing::info!("shutting down"),
        Some(done) = servers.join_next() => {
            // A server that stops is a failure: the other cannot do its job without it.
            done.context("server task")??;
        }
    }
    Ok(())
}
