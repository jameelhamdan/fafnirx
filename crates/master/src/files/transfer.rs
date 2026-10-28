//! Moving file contents: download (with ranges, so video and photos stream), previews, bulk zip, and
//! chunked, resumable uploads that fit behind proxies with request size limits.

use crate::files::browse::{expand, find_device, find_device_to_change, version_file, FileRef};
use crate::files::ingest::{self, NewVersion};
use crate::library::{artwork, meta};
use crate::platform::db;
use crate::platform::id::Id;
use crate::repo::{devices, files};
use crate::web::access::Access;
use crate::web::{run, ApiError, Res, Shared};
use axum::body::Body;
use axum::extract::{Form, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use fafnirx_common::{base_name, clean_remote_path, secs_of_ns, unix_now};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

/// One upload being filled chunk by chunk.
#[derive(Clone)]
pub struct Upload {
    /// The user who started it: nobody else can add to it or finish it.
    user_id: i64,
    device: String,
    path: String,
    size: u64,
    mtime_ns: i64,
    received: u64,
    file: PathBuf,
    touched: Instant,
}

const MAX_CHUNK: u64 = 64 * 1024 * 1024;
const MAX_UPLOADS: usize = 2000;
const STALE_UPLOAD: Duration = Duration::from_secs(24 * 3600);

#[derive(Deserialize)]
pub struct FileQuery {
    device: String,
    path: String,
    version: Option<i64>,
    inline: Option<u8>,
    size: Option<u32>,
}

/// What a download needs to know about a stored version.
struct Stored {
    disk: PathBuf,
    root: Vec<u8>,
    size: u64,
    name: String,
    /// The video codec, when it is known.
    codec: Option<String>,
    /// A picture that sits next to a video and stands for it (see `library::artwork`): its place on disk and content.
    art: Option<(PathBuf, Vec<u8>)>,
}

fn stored_version(
    state: &Shared,
    who: Access,
    q: &FileQuery,
) -> impl FnOnce(&mut postgres::Client) -> anyhow::Result<Stored> {
    let (store, device, path, version) = (state.store.clone(), q.device.clone(), q.path.clone(), q.version);
    move |c| {
        let dev = find_device(c, &who, &device)?;
        let path = clean_remote_path(&path).map_err(|e| ApiError::bad(e.to_string()))?;
        let file = files::locate(c, dev.id, &path)?.ok_or_else(|| ApiError::not_found("No such file"))?;
        let (file_id, current, deleted) = (file.id, file.current, file.deleted_at);
        let version = version.or(current).ok_or_else(|| ApiError::not_found("No such version"))?;
        let (root, size) =
            files::version_content(c, file_id, version)?.ok_or_else(|| ApiError::not_found("No such version"))?;
        let disk = version_file(&store, &dev.name, &path, version, current, deleted.is_some())?;
        let art = if deleted.is_none() && Some(version) == current {
            artwork::lookup(c, &store, &dev.name, &path)
        } else {
            None
        };
        Ok(Stored { disk, root, size: size as u64, name: base_name(&path).to_string(), codec: None, art })
    }
}

/// `filename*=UTF-8''...` value: everything but unreserved characters is percent-encoded.
fn percent_encode(name: &str) -> String {
    name.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Kinds a browser may show in place. HTML and SVG can run script, so they are always downloaded.
fn shows_inline(mime: &str) -> bool {
    mime != "image/svg+xml"
        && (mime.starts_with("image/")
            || mime.starts_with("video/")
            || mime.starts_with("audio/")
            || mime == "application/pdf"
            || mime == "text/plain")
}

/// The byte range a `Range` header asks for, as (first, last) inclusive.
fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?;
    if spec.contains(',') || size == 0 {
        return None;
    }
    let (from, to) = spec.split_once('-')?;
    let (start, end) = match (from.trim(), to.trim()) {
        ("", suffix) => (size.saturating_sub(suffix.parse().ok()?), size - 1),
        (from, "") => (from.parse().ok()?, size - 1),
        (from, to) => (from.parse().ok()?, to.parse::<u64>().ok()?.min(size - 1)),
    };
    (start <= end && start < size).then_some((start, end))
}

pub async fn download(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Query(q): Query<FileQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let stored = run(&state, stored_version(&state, who, &q)).await?;
    let etag = format!("\"{}\"", hex::encode(&stored.root));
    let file =
        SendFile { disk: &stored.disk, etag, size: stored.size, name: &stored.name, inline: q.inline == Some(1) };
    send_file(&state, &headers, file).await
}

/// A file on disk to answer a request with.
struct SendFile<'a> {
    disk: &'a std::path::Path,
    etag: String,
    /// What the database says; the real length on disk wins when they differ.
    size: u64,
    /// Names the file for the browser, and decides the media type.
    name: &'a str,
    /// Show in the browser instead of saving, for the kinds that are safe to show.
    inline: bool,
}

