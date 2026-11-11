//! Moving file contents out: download (with ranges, so video and photos stream), previews and bulk zip. The way in is `upload`.

use crate::files::browse::VersionAt;
use crate::files::browse::{find_device, version_file, FileRef};
use crate::library::{artwork, meta};
use crate::platform::id::Id;
use crate::repo::files;
use crate::repo::jobs::{self, JobKind, JobStatus};
use crate::web::access::Access;
use crate::web::{run, ApiError, Shared};
use axum::body::Body;
use axum::extract::{Form, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use fafnirx_common::{base_name, clean_remote_path, secs_of_ns, unix_now};
use fafnirx_db::queries as sql;
use futures_util::StreamExt;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

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
    /// A picture that sits next to a video and stands for it (see `library::artwork`): its place on disk and content.
    art: Option<(PathBuf, Vec<u8>)>,
    /// Which file and content this is, for a picture of it that is asked for.
    id: uuid::Uuid,
    device: String,
    path: String,
    version: i64,
    current: Option<i64>,
    deleted: bool,
}

fn stored_version(state: &Shared, who: Access, q: &FileQuery) -> impl FnOnce(&mut postgres::Client) -> anyhow::Result<Stored> {
    let (store, device, path, version) = (state.store.clone(), q.device.clone(), q.path.clone(), q.version);
    move |c| {
        let dev = find_device(c, &who, &device)?;
        let path = clean_remote_path(&path).map_err(|e| ApiError::bad(e.to_string()))?;
        let file = files::locate(c, dev.id, &path)?.ok_or_else(|| ApiError::not_found("No such file"))?;
        let (file_id, current, deleted) = (file.id, file.current, file.deleted_at);
        let version = version.or(current).ok_or_else(|| ApiError::not_found("No such version"))?;
        let (root, size) = files::version_content(c, file_id, version)?.ok_or_else(|| ApiError::not_found("No such version"))?;
        let disk = version_file(&store, &VersionAt { device: &dev.name, path: &path, version, current, deleted: deleted.is_some() })?;
        let art = if deleted.is_none() && Some(version) == current {
            artwork::lookup(c, &store, &dev.name, &path)
        } else {
            None
        };
        Ok(Stored {
            disk,
            root,
            size: size as u64,
            name: base_name(&path).to_string(),
            art,
            id: file_id,
            device: dev.name.clone(),
            version,
            current,
            deleted: deleted.is_some(),
            path,
        })
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
    mime != "image/svg+xml" && (mime.starts_with("image/") || mime.starts_with("video/") || mime.starts_with("audio/") || mime == "application/pdf" || mime == "text/plain")
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

/// Whether a well-formed single range starts at or after the end (or asks for the last 0 bytes): nothing of the file can answer
/// it. A range that is not understood is ignored instead, and the whole file is sent.
fn past_the_end(header: &str, size: u64) -> bool {
    let Some((from, to)) = header.strip_prefix("bytes=").filter(|s| !s.contains(',')).and_then(|s| s.split_once('-')) else {
        return false;
    };
    match (from.trim(), to.trim()) {
        ("", suffix) => suffix.parse::<u64>().is_ok_and(|n| n == 0 || size == 0),
        (from, _) => from.parse::<u64>().is_ok_and(|start| start >= size),
    }
}

pub async fn download(State(state): State<Shared>, Extension(who): Extension<Access>, Query(q): Query<FileQuery>, headers: HeaderMap) -> Result<Response, ApiError> {
    let stored = run(&state, stored_version(&state, who, &q)).await?;
    let etag = format!("\"{}\"", hex::encode(&stored.root));
    let file = SendFile { disk: &stored.disk, etag, size: stored.size, name: &stored.name, inline: q.inline == Some(1) };
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
    let asked = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let range = asked.and_then(|v| parse_range(v, size));
    if asked.is_some_and(|v| range.is_none() && past_the_end(v, size)) {
        return Ok((StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{size}"))]).into_response());
    }
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
    set(
        h,
        header::CONTENT_DISPOSITION,
        format!("{disposition}; filename*=UTF-8''{}", percent_encode(f.name)),
    );
    if let Some((a, b)) = range {
        set(h, header::CONTENT_RANGE, format!("bytes {a}-{b}/{size}"));
    }
    Ok(resp)
}

/// A picture that could not be made is not asked for again for this long.
const FAILED_PICTURE_RETRY_SECS: i64 = 600;

/// What the job queue says about a picture that is not made yet.
enum Picture {
    /// A job is making it, or was just asked to: the browser is told when it is done.
    Coming,
    /// There will not be one.
    Never,
}

/// A JPEG picture of an image or a video frame. When it is made already it is sent. When it is not, a job makes it, and the
/// answer is "accepted": the browser waits for the master to say it is ready (`thumbs`, see `web::events`) and asks again.
/// Nothing is made when thumbnails are off.
async fn preview_response(state: &Shared, stored: &Stored, max: u32, immutable: bool) -> Result<Response, ApiError> {
    if !meta::thumbnails_enabled() || (!meta::can_preview(&stored.name) && stored.art.is_none()) {
        return Err(ApiError::not_found("No preview for this kind of file"));
    }
    let thumbs = state.data_dir.join("cache").join("thumbs");
    let picture_of = match &stored.art {
        Some((_, root)) => hex::encode(root),
        None => hex::encode(&stored.root),
    };
    if let Ok(bytes) = tokio::fs::read(meta::cached_picture(&thumbs, &picture_of, max)).await {
        // The picture belongs to one content: when the link says which, it never changes.
        let cache_control = if immutable { "private, max-age=31536000, immutable" } else { "private, max-age=86400" };
        return Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, cache_control)], bytes).into_response());
    }
    let job = serde_json::to_value(meta::ThumbJob {
        id: Id(stored.id).to_string(),
        device: stored.device.clone(),
        path: stored.path.clone(),
        root: hex::encode(&stored.root),
        key: picture_of,
        size: max,
        version: stored.version,
        current: stored.current,
        deleted: stored.deleted,
    })
    .map_err(anyhow::Error::from)?;
    let outcome = run(state, move |c| {
        Ok(match jobs::status_of(c, JobKind::Thumb, &job, unix_now() - FAILED_PICTURE_RETRY_SECS)? {
            Some(JobStatus::Failed) => Picture::Never,
            Some(JobStatus::Queued | JobStatus::Running) => Picture::Coming,
            _ => {
                jobs::enqueue(c, JobKind::Thumb, &job)?;
                Picture::Coming
            }
        })
    })
    .await?;
    match outcome {
        Picture::Never => Err(ApiError::not_found("This file cannot be previewed")),
        Picture::Coming => {
            meta::wake();
            Ok((
                StatusCode::ACCEPTED,
                [(header::CACHE_CONTROL, "no-store")],
                Json(serde_json::json!({ "pending": true })),
            )
                .into_response())
        }
    }
}

