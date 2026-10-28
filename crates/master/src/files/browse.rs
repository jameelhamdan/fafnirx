//! The file browser: folders and files of the devices a user can reach (and the master files), search, details, history,
//! delete and restore. A device is named by its folder (`<user>/<device>`; `_master` for the master files).

use crate::devices::api::device_summaries;
use crate::files::ingest;
use crate::library::meta;
use crate::monitoring::api::Recent;
use crate::platform::db::{self, Device};
use crate::platform::store::MASTER_FOLDER;
use crate::repo::listing::{self, Listed, Page, Sort};
use crate::repo::{devices, events, files, settings};
use crate::web::access::Access;
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::{Query, State};
use axum::{Extension, Json};
use fafnirx_common::{base_name, clean_remote_path, secs_of_ns, DAY_SECS};
use postgres::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_LIMIT: i64 = 1000;
const MAX_LIMIT: i64 = 5000;

/// File types the browser can filter by, as the extensions they cover.
fn extensions(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "image" => &["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "heic", "avif", "svg", "raw", "dng"],
        "video" => &["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "mts"],
        "audio" => &["mp3", "flac", "wav", "m4a", "ogg", "opus", "aac"],
        "document" => &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "txt", "md", "rtf", "csv"],
        "archive" => &["zip", "7z", "rar", "tar", "gz", "bz2", "xz", "tgz"],
        _ => return None,
    })
}

#[derive(Deserialize)]
pub struct ListQuery {
    device: Option<String>,
    #[serde(default)]
    path: String,
    deleted: Option<u8>,
    q: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    sort: Option<String>,
    order: Option<String>,
    #[serde(default)]
    offset: i64,
    limit: Option<i64>,
}

#[derive(Serialize, Default)]
pub struct Entry {
    /// device, dir or file
    kind: &'static str,
    /// For a file: its id in the library (the address of its viewer, favorites and albums).
    id: Option<crate::platform::id::Id>,
    name: String,
    device: String,
    path: String,
    size: Option<i64>,
    /// Modification time on the source device, unix seconds.
    mtime: Option<i64>,
    /// When the master recorded the current version.
    changed: Option<i64>,
    /// Number of files, for folders and devices.
    files: Option<i64>,
    versions: Option<i64>,
    deleted_at: Option<i64>,
    purge_at: Option<i64>,
    mime: Option<String>,
    preview: bool,
    width: Option<i64>,
    height: Option<i64>,
    taken_at: Option<i64>,
    /// For a device entry: master, device, local or source.
    device_kind: Option<String>,
    /// For a photo or video: its details have not been read yet, so it is still being processed.
    processing: bool,
    /// For a device entry: whether the web app may add, change or delete files there.
    writable: bool,
}

#[derive(Serialize)]
pub struct Listing {
    device: Option<String>,
    /// What the device is (master, device, local or source) and whether the web app may change its files.
    device_kind: Option<String>,
    writable: bool,
    path: String,
    entries: Vec<Entry>,
    /// Files matching, before `offset` and `limit`.
    total: i64,
    searching: bool,
    retention_days: i64,
}

/// Whether the web app changes the files of a device of this kind: the master files, local folders and shared folders. What
/// a device or a source sends is theirs to change; here it can be read, downloaded and brought back from the trash.
pub fn editable_kind(kind: &str) -> bool {
    matches!(kind, "local" | "master" | "shared")
}

/// The device with this folder, if `who` may look at it: a device someone else owns does not exist for them.
pub fn find_device(c: &mut Client, who: &Access, folder: &str) -> anyhow::Result<Device> {
    who.check_see(folder)?;
    devices::by_folder(c, folder)?
        .map(|(id, name)| Device { id, name })
        .ok_or_else(|| ApiError::not_found("No such device").into())
}

/// The same, for a change: the user must be allowed to write there too.
pub fn find_device_to_change(c: &mut Client, who: &Access, folder: &str) -> anyhow::Result<Device> {
    who.check_write(folder)?;
    find_device(c, who, folder)
}