/// Streams a file with `Range` support, so that photos and videos can be seen while they download.
async fn send_file(state: &Shared, headers: &HeaderMap, f: SendFile<'_>) -> Result<Response, ApiError> {
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(f.etag.as_str()) {
        return Ok((StatusCode::NOT_MODIFIED, [(header::ETAG, f.etag)]).into_response());
    }
    let mut file = tokio::fs::File::open(f.disk)
        .await
        .map_err(|_| ApiError::not_found("The content of this version is no longer stored"))?;
    let size = file.metadata().await.map(|m| m.len()).unwrap_or(f.size);
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|v| parse_range(v, size));
    let (status, start, len) = match range {
        Some((a, b)) => (StatusCode::PARTIAL_CONTENT, a, b - a + 1),
        None => (StatusCode::OK, 0, size),
    };
    file.seek(std::io::SeekFrom::Start(start)).await.map_err(anyhow::Error::from)?;
    let permit = state.streams.clone().acquire_owned().await.map_err(anyhow::Error::from)?;
    let stream = ReaderStream::with_capacity(file.take(len), 256 * 1024).map(move |chunk| {
        let _held = &permit;
        chunk
    });
    let mime = meta::mime(f.name);
    let disposition = if f.inline && shows_inline(&mime) { "inline" } else { "attachment" };
    let mut resp = Response::new(Body::from_stream(stream));
    *resp.status_mut() = status;
    let h = resp.headers_mut();
    let set = |h: &mut HeaderMap, k: header::HeaderName, v: String| {
        if let Ok(v) = HeaderValue::from_str(&v) {
            h.insert(k, v);
        }
    };
    set(h, header::CONTENT_TYPE, mime);
    set(h, header::CONTENT_LENGTH, len.to_string());
    set(h, header::ACCEPT_RANGES, "bytes".into());
    set(h, header::ETAG, f.etag);
    set(h, header::CACHE_CONTROL, "private, max-age=3600".into());
    set(h, header::X_CONTENT_TYPE_OPTIONS, "nosniff".into());
    set(h, header::CONTENT_DISPOSITION, format!("{disposition}; filename*=UTF-8''{}", percent_encode(f.name)));
    if let Some((a, b)) = range {
        set(h, header::CONTENT_RANGE, format!("bytes {a}-{b}/{size}"));
    }
    Ok(resp)
}

/// A JPEG picture of an image or a video frame, from the cache or made now.
async fn preview_response(state: &Shared, stored: &Stored, max: u32, immutable: bool) -> Result<Response, ApiError> {
    if !meta::can_preview(&stored.name) {
        return Err(ApiError::not_found("No preview for this kind of file"));
    }
    let _slot = state.previews.acquire().await.map_err(anyhow::Error::from)?;
    let cache = state.data_dir.join("cache").join("thumbs");
    let (disk, root) = match &stored.art {
        Some((disk, root)) => (disk.clone(), hex::encode(root)),
        None => (stored.disk.clone(), hex::encode(&stored.root)),
    };
    let thumb = tokio::task::spawn_blocking(move || meta::thumbnail(&disk, &root, max, &cache))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|e| {
            tracing::warn!("preview: {e:#}");
            ApiError::not_found("This file cannot be previewed")
        })?;
    let bytes = tokio::fs::read(thumb).await.map_err(anyhow::Error::from)?;
    // The picture belongs to one content: when the link says which, it never changes.
    let cache_control = if immutable { "private, max-age=31536000, immutable" } else { "private, max-age=86400" };
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, cache_control)], bytes).into_response())
}

pub async fn thumbnail(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Query(q): Query<FileQuery>,
) -> Result<Response, ApiError> {
    let stored = run(&state, stored_version(&state, who, &q)).await?;
    let max = q.size.unwrap_or(256).clamp(64, 1600);
    preview_response(&state, &stored, max, false).await
}

