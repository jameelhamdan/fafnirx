//! Chunked, resumable uploads that fit behind proxies with request size limits.

use crate::files::browse::find_device_to_change;
use crate::files::ingest::{self, NewVersion};
use crate::platform::db;
use crate::repo::devices::DeviceKind;
use crate::web::access::Access;
use crate::web::{run, ApiError, Res, Shared};
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use fafnirx_common::{clean_remote_path, unix_now, LockExt};
use fafnirx_db::queries as sql;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

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

pub async fn start_upload(State(state): State<Shared>, Extension(who): Extension<Access>, Json(b): Json<NewUpload>) -> Res<UploadId> {
    let path = clean_remote_path(&b.path).map_err(|e| ApiError::bad(e.to_string()))?;
    let device = b.device.clone();
    let user = who.clone();
    let kind = run(&state, move |c| {
        user.check_write(&device)?;
        Ok(DeviceKind::from_db(
            &sql::devices::device_kind_by_folder()
                .bind(c, &device)
                .opt()?
                .ok_or_else(|| ApiError::not_found("No such device"))?,
        ))
    })
    .await?;
    // Only folders that live on the master take uploads: a device's files are what the device sends.
    if !kind.editable() {
        return Err(ApiError::bad("Files can only be uploaded to a local folder. Create one on the Files page first."));
    }
    state.store.live_path(&b.device, &path).map_err(|e| ApiError::bad(e.to_string()))?;
    let id = hex::encode(db::random_bytes::<16>());
    let file = state.store.temp_path();
    tokio::fs::File::create(&file).await.map_err(anyhow::Error::from)?;
    let mut uploads = state.uploads.locked();
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
    let mtime_ns = b
        .mtime_ms
        .map_or_else(|| unix_now().saturating_mul(1_000_000_000), |ms| ms.saturating_mul(1_000_000));
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
    let mut uploads = state.uploads.locked();
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

pub async fn upload_status(State(state): State<Shared>, Extension(who): Extension<Access>, Path(id): Path<String>) -> Res<Received> {
    Ok(Json(session(&state, who.user_id, &id, |u| Received { received: u.received, size: u.size })?))
}

#[derive(Deserialize)]
pub struct Offset {
    offset: u64,
}

pub async fn upload_chunk(State(state): State<Shared>, Extension(who): Extension<Access>, Path(id): Path<String>, Query(q): Query<Offset>, body: Body) -> Result<Json<Received>, Response> {
    let (file, received, size) = session(&state, who.user_id, &id, |u| (u.file.clone(), u.received, u.size)).map_err(IntoResponse::into_response)?;
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

pub async fn finish_upload(State(state): State<Shared>, Extension(who): Extension<Access>, Path(id): Path<String>) -> Res<Finished> {
    let upload = {
        let mut uploads = state.uploads.locked();
        let u = uploads
            .get(&id)
            .filter(|u| u.user_id == who.user_id)
            .ok_or_else(|| ApiError::not_found("No such upload (it may have expired)"))?;
        if u.received != u.size {
            let (received, size) = (u.received, u.size);
            return Err(ApiError::conflict(format!("Only {received} of {size} bytes arrived")));
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
    let kept = std::fs::hard_link(&upload.file, &spare)
        .is_ok()
        .then(|| Upload { file: spare.clone(), ..upload.clone() });
    let result = run(&state, move |c| {
        let (root, size) = crate::platform::store::hash_file(&upload.file)?;
        // Checked again: the user may have lost the right while the file was on its way.
        let dev = find_device_to_change(c, &who, &upload.device)?;
        ingest::check_names(c, &dev, &upload.path, None, true)?;
        ingest::commit_version(
            c,
            &store,
            &upload.file,
            &NewVersion { device: &dev, path: &upload.path, root, size, mtime: upload.mtime_ns, mode: 0, renamed_from: None },
        )?;
        Ok(Finished { device: upload.device, path: upload.path, size })
    })
    .await;
    match kept {
        // Not finished: the upload stays. It expires like any other upload that nobody touches.
        Some(kept) if result.is_err() => {
            state.uploads.locked().insert(id, kept);
        }
        Some(_) => {
            let _ = std::fs::remove_file(&spare);
        }
        None => {}
    }
    Ok(Json(result?))
}

pub async fn abort_upload(State(state): State<Shared>, Extension(who): Extension<Access>, Path(id): Path<String>) -> Res<crate::web::Done> {
    {
        let mut uploads = state.uploads.locked();
        if uploads.get(&id).is_some_and(|u| u.user_id == who.user_id) {
            if let Some(u) = uploads.remove(&id) {
                let _ = std::fs::remove_file(u.file);
            }
        }
    }
    crate::web::done()
}
