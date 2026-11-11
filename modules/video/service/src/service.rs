//! The HTTP side: playlists and segments of the videos in the master's file tree (the subtitles are in `subtitles.rs`, the
//! details and posters the master asks for in `derive.rs`).
//!
//! Only the master talks to this service (it holds the shared token and checks the login first). A request
//! names a file by its place in the tree (`p`, `<user>/<device>/<path>`, or just `<path>` for a master file) and its content (`h`, the hex root hash): a changed
//! file has a new `h`, so it gets new cache entries and never a stale segment. Segments are made when they are
//! asked for, one ffmpeg run each, a few at a time, and kept for a while (see [`sweep`]).

use crate::ffmpeg;
use crate::hls::{self, Source, Variant};
use axum::extract::{Path as UrlPath, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use fafnirx_module_api::{bearer_matches, FILES_PREFIX, HEALTH_PATH, MANIFEST_PATH, UI_PREFIX};
use fafnirx_module_kit::Tasks;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Semaphore;

pub struct Config {
    /// The master's data directory: videos are read from `<data>/files/<path>`.
    pub data: PathBuf,
    pub cache: PathBuf,
    /// What the master sends as `Authorization: Bearer`.
    pub token: String,
    pub ffmpeg: String,
    pub ffprobe: String,
    /// The video encoder: `libx264`, or a hardware one such as `h264_nvenc`.
    pub vcodec: String,
    /// Segments made at the same time.
    pub jobs: usize,
    /// The built script and style of the module's part of the web app (`FAFNIRX_PLUGIN_UI`), if it has been built.
    pub ui: Option<PathBuf>,
}

pub struct App {
    pub(crate) cfg: Config,
    pub(crate) jobs: Semaphore,
    /// Details and frames for the master have slots of their own: segments being cut must not starve the master's jobs.
    pub(crate) derive: Semaphore,
    /// What is being made, and what failed lately (see `GET /status`).
    pub(crate) tasks: Tasks,
    /// One lock per file being made, so that two requests for the same segment make it once.
    locks: Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
    /// What the manifest said last was missing, and when it was looked at (see `derive::missing`).
    pub(crate) setup: tokio::sync::Mutex<Option<(std::time::Instant, Vec<String>)>>,
}

impl App {
    pub fn new(cfg: Config) -> Arc<Self> {
        let jobs = Semaphore::new(cfg.jobs.max(1));
        let derive = Semaphore::new(cfg.jobs.max(1));
        Arc::new(Self { cfg, jobs, derive, tasks: Tasks::default(), locks: Mutex::default(), setup: Default::default() })
    }

    pub(crate) fn lock_for(&self, path: &Path) -> Arc<tokio::sync::Mutex<()>> {
        self.locks.lock().expect("lock table").entry(path.to_path_buf()).or_default().clone()
    }

    pub(crate) fn done_with(&self, path: &Path) {
        self.locks.lock().expect("lock table").remove(path);
    }

    /// Makes the cached file `out` unless it is there: `make` writes it to the path it is given (`out` with the extension
    /// `tmp`), which is moved to `out` once it worked, so nobody reads half a file and a failure leaves nothing behind.
    /// One request makes a file while the others for it wait, and at most `jobs` files are made at once.
    pub(crate) async fn make_once<F, Fut>(&self, video: &Video, kind: &'static str, out: &Path, make: F) -> anyhow::Result<()>
    where
        F: FnOnce(PathBuf) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<()>>,
    {
        if out.is_file() {
            return Ok(());
        }
        let lock = self.lock_for(out);
        let made = {
            let _held = lock.lock().await;
            if out.is_file() {
                Ok(())
            } else {
                let task = self.tasks.begin(kind, &video.name);
                let _permit = self.jobs.acquire().await.expect("the semaphore is never closed");
                task.started();
                let tmp = out.with_extension("tmp");
                let result = match make(tmp.clone()).await {
                    Ok(()) => std::fs::rename(&tmp, out).map_err(Into::into),
                    Err(e) => {
                        let _ = std::fs::remove_file(&tmp);
                        Err(e)
                    }
                };
                task.finish(result.as_ref().err().map(|e| format!("{e:#}")));
                result
            }
        };
        self.done_with(out);
        made
    }
}

pub(crate) struct Failure(StatusCode, String);

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

pub(crate) fn fail(status: StatusCode, message: impl Into<String>) -> Failure {
    Failure(status, message.into())
}

pub(crate) fn internal(context: &str, e: anyhow::Error) -> Failure {
    tracing::error!("{context}: {e:#}");
    fail(StatusCode::INTERNAL_SERVER_ERROR, format!("{context}: {e:#}"))
}

/// The module's part of the web app (`GET /ui/<file>`): what the web build left in `FAFNIRX_PLUGIN_UI`.
async fn ui_file(State(app): State<Arc<App>>, UrlPath(name): UrlPath<String>) -> Result<Response, Failure> {
    fafnirx_module_kit::ui::file(app.cfg.ui.as_deref(), &name)
        .await
        .map_err(|(status, message)| fail(status, message))
}

pub fn router(app: Arc<App>) -> Router {
    // What the web app may ask for, through the master (which has checked the login and the file first).
    let files = Router::new()
        .route("/hls/master.m3u8", get(master))
        .route("/hls/subtitles.json", get(crate::subtitles::list))
        .route("/hls/subtitles/{file}", get(crate::subtitles::track))
        .route("/hls/{variant}/index.m3u8", get(media))
        .route("/hls/{variant}/{segment}", get(segment));
    // What only the master asks for.
    let for_master = Router::new()
        .nest(FILES_PREFIX, files)
        .route(&format!("{UI_PREFIX}/{{file}}"), get(ui_file))
        .route(MANIFEST_PATH, get(crate::derive::manifest))
        .route("/probe", get(crate::derive::probe))
        .route("/poster", get(crate::derive::poster))
        .route("/status", get(status))
        .layer(middleware::from_fn_with_state(app.clone(), guard));
    Router::new().route(HEALTH_PATH, get(|| async { "ok" })).merge(for_master).with_state(app)
}

async fn guard(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let given = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !bearer_matches(given, &app.cfg.token) {
        return fail(StatusCode::UNAUTHORIZED, "the video module needs the master's token").into_response();
    }
    next.run(req).await
}

#[derive(Deserialize)]
pub(crate) struct Which {
    /// The place in the file tree: `<user>/<device>/<path>`, or `<path>` for a master file (they are at the root).
    pub(crate) p: String,
    /// Hex root hash of the content.
    pub(crate) h: String,
}

/// Where a video is kept, and where its playlists and segments are.
pub(crate) struct Video {
    /// Its place in the file tree, as the master named it (`p`).
    pub(crate) name: String,
    pub(crate) source: PathBuf,
    pub(crate) dir: PathBuf,
}

/// The name of a video's folder in the cache.
pub fn cache_key(p: &str, h: &str) -> String {
    hex::encode(&Sha256::digest(format!("{p}\0{h}").as_bytes())[..16])
}

/// A path inside the file tree, with nothing that leaves the tree. A master file is at the root, so one part is enough.
fn valid_path(p: &str) -> bool {
    let parts: Vec<&str> = p.split('/').collect();
    parts.iter().all(|c| !c.is_empty() && *c != "." && *c != ".." && !c.contains(['\\', ':', '\0']))
}

/// The file a request names, once it is known to be a file inside the tree.
pub(crate) fn source_of(app: &App, q: &Which) -> Result<PathBuf, Failure> {
    if q.h.is_empty() || q.h.len() > 64 || !q.h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(fail(StatusCode::BAD_REQUEST, "h is the hex hash of the content"));
    }
    if !valid_path(&q.p) {
        return Err(fail(StatusCode::BAD_REQUEST, "p is a path inside the file tree"));
    }
    let source = app.cfg.data.join("files").join(&q.p);
    if !source.is_file() {
        return Err(fail(StatusCode::NOT_FOUND, "no such file"));
    }
    Ok(source)
}

/// The video a request names, with the cache folder of its playlists and segments.
pub(crate) fn locate(app: &App, q: &Which) -> Result<Video, Failure> {
    let source = source_of(app, q)?;
    let dir = app.cfg.cache.join(cache_key(&q.p, &q.h));
    std::fs::create_dir_all(&dir).map_err(|e| internal("cache", e.into()))?;
    // The folder is used: it is not swept for a while.
    let _ = std::fs::write(dir.join(".used"), b"");
    Ok(Video { name: q.p.clone(), source, dir })
}

fn source_text(s: &Source) -> String {
    format!("{} {} {} {}", s.width, s.height, s.duration, u8::from(s.has_audio))
}

fn parse_source_text(text: &str) -> Option<Source> {
    let mut it = text.split_whitespace();
    let (width, height, duration, audio) = (it.next()?, it.next()?, it.next()?, it.next()?);
    Some(Source {
        width: width.parse().ok()?,
        height: height.parse().ok()?,
        duration: duration.parse().ok()?,
        has_audio: audio == "1",
    })
}

/// What the video is: read once and kept with its segments.
async fn describe(app: &App, video: &Video) -> Result<Source, Failure> {
    let file = video.dir.join("probe.txt");
    let known = |file: &Path| std::fs::read_to_string(file).ok().and_then(|t| parse_source_text(&t));
    if let Some(s) = known(&file) {
        return Ok(s);
    }
    let lock = app.lock_for(&file);
    let _held = lock.lock().await;
    let result = async {
        if let Some(s) = known(&file) {
            return Ok(s);
        }
        let _permit = app.jobs.acquire().await.expect("the semaphore is never closed");
        let source = ffmpeg::probe(&app.cfg.ffprobe, &video.source)
            .await
            .map_err(|e| fail(StatusCode::UNSUPPORTED_MEDIA_TYPE, format!("{e:#}")))?;
        std::fs::write(&file, source_text(&source)).map_err(|e| internal("cache", e.into()))?;
        Ok(source)
    }
    .await;
    app.done_with(&file);
    result
}

pub(crate) fn reply(content_type: &'static str, cache: &'static str, body: impl Into<axum::body::Body>) -> Response {
    let mut resp = Response::new(body.into());
    resp.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    resp.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    resp
}

const PLAYLIST: &str = "application/vnd.apple.mpegurl";

async fn master(State(app): State<Arc<App>>, Query(q): Query<Which>) -> Result<Response, Failure> {
    let video = locate(&app, &q)?;
    let source = describe(&app, &video).await?;
    Ok(reply(PLAYLIST, "private, max-age=60", hls::master_playlist(&hls::variants(&source))))
}

fn pick(variants: Vec<Variant>, name: &str) -> Result<Variant, Failure> {
    variants
        .into_iter()
        .find(|v| v.name == name)
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "no such quality"))
}

