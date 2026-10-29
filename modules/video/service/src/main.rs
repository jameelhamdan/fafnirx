//! fafnirx-video: the video module of fafnirx. Adaptive streaming for the web app, and what the master needs to know about
//! a video (its details, a poster frame).
//!
//! A separate service next to the master: it reads the videos from the master's data directory and serves them as
//! HLS (playlists, and segments in several qualities made with ffmpeg when they are asked for). It has no login of its
//! own. The master puts its own login in front of it and holds the token that this service asks for, so keep
//! this service's port off the network (in Docker Compose it is not published). The contract with the master is
//! `fafnirx-module-api` (docs/modules.md).
//!
//! Configuration (environment):
//!   FAFNIRX_MODULE_TOKEN      shared secret the master sends as a bearer token (required)
//!   FAFNIRX_MODULE_LISTEN     listen address (default 0.0.0.0:8090)
//!   FAFNIRX_MODULE_UI         the built script and style of the web app's part of the module (default: none)
//!   FAFNIRX_MODULE_EXIT_WITH_STDIN   set by the master when it runs this as a plugin: exit when its standard input closes
//!   FAFNIRX_DATA              the master's data directory (default ./data); videos are read from `files/` in it
//!   FAFNIRX_VIDEO_CACHE       where probes and segments are kept (default <FAFNIRX_DATA>/cache/video-module)
//!   FAFNIRX_VIDEO_KEEP_DAYS   days a video's segments are kept after the last request (default 7)
//!   FAFNIRX_VIDEO_JOBS        files worked on at the same time (default 2)
//!   FAFNIRX_VIDEO_VCODEC      the video encoder (default libx264; for example h264_nvenc for an NVIDIA card)
//!   FAFNIRX_FFMPEG, FAFNIRX_FFPROBE   the programs (default: from PATH)

mod derive;
mod ffmpeg;
mod hls;
mod probe;
mod service;
mod subtitles;
mod tasks;
mod ui;

use anyhow::{bail, Context, Result};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn number(name: &str, default: u64) -> Result<u64> {
    match var(name) {
        Some(v) => v.parse().with_context(|| format!("{name} must be a number")),
        None => Ok(default),
    }
}

/// Started by the master as a plugin (`FAFNIRX_MODULE_EXIT_WITH_STDIN`), the module's standard input is a pipe the master holds
/// open; when it closes the master is gone. Never ready otherwise.
async fn master_gone() {
    if var("FAFNIRX_MODULE_EXIT_WITH_STDIN").is_none() {
        return std::future::pending().await;
    }
    let mut sink = Vec::new();
    let _ = tokio::io::AsyncReadExt::read_to_end(&mut tokio::io::stdin(), &mut sink).await;
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    if let Some(arg) = std::env::args().nth(1) {
        bail!("unknown argument {arg:?}: fafnirx-video is configured through environment variables only");
    }
    let Some(token) = var("FAFNIRX_MODULE_TOKEN") else {
        bail!("FAFNIRX_MODULE_TOKEN must be set (the same value the master has)");
    };
    if token.len() < 16 {
        bail!("FAFNIRX_MODULE_TOKEN is too short: use at least 16 characters, for example the output of `openssl rand -hex 24`");
    }
    let data = PathBuf::from(var("FAFNIRX_DATA").unwrap_or_else(|| "data".into()));
    let cache =
        var("FAFNIRX_VIDEO_CACHE").map(PathBuf::from).unwrap_or_else(|| data.join("cache").join("video-module"));
    std::fs::create_dir_all(&cache).with_context(|| format!("create {}", cache.display()))?;
    let keep = Duration::from_secs(number("FAFNIRX_VIDEO_KEEP_DAYS", 7)? * 86400);
    let addr: SocketAddr = var("FAFNIRX_MODULE_LISTEN")
        .unwrap_or_else(|| "0.0.0.0:8090".into())
        .parse()
        .context("invalid FAFNIRX_MODULE_LISTEN")?;

    let app = service::App::new(service::Config {
        data,
        cache: cache.clone(),
        token,
        ffmpeg: var("FAFNIRX_FFMPEG").unwrap_or_else(|| "ffmpeg".into()),
        ffprobe: var("FAFNIRX_FFPROBE").unwrap_or_else(|| "ffprobe".into()),
        vcodec: var("FAFNIRX_VIDEO_VCODEC").unwrap_or_else(|| "libx264".into()),
        jobs: number("FAFNIRX_VIDEO_JOBS", 2)? as usize,
        ui: var("FAFNIRX_MODULE_UI").map(PathBuf::from),
    });

    tokio::task::spawn_blocking({
        let cache = cache.clone();
        move || service::sweep(&cache, keep)
    });
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            let cache = cache.clone();
            if let Ok(n) = tokio::task::spawn_blocking(move || service::sweep(&cache, keep)).await {
                if n > 0 {
                    tracing::info!("removed the segments of {n} videos nobody has watched for a while");
                }
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("listen on {addr}"))?;
    tracing::info!("fafnirx-video listening on {addr}");
    tokio::select! {
        r = axum::serve(listener, service::router(app)) => r.context("server")?,
        _ = fafnirx_common::shutdown_signal() => tracing::info!("shutting down"),
        _ = master_gone() => tracing::info!("the master that started this is gone, shutting down"),
    }
    Ok(())
}
