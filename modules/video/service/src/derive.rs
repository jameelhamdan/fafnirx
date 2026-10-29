//! What the master asks for besides streaming: what a video is (length, size, codec, date, place) and a picture of one frame.
//! The master makes these once per file, as jobs of its own queue, and keeps the answers.

use crate::service::{fail, internal, source_of, App, Failure, Which};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use fafnirx_module_api::{capability, Manifest, Probe, CONTRACT};
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) async fn manifest(State(app): State<Arc<App>>) -> Json<Manifest> {
    Json(Manifest {
        id: "video".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        contract: CONTRACT,
        capabilities: [capability::PROBE_VIDEO, capability::POSTER, capability::STATUS]
            .into_iter()
            .map(String::from)
            .collect(),
        ui: crate::ui::describe(app.cfg.ui.as_deref()),
        setup: missing(&app).await,
    })
}

/// How long a look at what is missing is trusted: the master asks for the manifest every 2 s while a plugin starts.
const SETUP_TTL: Duration = Duration::from_secs(20);
/// What a program gets to answer `-version`.
const PROGRAM_TIMEOUT: Duration = Duration::from_secs(3);

/// What the module needs and does not have: ffmpeg and ffprobe that start, and a cache folder it can write to.
async fn missing(app: &App) -> Vec<String> {
    let mut seen = app.setup.lock().await;
    if let Some((_, out)) = seen.as_ref().filter(|(at, _)| at.elapsed() < SETUP_TTL) {
        return out.clone();
    }
    let out = look(app).await;
    *seen = Some((Instant::now(), out.clone()));
    out
}

async fn look(app: &App) -> Vec<String> {
    let mut out = Vec::new();
    for program in [&app.cfg.ffmpeg, &app.cfg.ffprobe] {
        let run = tokio::process::Command::new(program).arg("-version").kill_on_drop(true).output();
        let started = tokio::time::timeout(PROGRAM_TIMEOUT, run).await;
        if !matches!(started, Ok(Ok(o)) if o.status.success()) {
            out.push(format!(
                "{program} cannot be run: install ffmpeg, or point FAFNIRX_FFMPEG and FAFNIRX_FFPROBE at it"
            ));
        }
    }
    let probe = app.cfg.cache.join(".writable");
    if tokio::fs::write(&probe, b"").await.is_err() {
        out.push(format!("{} is not writable by this service (FAFNIRX_VIDEO_CACHE)", app.cfg.cache.display()));
    } else {
        let _ = tokio::fs::remove_file(&probe).await;
    }
    out
}

pub(crate) async fn probe(State(app): State<Arc<App>>, Query(q): Query<Which>) -> Result<Json<Probe>, Failure> {
    let source = source_of(&app, &q)?;
    let _permit = app.derive.acquire().await.expect("the semaphore is never closed");
    let found = crate::ffmpeg::details(&app.cfg.ffprobe, &source)
        .await
        .map_err(|e| fail(StatusCode::UNSUPPORTED_MEDIA_TYPE, format!("{e:#}")))?;
    Ok(Json(describe(&found)))
}

/// What a probe says, in the terms of the contract.
fn describe(p: &crate::probe::Probe) -> Probe {
    let (codec, width, height) = p.video.as_ref().map_or((None, None, None), |v| (v.codec.clone(), v.width, v.height));
    Probe {
        duration_ms: p.duration.map(|d| (d * 1000.0).round() as i64),
        width: width.map(i64::from),
        height: height.map(i64::from),
        codec,
        taken_at: p
            .tag(&["creation_time", "com.apple.quicktime.creationdate"])
            .and_then(fafnirx_common::meta::parse_time)
            .filter(|t| *t > 0),
        position: p.tag(&["com.apple.quicktime.location.ISO6709", "location"]).and_then(parse_iso6709),
    }
}

/// "+52.5200+013.4050+034.000/" to (latitude, longitude).
fn parse_iso6709(text: &str) -> Option<(f64, f64)> {
    let text = text.trim().trim_end_matches('/');
    let mut parts: Vec<String> = Vec::new();
    for c in text.chars() {
        if c == '+' || c == '-' || parts.is_empty() {
            parts.push(String::new());
        }
        parts.last_mut()?.push(c);
    }
    let lat: f64 = parts.first()?.parse().ok()?;
    let lon: f64 = parts.get(1)?.parse().ok()?;
    fafnirx_common::meta::valid_position(lat, lon)
}

