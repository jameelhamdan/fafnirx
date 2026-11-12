//! Deleted files: the trash a person can look through and empty, and the snapshot of the library a mirror keeps.

use super::short_hash;
use super::TRASH_PAGE;
use crate::library::meta;
use crate::platform::db;
use crate::platform::id::uuids;
use crate::platform::id::Id;
use crate::repo::library;
use crate::repo::settings;
use crate::repo::snapshot;
use crate::web::access::Ctx;
use crate::web::run;
use crate::web::ApiError;
use crate::web::Res;
use axum::extract::Query;
use axum::Json;
use fafnirx_common::base_name;
use fafnirx_common::unix_now;
use fafnirx_common::DAY_SECS;
use fafnirx_db::queries as sql;
use postgres::Client;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct TrashEntry {
    id: Id,
    folder: String,
    path: String,
    name: String,
    size: i64,
    deleted_at: i64,
    purge_at: i64,
    /// The version to restore: the last one the file had.
    version: i64,
    #[cfg_attr(test, ts(type = "'image' | 'video' | null"))]
    kind: Option<&'static str>,
    hash: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Trash {
    entries: Vec<TrashEntry>,
    total: i64,
    retention_days: i64,
}

#[derive(Deserialize)]
pub struct TrashQuery {
    #[serde(default)]
    offset: i64,
}

/// Deleted files of the folders the user can see, newest deletion first. They are removed for good when the retention
/// period ends; restoring one is `POST /api/files/restore` with its `version`.
pub async fn trash(Ctx { state, who }: Ctx, Query(q): Query<TrashQuery>) -> Res<Trash> {
    let out = run(&state, move |c| {
        let retention_days = settings::retention_days(c);
        let rows = library::trash(c, who.viewer(), TRASH_PAGE, q.offset.max(0))?;
        let total = rows.first().map_or(0, |r| r.total);
        let entries = rows
            .into_iter()
            .map(|r| TrashEntry {
                id: Id(r.id),
                name: base_name(&r.path).to_string(),
                folder: r.folder,
                size: r.size,
                deleted_at: r.deleted_at,
                purge_at: r.deleted_at + retention_days * DAY_SECS,
                version: r.version_id,
                kind: meta::kind(&r.path),
                hash: short_hash(&r.root_hash),
                path: r.path,
            })
            .collect();
        Ok(Trash { entries, total, retention_days })
    })
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct PurgeBody {
    /// The ids the trash lists.
    ids: Vec<Id>,
}

#[derive(Serialize)]
pub struct Purged {
    files: usize,
    /// What the versions of those files took on the disk.
    bytes: i64,
}

/// Removes files from the trash for good, with their versions, albums, tags and state, before the retention period ends.
/// Only what the trash lists, and only where the user may change files.
pub async fn purge(Ctx { state, who }: Ctx, Json(b): Json<PurgeBody>) -> Res<Purged> {
    if b.ids.is_empty() || b.ids.len() > TRASH_PAGE as usize {
        return Err(ApiError::bad("Name between 1 and 500 files"));
    }
    let store = state.store.clone();
    let (out, versions) = run(&state, move |c| {
        let mut tx = c.transaction()?;
        let files = library::purgeable(&mut tx, who.viewer(), &uuids(&b.ids))?;
        // Asking for something that is not in the trash (or was restored meanwhile) is an error, not a partial success.
        if files.len() != b.ids.len() {
            return Err(ApiError::not_found("Some of those files are no longer in the trash").into());
        }
        let folders: std::collections::BTreeSet<_> = files.iter().map(|f| f.folder.clone()).collect();
        for folder in &folders {
            who.check_write(folder)?;
        }
        let ids: Vec<_> = files.iter().map(|f| f.id).collect();
        let folder_ids: Vec<_> = files.iter().map(|f| f.folder_id).collect();
        let bytes = library::versions_size(&mut tx, &ids)?;
        let versions = crate::repo::retention::delete_versions_of(&mut tx, &ids)?;
        crate::repo::retention::delete_files(&mut tx, &ids)?;
        // A device that has not heard of these deletions must not send the files again: the deletions removed here are no
        // longer all known, so it is given the whole list (the same rule as when retention removes them).
        let newest = files.iter().map(|f| f.deleted_at).max().unwrap_or(0) + 1;
        let known_from = settings::get_i64(&mut tx, settings::DELETIONS_KNOWN_FROM, 0).max(newest);
        settings::set(&mut tx, settings::DELETIONS_KNOWN_FROM, &known_from.to_string())?;
        sql::folders::mark_totals_stale().bind(&mut tx, &folder_ids).map(drop)?;
        for folder in &folders {
            db::notify(&mut tx, db::Topic::Files, folder)?;
        }
        db::notify(&mut tx, db::Topic::Folders, "")?;
        tx.commit()?;
        Ok((Purged { files: ids.len(), bytes }, versions))
    })
    .await?;
    for id in versions {
        store.remove_version(id);
    }
    Ok(Json(out))
}

/// The tables a mirror keeps a copy of, so that albums, tags and favorites survive losing the database. Everything is
/// named by user, folder and path rather than by id, because ids do not survive a restore.
pub fn snapshot(c: &mut Client) -> anyhow::Result<serde_json::Value> {
    use serde_json::json;
    use std::collections::BTreeMap;

    /// The files of each tag, by the id of the tag.
    fn files_by<'a>(rows: impl Iterator<Item = (uuid::Uuid, &'a str, &'a str)>) -> BTreeMap<uuid::Uuid, Vec<serde_json::Value>> {
        let mut map: BTreeMap<uuid::Uuid, Vec<serde_json::Value>> = BTreeMap::new();
        for (id, folder, path) in rows {
            map.entry(id).or_default().push(json!({ "folder": folder, "path": path }));
        }
        map
    }

    let albums: Vec<_> = sql::library::snapshot_albums()
        .bind(c)
        .all()?
        .into_iter()
        .map(|a| {
            json!({ "user": a.owner, "name": a.name, "description": a.description, "created_at": a.created_at,
                    "folder": { "folder": a.folder, "path": "" } })
        })
        .collect();

    let tag_files = sql::library::snapshot_tag_files().bind(c).all()?;
    let mut files_of_tag = files_by(tag_files.iter().map(|f| (f.tag_id, f.folder.as_str(), f.path.as_str())));
    let tags: Vec<_> = snapshot::tags(c)?
        .into_iter()
        .map(|t| json!({ "user": t.owner, "name": t.name, "files": files_of_tag.remove(&t.id).unwrap_or_default() }))
        .collect();

    let states: Vec<_> = sql::library::snapshot_states()
        .bind(c)
        .all()?
        .into_iter()
        .map(|s| {
            json!({ "user": s.owner, "folder": s.folder, "path": s.path, "favorite": s.favorite, "archived": s.archived,
                    "rating": s.rating, "description": s.description, "taken_at": s.taken_at, "lat": s.lat, "lon": s.lon })
        })
        .collect();

    Ok(json!({ "generated_at_unix": unix_now(), "albums": albums, "tags": tags, "files": states }))
}
