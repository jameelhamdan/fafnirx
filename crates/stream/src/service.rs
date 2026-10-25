//! The HTTP side: playlists and segments of the videos in the master's file tree (the subtitles are in `subtitles.rs`).
//!
//! Only the master talks to this service (it holds the shared token and checks the login first). A request
//! names a file by its place in the tree (`p`, `<user>/<device>/<path>`, or just `<path>` for a master file) and its content (`h`, the hex root hash): a changed
//! file has a new `h`, so it gets new cache entries and never a stale segment. Segments are made when they are
//! asked for, one ffmpeg run each, a few at a time, and kept for a while (see [`sweep`]).

use crate::ffmpeg;
use crate::hls::{self, Source, Variant};
use crate::tasks::Tasks;
use axum::extract::{Path as UrlPath, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Semaphore;

pub struct Config {
    /// The master's data directory: videos are read from `<data>/files/<device>/<path>`.
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
}

pub struct App {
    pub(crate) cfg: Config,
    pub(crate) jobs: Semaphore,
    /// What is being made, and what failed lately (see `GET /tasks`).
    pub(crate) tasks: Tasks,
    /// One lock per file being made, so that two requests for the same segment make it once.
    locks: Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
}

impl App {
    pub fn new(cfg: Config) -> Arc<Self> {
        let jobs = Semaphore::new(cfg.jobs.max(1));
        Arc::new(Self { cfg, jobs, tasks: Tasks::default(), locks: Mutex::default() })
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
    pub(crate) async fn make_once<F, Fut>(
        &self,
        video: &Video,
        kind: &'static str,
        out: &Path,
        make: F,
    ) -> anyhow::Result<()>
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

pub fn router(app: Arc<App>) -> Router {
    let hls = Router::new()
        .route("/hls/master.m3u8", get(master))
        .route("/hls/subtitles.json", get(crate::subtitles::list))
        .route("/hls/subtitles/:file", get(crate::subtitles::track))
        .route("/hls/:variant/index.m3u8", get(media))
        .route("/hls/:variant/:segment", get(segment))
        .route("/tasks", get(tasks))
        .layer(middleware::from_fn_with_state(app.clone(), guard));
    Router::new().route("/health", get(|| async { "ok" })).merge(hls).with_state(app)
}

/// Compares without stopping at the first difference.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn guard(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let given =
        req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
    if !given.is_some_and(|t| same(t, &app.cfg.token)) {
        return fail(StatusCode::UNAUTHORIZED, "streaming needs the master's token").into_response();
    }
    next.run(req).await
}

#[derive(Deserialize)]
pub(crate) struct Which {
    /// The place in the file tree: `<user>/<device>/<path>`, or `<path>` for a master file (they are at the root).
    p: String,
    /// Hex root hash of the content.
    h: String,
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

pub(crate) fn locate(app: &App, q: &Which) -> Result<Video, Failure> {
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
    variants.into_iter().find(|v| v.name == name).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no such quality"))
}

async fn media(
    State(app): State<Arc<App>>,
    UrlPath(variant): UrlPath<String>,
    Query(q): Query<Which>,
) -> Result<Response, Failure> {
    let video = locate(&app, &q)?;
    let source = describe(&app, &video).await?;
    pick(hls::variants(&source), &variant)?;
    Ok(reply(PLAYLIST, "private, max-age=60", hls::media_playlist(source.duration)))
}

async fn segment(
    State(app): State<Arc<App>>,
    UrlPath((variant, name)): UrlPath<(String, String)>,
    Query(q): Query<Which>,
) -> Result<Response, Failure> {
    let n = hls::parse_segment(&name).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no such segment"))?;
    let video = locate(&app, &q)?;
    let source = describe(&app, &video).await?;
    let v = pick(hls::variants(&source), &variant)?;
    let (start, len) =
        hls::segment_span(n, source.duration).ok_or_else(|| fail(StatusCode::NOT_FOUND, "no such segment"))?;

    let out = video.dir.join(format!("{}-seg{n}.ts", v.name));
    let cfg = &app.cfg;
    let source_file = &video.source;
    app.make_once(&video, "segment", &out, |tmp| async move {
        let args = ffmpeg::segment_args(source_file, &tmp, &v, start, len, source.has_audio, &cfg.vcodec);
        ffmpeg::run(&cfg.ffmpeg, &args).await
    })
    .await
    .map_err(|e| internal("making the segment", e))?;
    let bytes = std::fs::read(&out).map_err(|e| internal("reading the segment", e.into()))?;
    Ok(reply("video/mp2t", "private, max-age=86400", bytes))
}

/// What the service is making and what failed lately, for whoever runs it (the master shows it next to its own queue).
async fn tasks(State(app): State<Arc<App>>) -> Response {
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
mod tests {
    #[test]
    fn paths_stay_inside_the_tree_and_master_files_are_one_part() {
        for ok in ["clip.mp4", "scans/2024/clip.mp4", "admin/pc/clip.mp4", "اناشيد/ملف.mp4"] {
            assert!(super::valid_path(ok), "{ok}");
        }
        for bad in ["", "/", "../clip.mp4", "a/../b.mp4", "a//b.mp4", "a\\b.mp4", "c:/b.mp4", "a/./b.mp4"] {
            assert!(!super::valid_path(bad), "{bad}");
        }
    }

    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    const H: &str = "ab12cd34";

    struct Fixture {
        app: Router,
        data: tempfile::TempDir,
        cache: PathBuf,
    }

    fn fixture() -> Fixture {
        let data = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(data.path().join("files/pc")).unwrap();
        std::fs::write(data.path().join("files/pc/clip.mkv"), b"not really a video").unwrap();
        let cache = data.path().join("cache");
        let app = router(App::new(Config {
            data: data.path().to_path_buf(),
            cache: cache.clone(),
            token: "secret-token".into(),
            // Programs that do not exist: a request that needs one fails, one that does not, works.
            ffmpeg: "fafnirx-no-such-ffmpeg".into(),
            ffprobe: "fafnirx-no-such-ffprobe".into(),
            vcodec: "libx264".into(),
            jobs: 2,
        }));
        Fixture { app, data, cache }
    }

    /// The video as if it had been probed already: 1280x720, 14.5 s.
    fn probed(f: &Fixture) {
        let dir = f.cache.join(cache_key("pc/clip.mkv", H));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("probe.txt"),
            source_text(&Source { width: 1280, height: 720, duration: 14.5, has_audio: true }),
        )
        .unwrap();
    }

    async fn get(f: &Fixture, uri: &str, token: Option<&str>) -> (StatusCode, String) {
        let mut req = Request::builder().uri(uri);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let resp = f.app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let status = resp.status();
        (status, String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned())
    }

    const T: Option<&str> = Some("secret-token");

    #[tokio::test]
    async fn only_the_master_may_ask() {
        let f = fixture();
        assert_eq!(get(&f, "/health", None).await.0, StatusCode::OK, "health needs no token");
        let uri = format!("/hls/master.m3u8?p=pc/clip.mkv&h={H}");
        assert_eq!(get(&f, &uri, None).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(get(&f, &uri, Some("wrong")).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(get(&f, &uri, Some("secret-token-and-more")).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn requests_cannot_leave_the_file_tree() {
        let f = fixture();
        for p in [
            "../etc/passwd",
            "pc/../../x",
            "pc/./clip.mkv",
            "/pc/clip.mkv",
            "pc//clip.mkv",
            "clip.mkv",
            "pc%5Cclip.mkv",
            "c:/x",
        ] {
            let (status, _) = get(&f, &format!("/hls/master.m3u8?p={p}&h={H}"), T).await;
            assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND, "{p}: {status}");
        }
        assert_eq!(get(&f, "/hls/master.m3u8?p=pc/clip.mkv&h=nothex", T).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(get(&f, &format!("/hls/master.m3u8?p=pc/missing.mkv&h={H}"), T).await.0, StatusCode::NOT_FOUND);
        assert_eq!(get(&f, "/hls/master.m3u8", T).await.0, StatusCode::BAD_REQUEST, "p and h are needed");
    }

    #[tokio::test]
    async fn playlists_come_without_transcoding_anything() {
        let f = fixture();
        probed(&f);
        let q = format!("p=pc/clip.mkv&h={H}");
        let (status, master) = get(&f, &format!("/hls/master.m3u8?{q}"), T).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            master.contains("720p/index.m3u8") && master.contains("360p/index.m3u8") && !master.contains("1080p"),
            "{master}"
        );
        let (status, media) = get(&f, &format!("/hls/720p/index.m3u8?{q}"), T).await;
        assert_eq!(status, StatusCode::OK);
        assert!(media.contains("seg2.ts") && !media.contains("seg3.ts"), "{media}");
        assert_eq!(
            get(&f, &format!("/hls/1080p/index.m3u8?{q}"), T).await.0,
            StatusCode::NOT_FOUND,
            "a quality it was not offered"
        );
        assert_eq!(get(&f, &format!("/hls/720p/seg3.ts?{q}"), T).await.0, StatusCode::NOT_FOUND, "past the end");
        assert_eq!(get(&f, &format!("/hls/720p/seg1.mp4?{q}"), T).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_segment_that_cannot_be_made_is_an_error_and_leaves_nothing_behind() {
        let f = fixture();
        probed(&f);
        let q = format!("p=pc/clip.mkv&h={H}");
        let (status, body) = get(&f, &format!("/hls/720p/seg0.ts?{q}"), T).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        let leftovers: Vec<_> = std::fs::read_dir(f.cache.join(cache_key("pc/clip.mkv", H)))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("seg"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[tokio::test]
    async fn a_segment_already_made_is_served_as_it_is() {
        let f = fixture();
        probed(&f);
        let dir = f.cache.join(cache_key("pc/clip.mkv", H));
        std::fs::write(dir.join("720p-seg1.ts"), b"TS-BYTES").unwrap();
        let resp = f
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/hls/720p/seg1.ts?p=pc/clip.mkv&h={H}"))
                    .header(header::AUTHORIZATION, "Bearer secret-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers()[header::CONTENT_TYPE], "video/mp2t");
        assert_eq!(resp.into_body().collect().await.unwrap().to_bytes().as_ref(), b"TS-BYTES");
    }

    #[tokio::test]
    async fn a_video_that_cannot_be_probed_is_not_a_video() {
        let f = fixture();
        let (status, _) = get(&f, &format!("/hls/master.m3u8?p=pc/clip.mkv&h={H}"), T).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn changed_content_has_its_own_cache_folder() {
        assert_ne!(cache_key("pc/a.mkv", "aa"), cache_key("pc/a.mkv", "bb"));
        assert_ne!(cache_key("pc/a.mkv", "aa"), cache_key("pc/b.mkv", "aa"));
        assert_eq!(cache_key("pc/a.mkv", "aa").len(), 32);
    }

    #[test]
    fn idle_cache_folders_are_swept_and_used_ones_kept() {
        let f = fixture();
        std::fs::create_dir_all(f.cache.join("old")).unwrap();
        std::fs::write(f.cache.join("old/.used"), b"").unwrap();
        std::fs::create_dir_all(f.cache.join("new")).unwrap();
        std::fs::write(f.cache.join("new/.used"), b"").unwrap();
        let old = std::fs::File::options().write(true).open(f.cache.join("old/.used")).unwrap();
        old.set_modified(std::time::SystemTime::now() - Duration::from_secs(10 * 86400)).unwrap();
        assert_eq!(sweep(&f.cache, Duration::from_secs(7 * 86400)), 1);
        assert!(!f.cache.join("old").exists() && f.cache.join("new").exists());
        drop(f.data);
    }

    #[test]
    fn tokens_are_compared_whole() {
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab") && !same("", "a"));
    }
}
