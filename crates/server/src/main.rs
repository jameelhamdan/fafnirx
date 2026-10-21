//! fafnirx-server: the Rust data path of the orchestrator.
//!
//! Configuration (environment):
//!   FAFNIRX_DATABASE_URL PostgreSQL connection string, e.g. postgres://user:password@host/database (required)
//!   FAFNIRX_DATA       data directory (default ./data)
//!   FAFNIRX_LISTEN     gRPC listen address (default 0.0.0.0:7443)
//!   FAFNIRX_TLS_HOSTS  comma separated DNS names / IPs for the server certificate
//!                      (default localhost,127.0.0.1)
//!   FAFNIRX_RCLONE     rclone binary used by mirrors (default: rclone from PATH)
//!   FAFNIRX_TICK_SECS  how often sources and mirrors are checked for being due (default 15)
//!   FAFNIRX_GDRIVE_API Google Drive API base URL (default https://www.googleapis.com; for tests)

#![allow(clippy::result_large_err)] // tonic::Status is large and is the error type of every RPC helper

mod db;
mod gdrive;
mod ingest;
mod jobs;
mod mirror;
mod service;
mod sources;
mod store;
mod tls;

use anyhow::{Context, Result};
use fafnirx_common::pb::collector_server::CollectorServer;
use fafnirx_common::MAX_MESSAGE;
use std::path::PathBuf;
use std::sync::Arc;
use tonic::transport::{Identity, Server, ServerTlsConfig};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let data_dir = PathBuf::from(std::env::var("FAFNIRX_DATA").unwrap_or_else(|_| "data".into()));
    let listen = std::env::var("FAFNIRX_LISTEN").unwrap_or_else(|_| "0.0.0.0:7443".into());
    let hosts: Vec<String> = std::env::var("FAFNIRX_TLS_HOSTS")
        .unwrap_or_else(|_| "localhost,127.0.0.1".into())
        .split(',')
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .collect();

    std::fs::create_dir_all(&data_dir)?;
    let url = std::env::var("FAFNIRX_DATABASE_URL")
        .context("FAFNIRX_DATABASE_URL must be set (postgres://user:password@host/database)")?;
    let db = tokio::task::spawn_blocking(move || db::open(&url)).await?.context("open database")?;
    // Pooled connections own a small runtime each, and dropping one on an async thread panics. Keeping a
    // reference alive for the life of the process means the pool is never dropped (not even on an error exit).
    std::mem::forget(db.clone());
    let store = Arc::new(store::Store::open(&data_dir)?);
    let ident = tls::ensure(&data_dir.join("certs"), &hosts).context("prepare TLS")?;
    jobs::spawn(db.clone(), store.clone(), data_dir.clone());
    sources::spawn(db.clone(), store.clone(), data_dir.clone()).await;

    let svc = service::Service { db, store, updates_dir: data_dir.join("updates") };
    let addr = listen.parse().context("invalid FAFNIRX_LISTEN")?;
    tracing::info!("fafnirx-server listening on {addr} (TLS, CA at {})", data_dir.join("certs/ca.pem").display());

    Server::builder()
        .tls_config(ServerTlsConfig::new().identity(Identity::from_pem(ident.cert_pem, ident.key_pem)))?
        .add_service(
            CollectorServer::new(svc).max_decoding_message_size(MAX_MESSAGE).max_encoding_message_size(MAX_MESSAGE),
        )
        .serve_with_shutdown(addr, async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
        })
        .await?;
    Ok(())
}