/// `a/b/` for `a/b`, and `""` for the root.
fn prefix_of(path: &str) -> Result<String, ApiError> {
    let path = path.trim_matches('/');
    if path.is_empty() {
        return Ok(String::new());
    }
    clean_remote_path(path).map(|p| p + "/").map_err(|e| ApiError::bad(e.to_string()))
}

/// What a request for a listing asks for, once it is checked.
struct View {
    /// `a/b/` for the folder `a/b`, and `""` for the root.
    prefix: String,
    /// The folder as the request named it, without slashes at the ends.
    path: String,
    sort: Sort,
    descending: bool,
    /// `%text%`: what to look for in paths.
    search: Option<String>,
    /// A regular expression for the extensions of a kind of file.
    type_regex: Option<String>,
    limit: i64,
    offset: i64,
    include_deleted: bool,
}

impl ListQuery {
    fn checked(&self) -> Result<View, ApiError> {
        let descending = match self.order.as_deref() {
            None | Some("asc") => false,
            Some("desc") => true,
            Some(other) => return Err(ApiError::bad(format!("order must be asc or desc, not {other}"))),
        };
        let sort = self.sort.as_deref().unwrap_or("name");
        let sort = Sort::parse(sort).ok_or_else(|| ApiError::bad(format!("cannot sort by {sort}")))?;
        let type_regex = match self.kind.as_deref().filter(|k| !k.is_empty() && *k != "all") {
            None => None,
            Some(k) => Some(format!(
                r"\.({})$",
                extensions(k).ok_or_else(|| ApiError::bad(format!("unknown type {k}")))?.join("|")
            )),
        };
        let search =
            self.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{}%", db::like_escape(s)));
        Ok(View {
            prefix: prefix_of(&self.path)?,
            path: self.path.trim_matches('/').to_string(),
            sort,
            descending,
            search,
            type_regex,
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            offset: self.offset.max(0),
            include_deleted: self.deleted == Some(1),
        })
    }
}

/// The top level: the master files, then one folder per device (an administrator's own and the others').
fn top_level(c: &mut Client, who: &Access, path: String, retention_days: i64) -> anyhow::Result<Listing> {
    let (master_files, master_bytes) = devices::master_totals(c)?;
    let mut entries = vec![Entry {
        kind: "device",
        name: MASTER_FOLDER.into(),
        device: MASTER_FOLDER.into(),
        files: Some(master_files),
        size: Some(master_bytes),
        device_kind: Some("master".into()),
        writable: who.can_write(MASTER_FOLDER),
        ..Default::default()
    }];
    entries.extend(device_summaries(c, who, None)?.into_iter().filter(|d| d.kind != "source" || d.files > 0).map(
        |d| Entry {
            kind: "device",
            name: if d.user == who.name { d.name.clone() } else { d.folder.clone() },
            writable: editable_kind(&d.kind) && who.can_write(&d.folder),
            device_kind: Some(d.kind),
            device: d.folder,
            files: Some(d.files),
            size: Some(d.bytes),
            ..Default::default()
        },
    ));
    let total = entries.len() as i64;
    Ok(Listing {
        device: None,
        device_kind: None,
        writable: false,
        path,
        entries,
        total,
        searching: false,
        retention_days,
    })
}

/// A file of a listing, named relative to the folder shown (`strip` bytes of its path are the folder).
fn file_entry(f: Listed, strip: usize, device: &str, retention_days: i64) -> Entry {
    let full = f.path;
    Entry {
        kind: "file",
        id: Some(crate::platform::id::Id(f.id)),
        name: full[strip..].to_string(),
        device: device.to_string(),
        mtime: Some(secs_of_ns(f.mtime)),
        size: Some(f.size),
        changed: Some(f.created_at),
        deleted_at: f.deleted_at,
        purge_at: f.deleted_at.map(|t| t + retention_days * DAY_SECS),
        processing: f.deleted_at.is_none() && meta::wants(&full) && f.mime.is_none(),
        preview: meta::can_preview(&full),
        mime: Some(f.mime.unwrap_or_else(|| meta::mime(&full))),
        width: f.width,
        height: f.height,
        taken_at: f.taken_at,
        versions: Some(f.versions),
        path: full,
        ..Default::default()
    }
}