#[derive(Deserialize)]
pub(crate) struct PosterQuery {
    #[serde(flatten)]
    which: Which,
    /// The longest side, in pixels.
    max: u32,
}

/// The sizes a poster may have.
const POSTER_SIZE: std::ops::RangeInclusive<u32> = 16..=4096;

pub(crate) async fn poster(State(app): State<Arc<App>>, Query(q): Query<PosterQuery>) -> Result<Response, Failure> {
    if !POSTER_SIZE.contains(&q.max) {
        return Err(fail(StatusCode::BAD_REQUEST, "max is a size in pixels"));
    }
    let source = source_of(&app, &q.which)?;
    let _permit = app.derive.acquire().await.expect("the semaphore is never closed");
    let dir = app.cfg.cache.join("posters");
    std::fs::create_dir_all(&dir).map_err(|e| internal("cache", e.into()))?;
    let out = dir.join(format!("{}-{}.jpg", std::process::id(), NEXT_POSTER.fetch_add(1, Ordering::Relaxed)));
    // A second in is usually past the black first frame; a clip shorter than that gets its first frame.
    let mut made = false;
    for seek in ["1", "0"] {
        let _ = std::fs::remove_file(&out);
        let args = crate::ffmpeg::poster_args(&source, &out, q.max, seek);
        if crate::ffmpeg::run(&app.cfg.ffmpeg, &args).await.is_ok()
            && std::fs::metadata(&out).is_ok_and(|m| m.len() > 0)
        {
            made = true;
            break;
        }
    }
    let bytes = if made { std::fs::read(&out) } else { Err(std::io::ErrorKind::NotFound.into()) };
    let _ = std::fs::remove_file(&out);
    let bytes = bytes.map_err(|_| fail(StatusCode::UNPROCESSABLE_ENTITY, "no frame could be taken from the file"))?;
    Ok(crate::service::reply("image/jpeg", "no-store", bytes))
}

/// Numbers the posters being made, so that two never share a file.
static NEXT_POSTER: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_result_is_read() {
        let json = br#"{
            "streams": [
                {"codec_type": "audio", "codec_name": "aac"},
                {"codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
                 "side_data_list": [{"rotation": -90}]}
            ],
            "format": {"duration": "12.5", "tags": {
                "creation_time": "2023-11-14T22:13:20.000000Z",
                "com.apple.quicktime.location.ISO6709": "+52.5200+013.4050+034.000/"}}
        }"#;
        let p = describe(&crate::probe::parse(json).unwrap());
        assert_eq!((p.width, p.height), (Some(1080), Some(1920)), "a sideways video is shown upright");
        assert_eq!(p.duration_ms, Some(12_500));
        assert_eq!(p.codec.as_deref(), Some("h264"));
        assert_eq!(p.taken_at, Some(1_700_000_000));
        assert_eq!(p.position, Some((52.52, 13.405)));
    }

    #[test]
    fn a_file_without_tags_has_only_what_it_says() {
        let json =
            br#"{"streams": [{"codec_type": "video", "codec_name": "vp9", "width": 4, "height": 2}], "format": {}}"#;
        let p = describe(&crate::probe::parse(json).unwrap());
        assert_eq!((p.width, p.height, p.duration_ms, p.taken_at, p.position), (Some(4), Some(2), None, None, None));
    }

    #[test]
    fn positions_in_the_southern_and_western_hemispheres_are_negative() {
        assert_eq!(parse_iso6709("-33.8688+151.2093/"), Some((-33.8688, 151.2093)));
        assert_eq!(parse_iso6709("+40.7128-074.0060+10.0/"), Some((40.7128, -74.006)));
        assert_eq!(parse_iso6709("+00.0000+000.0000/"), None, "0,0 is what broken cameras write");
        assert_eq!(parse_iso6709("nonsense"), None);
    }

    #[test]
    fn a_poster_is_one_frame_scaled_down() {
        let args =
            crate::ffmpeg::poster_args(std::path::Path::new("in.mp4"), std::path::Path::new("out.jpg"), 320, "1");
        assert!(args.windows(2).any(|w| w == ["-ss", "1"]) && args.windows(2).any(|w| w == ["-frames:v", "1"]));
        assert!(args.iter().any(|a| a.starts_with("scale=min(320")));
    }
}