/// The file with this id as the library knows it: its current content, deleted or not.
fn stored_by_id(
    state: &Shared,
    who: Access,
    id: uuid::Uuid,
) -> impl FnOnce(&mut postgres::Client) -> anyhow::Result<Stored> {
    let store = state.store.clone();
    move |c| {
        let row = files::by_id(c, id)?.ok_or_else(|| ApiError::not_found("No such file"))?;
        let (device, path, version, deleted_at) = (row.folder, row.path, row.current_version_id, row.deleted_at);
        // A file of someone else's device does not exist for this user.
        who.check_see(&device).map_err(|_| ApiError::not_found("No such file"))?;
        Ok(Stored {
            disk: version_file(&store, &device, &path, version, Some(version), deleted_at.is_some())?,
            root: row.root_hash,
            size: row.size as u64,
            name: base_name(&path).to_string(),
            codec: row.codec,
            art: if deleted_at.is_none() { artwork::lookup(c, &store, &device, &path) } else { None },
        })
    }
}

/// What the library shows of a file: a small or large picture, the video as played in a browser, the
/// original in place, or the original as a download.
#[derive(Clone, Copy, PartialEq)]
pub enum Variant {
    Thumb,
    Preview,
    Video,
    Original,
    Download,
}

impl Variant {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "thumb" => Self::Thumb,
            "preview" => Self::Preview,
            "video" => Self::Video,
            "original" => Self::Original,
            "download" => Self::Download,
            _ => return None,
        })
    }
}

#[derive(Deserialize)]
pub struct MediaQuery {
    /// The start of the content hash the link was made for (see `Asset::hash`).
    v: Option<String>,
}

/// Answers a request for one of the views of a file.
pub async fn media(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path((Id(id), variant)): Path<(Id, String)>,
    Query(q): Query<MediaQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let variant = Variant::parse(&variant).ok_or_else(|| ApiError::not_found("No such view"))?;
    let stored = run(&state, stored_by_id(&state, who, id)).await?;
    let root_hex = hex::encode(&stored.root);
    let immutable = q.v.as_deref().is_some_and(|v| !v.is_empty() && root_hex.starts_with(v));
    match variant {
        Variant::Thumb => preview_response(&state, &stored, meta::THUMB, immutable).await,
        Variant::Preview => preview_response(&state, &stored, meta::PREVIEW, immutable).await,
        Variant::Video => {
            let proxy = meta::proxy_path(&state.data_dir.join("cache"), &root_hex);
            if proxy.is_file() {
                let stem = stored.name.rsplit_once('.').map_or(stored.name.as_str(), |(n, _)| n);
                let name = format!("{stem}.mp4");
                let file = SendFile {
                    disk: &proxy,
                    etag: format!("\"{root_hex}-proxy\""),
                    size: 0,
                    name: &name,
                    inline: true,
                };
                return send_file(&state, &headers, file).await;
            }
            if !crate::library::media::plays_everywhere(&meta::extension(&stored.name), stored.codec.as_deref()) {
                return Err(ApiError::not_found("A copy that plays in the browser is still being made"));
            }
            send_original(&state, &headers, &stored, true).await
        }
        Variant::Original | Variant::Download => {
            send_original(&state, &headers, &stored, variant == Variant::Original).await
        }
    }
}

async fn send_original(
    state: &Shared,
    headers: &HeaderMap,
    stored: &Stored,
    inline: bool,
) -> Result<Response, ApiError> {
    let etag = format!("\"{}\"", hex::encode(&stored.root));
    send_file(state, headers, SendFile { disk: &stored.disk, etag, size: stored.size, name: &stored.name, inline })
        .await
}

#[derive(Deserialize)]
pub struct ZipForm {
    /// JSON: `[{"device": "...", "path": "..."}]`; an empty path is the whole device.
    items: String,
    name: Option<String>,
}