/// The inside of a device: either what a search or a type filter finds below the folder, or the folder's own folders
/// (on the first page only) and files.
fn inside_device(
    c: &mut Client,
    who: &Access,
    device_name: String,
    view: View,
    retention_days: i64,
) -> anyhow::Result<Listing> {
    let dev = find_device(c, who, &device_name)?;
    let kind = devices::kind_of(c, dev.id)?;
    let writable = editable_kind(&kind) && who.can_write(&device_name);
    // SQL counts characters, not bytes.
    let names_start = view.prefix.chars().count() as i32 + 1;
    let below = db::like_prefix(&view.prefix);
    let page = Page {
        device_id: dev.id,
        below: &below,
        include_deleted: view.include_deleted,
        sort: view.sort,
        descending: view.descending,
        limit: view.limit,
        offset: view.offset,
    };
    let searching = view.search.is_some() || view.type_regex.is_some();
    let mut entries = Vec::new();
    let found = if searching {
        listing::search(c, &page, view.search.as_deref(), view.type_regex.as_deref())?
    } else {
        if view.offset == 0 {
            let folders = files::folders_below(c, dev.id, &below, view.include_deleted, names_start)?;
            entries.extend(folders.into_iter().map(|f| Entry {
                kind: "dir",
                path: format!("{}{}", view.prefix, f.name),
                device: device_name.clone(),
                files: Some(f.files),
                size: Some(f.bytes),
                changed: f.changed,
                deleted_at: f.deleted_at,
                name: f.name,
                ..Default::default()
            }));
        }
        listing::files_in(c, &page, names_start)?
    };
    let total = found.total;
    let strip = view.prefix.len();
    entries.extend(found.files.into_iter().map(|f| file_entry(f, strip, &device_name, retention_days)));
    Ok(Listing {
        device: Some(device_name),
        device_kind: Some(kind),
        writable,
        path: view.path,
        entries,
        total,
        searching,
        retention_days,
    })
}

pub async fn list(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Query(q): Query<ListQuery>,
) -> Res<Listing> {
    let view = q.checked()?;
    let device = q.device;
    let listing = run(&state, move |c| {
        let retention_days = settings::get_i64(c, "retention_days", 30);
        match device {
            None => top_level(c, &who, view.path, retention_days),
            Some(device) => inside_device(c, &who, device, view, retention_days),
        }
    })
    .await?;
    Ok(Json(listing))
}

