//! The file browser: folders and files of the folders a user can reach (and the master files), search, details, history,
//! delete and restore. A folder is named by its folder (`<user>/<folder>`; `master:<name>` for a master folder).

use crate::library::meta;
use crate::monitoring::api::Recent;
use crate::platform::db::{self, FolderRef};
use crate::repo::folders::{self, FolderKind};
use crate::repo::listing::{self, Listed, Page, Sort};
use crate::repo::{events, files, settings};
use crate::web::access::{Access, Ctx};
use crate::web::input;
use crate::web::{run, ApiError, Res};
use axum::extract::Query;
use axum::Json;
use fafnirx_common::{base_name, clean_remote_path, secs_of_ns, DAY_SECS};
use fafnirx_db::queries as sql;
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
    folder: Option<String>,
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
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Entry {
    /// folder, dir or file
    #[cfg_attr(test, ts(type = "'folder' | 'dir' | 'file'"))]
    kind: &'static str,
    /// For a file: its id in the library (the address of its viewer, favorites and albums).
    id: Option<crate::platform::id::Id>,
    name: String,
    folder: String,
    path: String,
    size: Option<i64>,
    /// Modification time on the source folder, unix seconds.
    mtime: Option<i64>,
    /// When the master recorded the current version.
    changed: Option<i64>,
    /// Number of files, for folders and directories.
    files: Option<i64>,
    versions: Option<i64>,
    deleted_at: Option<i64>,
    purge_at: Option<i64>,
    mime: Option<String>,
    preview: bool,
    width: Option<i64>,
    height: Option<i64>,
    taken_at: Option<i64>,
    /// For a folder entry: what the folder is.
    #[cfg_attr(test, ts(type = "'backup' | 'user' | 'master' | null"))]
    folder_kind: Option<FolderKind>,
    /// For a photo or video: its details have not been read yet, so it is still being processed.
    processing: bool,
    /// For a folder entry: whether the web app may add, change or delete files there.
    writable: bool,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Listing {
    folder: Option<String>,
    /// What the folder is and whether the web app may change its files.
    #[cfg_attr(test, ts(type = "'backup' | 'user' | 'master' | null"))]
    folder_kind: Option<FolderKind>,
    writable: bool,
    path: String,
    entries: Vec<Entry>,
    /// Files matching, before `offset` and `limit`.
    total: i64,
    searching: bool,
    retention_days: i64,
}

/// The folder with this key, if `who` may look at it: a folder someone else owns does not exist for them.
pub fn find_folder(c: &mut Client, who: &Access, folder: &str) -> anyhow::Result<FolderRef> {
    who.check_see(folder)?;
    folders::id_of(c, folder)?
        .map(|id| FolderRef { id, key: folder.to_string() })
        .ok_or_else(|| ApiError::not_found("No such folder").into())
}

/// The same, for a change: the user must be allowed to write there too.
pub fn find_folder_to_change(c: &mut Client, who: &Access, folder: &str) -> anyhow::Result<FolderRef> {
    who.check_write(folder)?;
    find_folder(c, who, folder)
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
        let search = self
            .q
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("%{}%", db::like_escape(s)));
        Ok(View {
            prefix: input::folder_prefix(&self.path)?,
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

/// The top level: every folder the user can see (the master folders, their own, those shared with them and, for an administrator, the others').
fn top_level(c: &mut Client, who: &Access, path: String, retention_days: i64) -> anyhow::Result<Listing> {
    let (all, user) = who.scope_args();
    let entries: Vec<Entry> = folders::visible(c, all, user)?
        .into_iter()
        .map(|(key, kind, files, bytes)| Entry {
            kind: "folder",
            // Named by its key (`user/name`), as it is when opened.
            name: key.clone(),
            writable: kind.editable() && who.can_write(&key),
            folder_kind: Some(kind),
            folder: key,
            files: Some(files),
            size: Some(bytes),
            ..Default::default()
        })
        .collect();
    let total = entries.len() as i64;
    Ok(Listing {
        folder: None,
        folder_kind: None,
        writable: false,
        path,
        entries,
        total,
        searching: false,
        retention_days,
    })
}

/// A file of a listing, named relative to the folder shown (`strip` bytes of its path are the folder).
fn file_entry(f: Listed, strip: usize, folder: &str, retention_days: i64) -> Entry {
    let full = f.path;
    Entry {
        kind: "file",
        id: Some(crate::platform::id::Id(f.id)),
        name: full[strip..].to_string(),
        folder: folder.to_string(),
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

/// The inside of a folder: either what a search or a type filter finds below the folder, or the folder's own folders
/// (on the first page only) and files.
fn inside_folder(c: &mut Client, who: &Access, key: String, view: View, retention_days: i64) -> anyhow::Result<Listing> {
    let folder = find_folder(c, who, &key)?;
    let kind = folders::kind_of(c, folder.id)?;
    let writable = kind.editable() && who.can_write(&key);
    // SQL counts characters, not bytes.
    let names_start = view.prefix.chars().count() as i32 + 1;
    let below = db::like_prefix(&view.prefix);
    let page = Page {
        folder_id: folder.id,
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
            let folders = files::folders_below(c, folder.id, &below, view.include_deleted, names_start)?;
            entries.extend(folders.into_iter().map(|f| Entry {
                kind: "dir",
                path: format!("{}{}", view.prefix, f.name),
                folder: key.clone(),
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
    entries.extend(found.files.into_iter().map(|f| file_entry(f, strip, &key, retention_days)));
    Ok(Listing {
        folder: Some(key),
        folder_kind: Some(kind),
        writable,
        path: view.path,
        entries,
        total,
        searching,
        retention_days,
    })
}

pub async fn list(Ctx { state, who }: Ctx, Query(q): Query<ListQuery>) -> Res<Listing> {
    let view = q.checked()?;
    let folder = q.folder;
    let listing = run(&state, move |c| {
        let retention_days = settings::retention_days(c);
        match folder {
            None => top_level(c, &who, view.path, retention_days),
            Some(folder) => inside_folder(c, &who, folder, view, retention_days),
        }
    })
    .await?;
    Ok(Json(listing))
}

#[derive(Deserialize)]
pub struct FileRef {
    pub folder: String,
    pub path: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "FileInfo"))]
pub struct Info {
    /// The id of the file, which is also its address for the modules (`/api/m/<module>/<id>/...`).
    id: crate::platform::id::Id,
    folder: String,
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
    #[cfg_attr(test, ts(type = "'waiting' | 'running' | 'failed' | null"))]
    work: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    taken_at: Option<i64>,
    #[cfg_attr(test, ts(type = "Record<string, string> | null"))]
    exif: serde_json::Value,
    versions: Vec<VersionInfo>,
    events: Vec<Recent>,
}

/// Where the content of a version is on disk: the live file for the current version of a live file,
/// otherwise the folder of the version.
pub fn version_file(store: &crate::platform::store::Store, v: &VersionAt) -> anyhow::Result<PathBuf> {
    if Some(v.version) == v.current && !v.deleted {
        store.live_path(v.folder, v.path)
    } else {
        store.version_path(v.version, v.folder, v.path)
    }
}

/// A version of a file, and whether it is the live one: the current version of a path that is not deleted.
pub struct VersionAt<'a> {
    pub folder: &'a str,
    pub path: &'a str,
    pub version: i64,
    pub current: Option<i64>,
    pub deleted: bool,
}

pub async fn info(Ctx { state, who }: Ctx, Query(r): Query<FileRef>) -> Res<Info> {
    let store = state.store.clone();
    let out = run(&state, move |c| {
        let folder = find_folder(c, &who, &r.folder)?;
        let path = clean_remote_path(&r.path).map_err(|e| ApiError::bad(e.to_string()))?;
        let retention = settings::retention_days(c);
        let row = sql::files::file_info()
            .bind(c, &folder.id, &path)
            .opt()?
            .ok_or_else(|| ApiError::not_found("No such file"))?;
        let (file_id, current, deleted_at) = (row.id, row.current_version_id, row.deleted_at);
        let versions = files::versions(c, file_id)?
            .into_iter()
            .map(|v| {
                let available = version_file(
                    &store,
                    &VersionAt { folder: &folder.key, path: &path, version: v.id, current, deleted: deleted_at.is_some() },
                )
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
        let events = events::of_file(c, folder.id, &path, 30)?.into_iter().map(Recent::from).collect();
        let work = if deleted_at.is_none() && meta::wants(&path) {
            crate::library::photos::work_state(c, &folder.key, &path, row.mime.is_none())?
        } else {
            None
        };
        let exif = row.exif;
        Ok(Info {
            id: crate::platform::id::Id(file_id),
            folder: folder.key,
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