pub async fn zip(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Form(form): Form<ZipForm>,
) -> Result<Response, ApiError> {
    let items: Vec<FileRef> =
        serde_json::from_str(&form.items).map_err(|_| ApiError::bad("items must be a JSON list"))?;
    if items.is_empty() {
        return Err(ApiError::bad("Nothing selected"));
    }
    let store = state.store.clone();
    let entries = run(&state, move |c| {
        let several = items.iter().map(|i| &i.device).collect::<std::collections::HashSet<_>>().len() > 1;
        let mut entries = Vec::new();
        for item in &items {
            let dev = find_device(c, &who, &item.device)?;
            let selected = item.path.trim_matches('/');
            // Names inside the zip start at the selected folder (or file), not at the device root.
            let parent = selected.rfind('/').map_or(0, |i| i + 1);
            for path in expand(c, &dev, selected, false)? {
                let mtime = secs_of_ns(files::current_mtime(c, dev.id, &path)?);
                let inner = &path[parent.min(path.len())..];
                let name =
                    if several || selected.is_empty() { format!("{}/{inner}", dev.name) } else { inner.to_string() };
                entries.push(super::zip::Entry { name, path: store.live_path(&dev.name, &path)?, mtime });
            }
        }
        Ok(entries)
    })
    .await?;
    if entries.is_empty() {
        return Err(ApiError::not_found("Nothing to download"));
    }
    let (mut writer, reader) = tokio::io::duplex(256 * 1024);
    let state2 = state.clone();
    tokio::spawn(async move {
        let _slot = state2.streams.acquire().await;
        if let Err(e) = super::zip::write(&mut writer, entries).await {
            tracing::debug!("zip download ended early: {e}");
        }
        let _ = writer.shutdown().await;
    });
    let name = form.name.filter(|n| !n.is_empty()).unwrap_or_else(|| "fafnirx".into());
    // Any script is fine in the name; what a header or a file system would trip on is not.
    let name = name.chars().filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ')).collect::<String>();
    let name = if name.trim().is_empty() { "fafnirx".to_string() } else { name };
    let ascii = name.chars().map(|c| if c.is_ascii() { c } else { '_' }).collect::<String>();
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"{ascii}.zip\"; filename*=UTF-8''{}",
                    percent_encode(&format!("{name}.zip"))
                ),
            ),
        ],
        Body::from_stream(ReaderStream::new(reader)),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct NewUpload {
    device: String,
    path: String,
    size: u64,
    /// Modification time on the uploading computer, unix milliseconds.
    mtime_ms: Option<i64>,
}

#[derive(Serialize)]
pub struct UploadId {
    id: String,
    received: u64,
}

pub async fn start_upload(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(b): Json<NewUpload>,
) -> Res<UploadId> {
    let path = clean_remote_path(&b.path).map_err(|e| ApiError::bad(e.to_string()))?;
    let device = b.device.clone();
    let user = who.clone();
    let kind: String = run(&state, move |c| {
        user.check_write(&device)?;
        Ok(devices::kind_of_folder(c, &device)?.ok_or_else(|| ApiError::not_found("No such device"))?)
    })
    .await?;
    // Only folders that live on the master take uploads: a device's files are what the device sends.
    if !crate::files::browse::editable_kind(&kind) {
        return Err(ApiError::bad("Files can only be uploaded to a local folder. Create one on the Files page first."));
    }
    state.store.live_path(&b.device, &path).map_err(|e| ApiError::bad(e.to_string()))?;
    let id = hex::encode(db::random_bytes::<16>());
    let file = state.store.temp_path();
    tokio::fs::File::create(&file).await.map_err(anyhow::Error::from)?;
    let mut uploads = state.uploads.lock().unwrap_or_else(|e| e.into_inner());
    uploads.retain(|_, u| {
        let keep = u.touched.elapsed() < STALE_UPLOAD;
        if !keep {
            let _ = std::fs::remove_file(&u.file);
        }
        keep
    });
    if uploads.len() >= MAX_UPLOADS {
        let _ = std::fs::remove_file(&file);
        return Err(ApiError::conflict("Too many uploads in progress"));
    }
    let mtime_ns =
        b.mtime_ms.map_or_else(|| unix_now().saturating_mul(1_000_000_000), |ms| ms.saturating_mul(1_000_000));
    uploads.insert(
        id.clone(),
        Upload {
            user_id: who.user_id,
            device: b.device,
            path,
            size: b.size,
            mtime_ns,
            received: 0,
            file,
            touched: Instant::now(),
        },
    );
    Ok(Json(UploadId { id, received: 0 }))
}

fn session<T>(state: &Shared, user_id: i64, id: &str, f: impl FnOnce(&mut Upload) -> T) -> Result<T, ApiError> {
    let mut uploads = state.uploads.lock().unwrap_or_else(|e| e.into_inner());
    let upload = uploads
        .get_mut(id)
        .filter(|u| u.user_id == user_id)
        .ok_or_else(|| ApiError::not_found("No such upload (it may have expired)"))?;
    upload.touched = Instant::now();
    Ok(f(upload))
}

#[derive(Serialize)]
pub struct Received {
    received: u64,
    size: u64,
}

pub async fn upload_status(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(id): Path<String>,
) -> Res<Received> {
    Ok(Json(session(&state, who.user_id, &id, |u| Received { received: u.received, size: u.size })?))
}

#[derive(Deserialize)]
pub struct Offset {
    offset: u64,
}