async fn media(State(app): State<Arc<App>>, UrlPath(variant): UrlPath<String>, Query(q): Query<Which>) -> Result<Response, Failure> {
    let video = locate(&app, &q)?;
    let source = describe(&app, &video).await?;
    pick(hls::variants(&source), &variant)?;
    Ok(reply(PLAYLIST, "private, max-age=60", hls::media_playlist(source.duration)))
}

async fn segment(State(app): State<Arc<App>>, UrlPath((variant, name)): UrlPath<(String, String)>, Query(q): Query<Which>) -> Result<Response, Failure> {
    let n = hls::parse_segment(&name).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no such segment"))?;
    let video = locate(&app, &q)?;
    let source = describe(&app, &video).await?;
    let v = pick(hls::variants(&source), &variant)?;
    let (start, len) = hls::segment_span(n, source.duration).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no such segment"))?;

    let out = video.dir.join(format!("{}-seg{n}.ts", v.name));
    let cfg = &app.cfg;
    let source_file = &video.source;
    app.make_once(&video, "segment", &out, |tmp| async move {
        let args = ffmpeg::segment_args(
            &ffmpeg::Segment { source: source_file, out: &tmp, start, len, has_audio: source.has_audio, vcodec: &cfg.vcodec },
            &v,
        );
        ffmpeg::run(&cfg.ffmpeg, &args).await
    })
    .await
    .map_err(|e| internal("making the segment", e))?;
    let bytes = std::fs::read(&out).map_err(|e| internal("reading the segment", e.into()))?;
    Ok(reply("video/mp2t", "private, max-age=86400", bytes))
}

/// What the service is making and what failed lately (the master shows it next to its own queue).
async fn status(State(app): State<Arc<App>>) -> Response {
    let body = serde_json::to_vec(&app.tasks.report(app.cfg.jobs.max(1))).unwrap_or_default();
    reply("application/json", "no-store", body)
}

/// Removes the cache folders that have not been used for `keep`.
pub fn sweep(cache: &Path, keep: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(cache) else { return 0 };
    let mut removed = 0;
    for e in entries.flatten() {
        let dir = e.path();
        let used = std::fs::metadata(dir.join(".used")).or_else(|_| std::fs::metadata(&dir)).and_then(|m| m.modified());
        let idle = used.ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > keep);
        if dir.is_dir() && idle && std::fs::remove_dir_all(&dir).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
#[path = "../tests/unit/service.rs"]
mod tests;