pub async fn thumbnail(State(state): State<Shared>, Extension(who): Extension<Access>, Query(q): Query<FileQuery>) -> Result<Response, ApiError> {
    let stored = run(&state, stored_version(&state, who, &q)).await?;
    // Two sizes only: every size asked for would be a file kept for good.
    let max = if q.size.unwrap_or(meta::THUMB) <= meta::THUMB { meta::THUMB } else { meta::PREVIEW };
    preview_response(&state, &stored, max, false).await
}

/// The file with this id as the library knows it: its current content, deleted or not.
fn stored_by_id(state: &Shared, who: Access, id: uuid::Uuid) -> impl FnOnce(&mut postgres::Client) -> anyhow::Result<Stored> {
    let store = state.store.clone();
    move |c| {
        let row = sql::files::file_by_id().bind(c, &id).opt()?.ok_or_else(|| ApiError::not_found("No such file"))?;
        let (device, path, version, deleted_at) = (row.folder, row.path, row.current_version_id, row.deleted_at);
        // A file of someone else's device does not exist for this user.
        who.check_see(&device).map_err(|_| ApiError::not_found("No such file"))?;
        Ok(Stored {
            disk: version_file(
                &store,
                &VersionAt { device: &device, path: &path, version, current: Some(version), deleted: deleted_at.is_some() },
            )?,
            root: row.root_hash,
            size: row.size as u64,
            name: base_name(&path).to_string(),
            art: if deleted_at.is_none() { artwork::lookup(c, &store, &device, &path) } else { None },
            id,
            version,
            current: Some(version),
            deleted: deleted_at.is_some(),
            device,
            path,
        })
    }
}

