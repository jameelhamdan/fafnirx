//! fafnirx-video: the video module of fafnirx. Adaptive streaming for the web app, and what the master needs to know about
//! a video (its details, a poster frame).
//!
//! A separate service next to the master: it reads the videos from the master's data directory and serves them as
//! HLS (playlists, and segments in several qualities made with ffmpeg when they are asked for). It has no login of its
//! own. The master puts its own login in front of it and holds the token that this service asks for, so keep
//! this service's port off the network (in Docker Compose it is not published). The contract with the master is
//! `fafnirx-module-api` (docs/modules.md).
//!
//! Configuration (environment); the token, the address, `FAFNIRX_PLUGIN_UI` and `FAFNIRX_VIDEO_JOBS` are the module kit's
//! (`fafnirx-module-kit`):
//!   FAFNIRX_DATA              the master's data directory (default ./data); videos are read from `files/` in it
//!   FAFNIRX_VIDEO_CACHE       where probes and segments are kept (default <FAFNIRX_DATA>/cache/video-module)
//!   FAFNIRX_VIDEO_KEEP_DAYS   days a video's segments are kept after the last request (default 7)
//!   FAFNIRX_VIDEO_VCODEC      the video encoder (default libx264; for example h264_nvenc for an NVIDIA card)
//!   FAFNIRX_FFMPEG, FAFNIRX_FFPROBE   the programs (default: from PATH)

mod derive;
mod ffmpeg;
mod hls;
mod probe;
mod service;
mod subtitles;

use anyhow::{Context, Result};
use fafnirx_module_kit::{number, start, var};
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let common = start("fafnirx-video", "VIDEO", "0.0.0.0:8090")?;
    let data = common.data.clone();
    let cache = var("FAFNIRX_VIDEO_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| data.join("cache").join("video-module"));
    std::fs::create_dir_all(&cache).with_context(|| format!("create {}", cache.display()))?;
    let keep = Duration::from_secs(number("FAFNIRX_VIDEO_KEEP_DAYS", 7)? * 86400);

    let app = service::App::new(service::Config {
        data,
        cache: cache.clone(),
        token: common.token.clone(),
        ffmpeg: var("FAFNIRX_FFMPEG").unwrap_or_else(|| "ffmpeg".into()),
        ffprobe: var("FAFNIRX_FFPROBE").unwrap_or_else(|| "ffprobe".into()),
        vcodec: var("FAFNIRX_VIDEO_VCODEC").unwrap_or_else(|| "libx264".into()),
        jobs: common.jobs,
        ui: common.ui.clone(),
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

    fafnirx_module_kit::run("video", &common, service::router(app)).await
}