pub async fn upload_chunk(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(id): Path<String>,
    Query(q): Query<Offset>,
    body: Body,
) -> Result<Json<Received>, Response> {
    let (file, received, size) = session(&state, who.user_id, &id, |u| (u.file.clone(), u.received, u.size))
        .map_err(IntoResponse::into_response)?;
    if q.offset != received {
        let wrong = Json(serde_json::json!({ "error": format!("expected offset {received}"), "received": received }));
        return Err((StatusCode::CONFLICT, wrong).into_response());
    }
    let fail = |e: anyhow::Error| ApiError::from(e).into_response();
    let mut out = tokio::fs::OpenOptions::new().write(true).open(&file).await.map_err(|e| fail(e.into()))?;
    out.seek(std::io::SeekFrom::Start(received)).await.map_err(|e| fail(e.into()))?;
    let mut written = 0u64;
    let mut stream = body.into_data_stream();
    while let Some(part) = stream.next().await {
        let part = part.map_err(|e| fail(anyhow::anyhow!("upload interrupted: {e}")))?;
        written += part.len() as u64;
        if written > MAX_CHUNK || received + written > size {
            return Err(ApiError::bad("The chunk is larger than allowed or past the end of the file").into_response());
        }
        out.write_all(&part).await.map_err(|e| fail(e.into()))?;
    }
    out.flush().await.map_err(|e| fail(e.into()))?;
    let received = session(&state, who.user_id, &id, |u| {
        u.received = received + written;
        u.received
    })
    .map_err(IntoResponse::into_response)?;
    Ok(Json(Received { received, size }))
}

#[derive(Serialize)]
pub struct Finished {
    device: String,
    path: String,
    size: u64,
}

pub async fn finish_upload(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(id): Path<String>,
) -> Res<Finished> {
    let upload = {
        let mut uploads = state.uploads.lock().unwrap_or_else(|e| e.into_inner());
        let u = uploads
            .get(&id)
            .filter(|u| u.user_id == who.user_id)
            .ok_or_else(|| ApiError::not_found("No such upload (it may have expired)"))?;
        if u.received != u.size {
            return Err(ApiError::conflict(format!("Only {} of {} bytes arrived", u.received, u.size)));
        }
        uploads.remove(&id).expect("just found")
    };
    let store = state.store.clone();
    // Committing consumes the file whether it works or not, so a second name for it is kept until it has worked: the
    // upload can then be finished again after a failure instead of being sent again. (No second name, no second try.)
    let spare = {
        let mut name = upload.file.clone().into_os_string();
        name.push(".again");
        PathBuf::from(name)
    };
    let kept =
        std::fs::hard_link(&upload.file, &spare).is_ok().then(|| Upload { file: spare.clone(), ..upload.clone() });
    let result = run(&state, move |c| {
        let (root, size) = crate::platform::store::hash_file(&upload.file)?;
        // Checked again: the user may have lost the right while the file was on its way.
        let dev = find_device_to_change(c, &who, &upload.device)?;
        ingest::check_names(c, &dev, &upload.path, None, true)?;
        ingest::commit_version(
            c,
            &store,
            &upload.file,
            &NewVersion {
                device: &dev,
                path: &upload.path,
                root,
                size,
                mtime: upload.mtime_ns,
                mode: 0,
                renamed_from: None,
            },
        )?;
        Ok(Finished { device: upload.device, path: upload.path, size })
    })
    .await;
    match kept {
        // Not finished: the upload stays. It expires like any other upload that nobody touches.
        Some(kept) if result.is_err() => {
            state.uploads.lock().unwrap_or_else(|e| e.into_inner()).insert(id, kept);
        }
        Some(_) => {
            let _ = std::fs::remove_file(&spare);
        }
        None => {}
    }
    Ok(Json(result?))
}

pub async fn abort_upload(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(id): Path<String>,
) -> Res<crate::web::Done> {
    {
        let mut uploads = state.uploads.lock().unwrap_or_else(|e| e.into_inner());
        if uploads.get(&id).is_some_and(|u| u.user_id == who.user_id) {
            if let Some(u) = uploads.remove(&id) {
                let _ = std::fs::remove_file(u.file);
            }
        }
    }
    crate::web::done()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=0-5000", 1000), Some((0, 999)));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("bytes=0-1,5-6", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
        assert_eq!(parse_range("bytes=0-1", 0), None);
    }

    #[test]
    fn names_are_safe_in_headers() {
        assert_eq!(percent_encode("my photo é.jpg"), "my%20photo%20%C3%A9.jpg");
        assert!(shows_inline("image/png") && shows_inline("application/pdf"));
        assert!(!shows_inline("image/svg+xml") && !shows_inline("text/html"));
    }
}