/// What the library shows of a file: a small or large picture, the original in place, or the original as a download.
#[derive(Clone, Copy, PartialEq)]
pub enum Variant {
    Thumb,
    Preview,
    Original,
    Download,
}

impl Variant {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "thumb" => Self::Thumb,
            "preview" => Self::Preview,
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
pub async fn media(State(state): State<Shared>, Extension(who): Extension<Access>, Path((Id(id), variant)): Path<(Id, String)>, Query(q): Query<MediaQuery>, headers: HeaderMap) -> Result<Response, ApiError> {
    let variant = Variant::parse(&variant).ok_or_else(|| ApiError::not_found("No such view"))?;
    let stored = run(&state, stored_by_id(&state, who, id)).await?;
    let root_hex = hex::encode(&stored.root);
    let immutable = q.v.as_deref().is_some_and(|v| !v.is_empty() && root_hex.starts_with(v));
    match variant {
        Variant::Thumb => preview_response(&state, &stored, meta::THUMB, immutable).await,
        Variant::Preview => preview_response(&state, &stored, meta::PREVIEW, immutable).await,
        Variant::Original | Variant::Download => send_original(&state, &headers, &stored, variant == Variant::Original).await,
    }
}

async fn send_original(state: &Shared, headers: &HeaderMap, stored: &Stored, inline: bool) -> Result<Response, ApiError> {
    let etag = format!("\"{}\"", hex::encode(&stored.root));
    send_file(state, headers, SendFile { disk: &stored.disk, etag, size: stored.size, name: &stored.name, inline }).await
}

#[derive(Deserialize)]
pub struct ZipForm {
    /// JSON: `[{"device": "...", "path": "..."}]`; an empty path is the whole device.
    items: String,
    name: Option<String>,
}

pub async fn zip(State(state): State<Shared>, Extension(who): Extension<Access>, Form(form): Form<ZipForm>) -> Result<Response, ApiError> {
    let items: Vec<FileRef> = serde_json::from_str(&form.items).map_err(|_| ApiError::bad("items must be a JSON list"))?;
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
            for (path, mtime) in files::expand_with_mtime(c, dev.id, selected)? {
                let mtime = secs_of_ns(mtime);
                let inner = &path[parent.min(path.len())..];
                let name = if several || selected.is_empty() { format!("{}/{inner}", dev.name) } else { inner.to_string() };
                entries.push(super::zip::Entry { name, path: store.live_path(&dev.name, &path)?, mtime });
            }
        }
        Ok(entries)
    })
    .await?;
    if entries.is_empty() {
        return Err(ApiError::not_found("Nothing to download"));
    }
    let (writer, reader) = tokio::io::duplex(256 * 1024);
    let state2 = state.clone();
    let failed = Arc::new(AtomicBool::new(false));
    let failed_here = failed.clone();
    tokio::spawn(async move {
        let _slot = state2.streams.acquire().await;
        if let Err(e) = super::zip::write(writer, entries, failed_here).await {
            tracing::debug!("zip download ended early: {e}");
        }
    });
    // The end of the stream is an error when the archive could not be finished: the browser then reports a failed
    // download instead of keeping a file that only looks complete.
    let ended = futures_util::stream::once(async move {
        if failed.load(Ordering::Relaxed) {
            Err(std::io::Error::other("an entry of the archive could not be read"))
        } else {
            Ok(axum::body::Bytes::new())
        }
    });
    let name = form.name.filter(|n| !n.is_empty()).unwrap_or_else(|| "fafnirx".into());
    // Any script is fine in the name; what a header or a file system would trip on is not.
    let name = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
        .collect::<String>();
    let name = if name.trim().is_empty() { "fafnirx".to_string() } else { name };
    let ascii = name.chars().map(|c| if c.is_ascii() { c } else { '_' }).collect::<String>();
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{ascii}.zip\"; filename*=UTF-8''{}", percent_encode(&format!("{name}.zip"))),
            ),
        ],
        Body::from_stream(ReaderStream::new(reader).chain(ended)),
    )
        .into_response())
}

#[cfg(test)]
#[path = "../../tests/unit/files/transfer.rs"]
mod tests;