#[derive(Deserialize)]
pub struct FileRef {
    pub device: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct VersionInfo {
    id: i64,
    created_at: i64,
    size: i64,
    mtime: i64,
    hash: String,
    current: bool,
    pruned_at: Option<i64>,
    /// Whether the content of this version is still on disk.
    available: bool,
}

#[derive(Serialize)]
pub struct Info {
    /// The id of the file, which is also its address for streaming (`/api/stream/<id>/...`).
    id: crate::platform::id::Id,
    device: String,
    path: String,
    name: String,
    deleted_at: Option<i64>,
    purge_at: Option<i64>,
    size: i64,
    mtime: i64,
    changed: i64,
    hash: String,
    mime: String,
    preview: bool,
    /// Work still to do on a photo or video: waiting, running or failed.
    work: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    taken_at: Option<i64>,
    exif: serde_json::Value,
    versions: Vec<VersionInfo>,
    events: Vec<Recent>,
}

/// Where the content of a version is on disk: the live file for the current version of a live file,
/// otherwise the folder of the version.
pub fn version_file(
    store: &crate::platform::store::Store,
    dev: &str,
    path: &str,
    version: i64,
    current: Option<i64>,
    deleted: bool,
) -> anyhow::Result<PathBuf> {
    if Some(version) == current && !deleted {
        store.live_path(dev, path)
    } else {
        store.version_path(version, dev, path)
    }
}

pub async fn info(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Query(r): Query<FileRef>,
) -> Res<Info> {
    let store = state.store.clone();
    let out = run(&state, move |c| {
        let dev = find_device(c, &who, &r.device)?;
        let path = clean_remote_path(&r.path).map_err(|e| ApiError::bad(e.to_string()))?;
        let retention = settings::get_i64(c, "retention_days", 30);
        let row = files::info(c, dev.id, &path)?.ok_or_else(|| ApiError::not_found("No such file"))?;
        let (file_id, current, deleted_at) = (row.id, row.current_version_id, row.deleted_at);
        let versions = files::versions(c, file_id)?
            .into_iter()
            .map(|v| {
                let available = version_file(&store, &dev.name, &path, v.id, current, deleted_at.is_some())
                    .is_ok_and(|p| p.is_file());
                VersionInfo {
                    id: v.id,
                    created_at: v.created_at,
                    size: v.size,
                    mtime: secs_of_ns(v.mtime),
                    hash: hex::encode(v.root_hash),
                    current: Some(v.id) == current,
                    pruned_at: v.pruned_at,
                    available,
                }
            })
            .collect();
        let events = events::of_file(c, dev.id, &path, 30)?.into_iter().map(Recent::from).collect();
        let work = if deleted_at.is_none() && meta::wants(&path) {
            crate::library::photos::work_state(c, &dev.name, &path, row.mime.is_none())?
        } else {
            None
        };
        let exif = row.exif;
        Ok(Info {
            id: crate::platform::id::Id(file_id),
            device: dev.name,
            name: base_name(&path).to_string(),
            deleted_at,
            purge_at: deleted_at.map(|t| t + retention * DAY_SECS),
            size: row.size,
            mtime: secs_of_ns(row.mtime),
            changed: row.created_at,
            hash: hex::encode(row.root_hash),
            mime: row.mime.unwrap_or_else(|| meta::mime(&path)),
            preview: meta::can_preview(&path),
            work,
            width: row.width,
            height: row.height,
            taken_at: row.taken_at,
            exif: exif.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(serde_json::Value::Null),
            versions,
            events,
            path,
        })
    })
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct Items {
    items: Vec<FileRef>,
}

#[derive(Serialize)]
pub struct Removed {
    deleted: usize,
}

/// The live paths a selection stands for: a file, or every file under a folder.
pub fn expand(c: &mut Client, dev: &Device, path: &str, include_deleted: bool) -> anyhow::Result<Vec<String>> {
    let path = path.trim_matches('/');
    Ok(files::expand(c, dev.id, path, include_deleted)?)
}

#[derive(Serialize)]
pub struct Rescan {
    /// False when a comparison was already running.
    started: bool,
}

/// Compares the file store with the database now instead of at the next round (administrators). Files put on the disk
/// by hand, or over a network share, are picked up either way; this is for when one does not want to wait.
pub async fn rescan(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Rescan> {
    who.require_admin()?;
    Ok(Json(Rescan { started: crate::files::livetree::rescan(state.db.clone(), state.store.clone()) }))
}

pub async fn remove(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(body): Json<Items>,
) -> Res<Removed> {
    let store = state.store.clone();
    let deleted = run(&state, move |c| {
        let mut deleted = 0;
        for item in &body.items {
            let dev = find_device_to_change(c, &who, &item.device)?;
            for path in expand(c, &dev, &item.path, false)? {
                if ingest::mark_deleted(c, &store, &dev, &path, true)? {
                    deleted += 1;
                }
            }
        }
        db::notify(c, "files", "")?;
        Ok(deleted)
    })
    .await?;
    Ok(Json(Removed { deleted }))
}

#[derive(Deserialize)]
pub struct Restore {
    device: String,
    path: String,
    version: i64,
}

pub async fn restore(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(r): Json<Restore>,
) -> Res<Done> {
    let store = state.store.clone();
    run(&state, move |c| {
        let dev = find_device_to_change(c, &who, &r.device)?;
        let path = clean_remote_path(&r.path).map_err(|e| ApiError::bad(e.to_string()))?;
        let row = files::version_to_restore(c, dev.id, &path, r.version)?
            .ok_or_else(|| ApiError::not_found("No such version"))?;
        let (current, deleted_at) = (row.current_version_id, row.deleted_at);
        if current == Some(r.version) && deleted_at.is_none() {
            return Err(ApiError::conflict("That is already the current version").into());
        }
        // What a device or a source sends is theirs to change: here it can be brought back from the trash, not edited.
        let kind = devices::kind_of(c, dev.id)?;
        if !editable_kind(&kind) && deleted_at.is_none() {
            return Err(ApiError::conflict(
                "Files sent by a device cannot be changed here. Download the version instead.",
            )
            .into());
        }
        let from = version_file(&store, &dev.name, &path, r.version, current, deleted_at.is_some())?;
        if !from.is_file() {
            return Err(ApiError::not_found("The content of that version is no longer stored").into());
        }
        let staged = store.temp_path();
        std::fs::copy(&from, &staged)?;
        let root = row.root_hash;
        ingest::commit_version(
            c,
            &store,
            &staged,
            &ingest::NewVersion {
                device: &dev,
                path: &path,
                root: root.try_into().map_err(|_| anyhow::anyhow!("stored hash has the wrong length"))?,
                size: row.size as u64,
                mtime: row.mtime,
                mode: row.mode as u32,
                renamed_from: None,
            },
        )?;
        Ok(())
    })
    .await?;
    done()
}

#[derive(Deserialize)]
pub struct MoveBody {
    device: String,
    /// A file or a folder of the device.
    from: String,
    /// Its new path on the same device.
    to: String,
}

#[derive(Serialize)]
pub struct Moved {
    moved: usize,
}

/// Renames or moves a file, or a folder with everything below it, within a device the master owns: a local folder or the
/// master files. What a device or a source sent is theirs to move, so it answers 409 there.
pub async fn move_files(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(b): Json<MoveBody>,
) -> Res<Moved> {
    let store = state.store.clone();
    let moved = run(&state, move |c| {
        let dev = find_device_to_change(c, &who, &b.device)?;
        if !editable_kind(&devices::kind_of(c, dev.id)?) {
            return Err(ApiError::conflict("Files sent by a device or a source cannot be moved here").into());
        }
        let clean = |p: &str| clean_remote_path(p.trim_matches('/')).map_err(|e| ApiError::bad(e.to_string()));
        let (from, to) = (clean(&b.from)?, clean(&b.to)?);
        if to == from || to.starts_with(&format!("{from}/")) {
            return Err(ApiError::bad("Choose a different place, not inside the folder itself").into());
        }
        let paths = expand(c, &dev, &from, false)?;
        if paths.is_empty() {
            return Err(ApiError::not_found("No such file or folder").into());
        }
        let targets: Vec<(String, String)> =
            paths.into_iter().map(|p| (format!("{to}{}", &p[from.len()..]), p)).collect();
        // Nothing moves if something is in the way.
        for (new, _) in &targets {
            let parents = new.match_indices('/').map(|(i, _)| &new[..i]);
            if parents.chain([new.as_str()]).any(|p| files::live_content(c, dev.id, p).ok().flatten().is_some()) {
                return Err(ApiError::conflict(format!("{new} exists already")).into());
            }
        }
        for (new, old) in &targets {
            ingest::move_path(c, &store, &dev, old, new)?;
        }
        db::notify(c, "files", &dev.name)?;
        Ok(targets.len())
    })
    .await?;
    Ok(Json(Moved { moved }))
}
