//! Recording files in the database and the file store. Shared by the gRPC Commit/MarkDeleted calls
//! (folders), by the web API (uploads, deletes), by sources that the master collects itself and by
//! the file watcher. The content must already be complete in a temporary file.
//!
//! Everything that changes one path takes that path's advisory lock first (see [`db::lock_path`]) and
//! moves files before the lock is released, so the database and the file store never disagree for long
//! and no two processes work on the same path at once.

use crate::platform::db::{self, FolderRef};
use crate::platform::store::{Attrs, Store};
use crate::repo::events::{self, NewEvent};
use crate::repo::folders::{self, FolderKind};
use crate::repo::jobs::{self, JobKind};
use crate::repo::{files, settings};
use anyhow::Result;
use fafnirx_common::{unix_now, HASH_LEN};
use fafnirx_db::client::GenericClient;
use fafnirx_db::queries as sql;
use postgres::Client;
use std::path::Path;

pub struct NewVersion<'a> {
    pub folder: &'a FolderRef,
    pub path: &'a str,
    pub root: [u8; HASH_LEN],
    pub size: u64,
    /// Unix nanoseconds.
    pub mtime: i64,
    pub mode: u32,
    /// Set when this path is a rename of another path whose content is identical.
    pub renamed_from: Option<&'a str>,
}

/// A file found in the live tree: where, what it holds, and the path it was moved from if it is a rename.
#[derive(Clone, Copy)]
pub struct Found<'a> {
    pub path: &'a str,
    pub seen: &'a Seen,
    pub renamed_from: Option<&'a str>,
}

/// What was found on disk for a file of the live tree.
pub struct Seen {
    pub root: [u8; HASH_LEN],
    pub size: u64,
    /// Unix nanoseconds.
    pub mtime: i64,
    pub mode: u32,
}

/// Keeps the newest `version_count` versions of a file; older ones are marked pruned and removed by retention.
fn prune_versions(conn: &mut impl GenericClient, file_id: uuid::Uuid, ts: i64) -> Result<(), postgres::Error> {
    let keep = settings::version_count(conn).max(1);
    sql::files::prune_versions().bind(conn, &ts, &file_id, &keep).map(drop)
}

/// Makes a version the current one of its file (undeleting it) and prunes the ones that are too old.
fn make_current(conn: &mut impl GenericClient, file_id: uuid::Uuid, version_id: i64, ts: i64) -> Result<(), postgres::Error> {
    files::make_current(conn, file_id, version_id)?;
    prune_versions(conn, file_id, ts)
}

/// Appends to the change log shown in the web UI.
fn log_event(conn: &mut impl GenericClient, folder: &FolderRef, event: &NewEvent) -> Result<(), postgres::Error> {
    events::record(conn, folder.id, event)?;
    db::notify(conn, db::Topic::Files, &folder.key)?;
    db::notify(conn, db::Topic::Activity, &folder.key)
}

/// Queues reading the kind, size and EXIF data of a new content, once per content.
fn queue_meta(conn: &mut impl GenericClient, folder: &FolderRef, path: &str, root: &[u8]) -> Result<(), postgres::Error> {
    if !crate::library::meta::wants(path) {
        return Ok(());
    }
    if !files::content_known(conn, root)? {
        jobs::enqueue(
            conn,
            JobKind::Meta,
            &serde_json::json!({ "folder": folder.key, "path": path, "root": hex::encode(root) }),
        )?;
    }
    Ok(())
}

/// Adds a version of a file and returns its id. `staged` is the complete content in a temporary file;
/// it is moved into the live tree (the file it replaces moves to the folder of its version) and is
/// gone afterwards either way. Committing the content a file already has is a no-op that returns the
/// current version.
pub fn commit_version(conn: &mut Client, store: &Store, staged: &Path, v: &NewVersion) -> Result<u64> {
    let result = commit_inner(conn, store, staged, v);
    let _ = std::fs::remove_file(staged);
    result
}

/// Rules of the most limited device: no name Windows cannot keep, no two paths (or folders above them) differing only in
/// case. They apply to what people add in the web app and to what devices sync into a folder (`strict`), in a folder of a user or a
/// master folder. The watcher and the backups of devices bypass them. An existing path can always be updated; a rename's source does not
/// count (case-only renames work).
pub fn check_names(conn: &mut impl GenericClient, folder: &FolderRef, path: &str, renamed_from: Option<&str>, strict: bool) -> Result<()> {
    let kind = folders::kind_of(conn, folder.id)?;
    let in_ = match kind {
        FolderKind::Master if strict => "the master files",
        FolderKind::User if strict => "this folder",
        _ => return Ok(()),
    };
    let conflict = |what: String| -> Result<()> { Err(crate::web::ApiError::conflict(what).into()) };
    if let Some(why) = fafnirx_common::not_portable(path) {
        return Err(crate::web::ApiError::bad(format!("Windows could not keep that: {why}")).into());
    }
    if let Some(other) = files::case_clash(conn, folder.id, path, renamed_from)? {
        return conflict(format!("{other} is in {in_} already, and {path} differs from it only by case"));
    }
    let parts: Vec<&str> = path.split('/').collect();
    for depth in 1..parts.len() {
        let ancestor = parts[..depth].join("/");
        // The spelling of this folder that the file is being renamed from is on its way out.
        let leaving = renamed_from.map(|from| from.split('/').take(depth).collect::<Vec<_>>().join("/"));
        for spelling in sql::files::folder_spellings().bind(conn, &ancestor, &folder.id).all()? {
            if Some(&spelling) != leaving.as_ref() {
                return conflict(format!("the folder {spelling} is in {in_} already, and {ancestor} differs from it only by case"));
            }
        }
    }
    Ok(())
}

fn commit_inner(conn: &mut Client, store: &Store, staged: &Path, v: &NewVersion) -> Result<u64> {
    let ts = unix_now();
    let root: &[u8] = &v.root;
    let folder = v.folder;
    check_names(conn, folder, v.path, v.renamed_from, false)?;
    let mut tx = conn.transaction()?;
    // Both paths of a rename, in a fixed order so that two renames cannot wait for each other.
    let mut locked = vec![v.path];
    locked.extend(v.renamed_from);
    locked.sort_unstable();
    for path in locked {
        db::lock_path(&mut tx, folder.id, path)?;
    }
    let existing = files::lock_row(&mut tx, folder.id, v.path)?;
    let live = existing.as_ref().and_then(files::Row::live_version);
    if let Some(cur) = live {
        if files::has_content(&mut tx, cur, root)? {
            return Ok(cur as u64);
        }
    }
    let file_id = match &existing {
        Some(row) => row.id,
        None => files::ensure(&mut tx, folder.id, v.path)?,
    };
    let version_id = files::insert_version(
        &mut tx,
        &files::NewVersion { file_id, root, size: v.size, mtime: v.mtime, mode: v.mode, created_at: ts },
    )?;

    // The file store is changed before the commit and put back if anything fails after that.
    let replaced = match live {
        Some(cur) if store.archive(&folder.key, v.path, cur)? => Some(cur),
        _ => None,
    };
    let undo = |store: &Store| {
        store.discard_live(&folder.key, v.path);
        if let Some(cur) = replaced {
            let _ = store.unarchive(&folder.key, v.path, cur);
        }
    };
    if let Err(e) = store.install(&folder.key, v.path, staged, Attrs { mtime_ns: v.mtime, mode: v.mode }) {
        undo(store);
        return Err(e);
    }
    let recorded = (|| -> Result<Option<(&str, i64)>> {
        make_current(&mut tx, file_id, version_id, ts)?;
        let (action, detail) = match v.renamed_from {
            Some(old) => ("rename", Some(old)),
            None if live.is_some() => ("update", None),
            None => ("add", None),
        };
        log_event(&mut tx, folder, &NewEvent { path: v.path, action, ts, hash: Some(&hex::encode(v.root)), detail })?;
        queue_meta(&mut tx, folder, v.path, root)?;
        // The old path is vacated, not deleted: the log has the one `rename` event, and the trash does not get the file.
        let vacated = match v.renamed_from {
            Some(old) => files::vacate_renamed(&mut tx, folder.id, old, ts)?.map(|id| (old, id)),
            None => None,
        };
        folders::touch_collected(&mut tx, folder.id, ts)?;
        db::notify(&mut tx, db::Topic::Folders, &folder.key)?;
        Ok(vacated)
    })();
    let vacated = match recorded {
        Ok(vacated) => vacated,
        Err(e) => {
            undo(store);
            return Err(e);
        }
    };
    // The old path of a rename leaves the live tree while this path is still locked.
    let archived_old = vacated.filter(|(old, id)| archive_deleted(store, folder, old, *id));
    if let Err(e) = tx.commit() {
        undo(store);
        if let Some((old, id)) = archived_old {
            let _ = store.unarchive(&folder.key, old, id);
        }
        return Err(e.into());
    }
    Ok(version_id as u64)
}

/// Moves or renames a live file of a folder the master owns (a local folder or the master files): the file in the store is
/// moved, not copied, and recorded as a rename of the same content, with the old path vacated like any rename. Folders and
/// sources are not moved here: their files are theirs to move. The caller checks that the folder is changeable.
pub fn move_path(conn: &mut Client, store: &Store, folder: &FolderRef, from: &str, to: &str) -> Result<()> {
    check_names(conn, folder, to, Some(from), true)?;
    let mut tx = conn.transaction()?;
    let mut locked = [from, to];
    locked.sort_unstable();
    for path in locked {
        db::lock_path(&mut tx, folder.id, path)?;
    }
    let version = files::lock_row(&mut tx, folder.id, from)?
        .and_then(|r| r.live_version())
        .ok_or_else(|| crate::web::ApiError::not_found("No such file"))?;
    let target = files::lock_row(&mut tx, folder.id, to)?;
    if target.as_ref().is_some_and(|r| r.live_version().is_some()) {
        return Err(crate::web::ApiError::conflict(format!("{to} exists already")).into());
    }
    let record = files::version_record(&mut tx, version)?.ok_or_else(|| crate::web::ApiError::not_found("No such file"))?;
    let (root, size, mtime, mode) = (record.root_hash, record.size, record.mtime, record.mode);
    let (src, dst) = (store.live_path(&folder.key, from)?, store.live_path(&folder.key, to)?);
    if dst.exists() {
        return Err(crate::web::ApiError::conflict(format!("{to} exists already")).into());
    }
    if !src.is_file() {
        return Err(crate::web::ApiError::conflict(format!("{from} is not in the file store")).into());
    }
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::rename(&src, &dst)?;

    let ts = unix_now();
    let recorded = (|| -> Result<()> {
        let file_id = match &target {
            Some(row) => row.id,
            None => files::ensure(&mut tx, folder.id, to)?,
        };
        let new = files::insert_version(
            &mut tx,
            &files::NewVersion { file_id, root: &root, size: size as u64, mtime, mode: mode as u32, created_at: ts },
        )?;
        make_current(&mut tx, file_id, new, ts)?;
        log_event(
            &mut tx,
            folder,
            &NewEvent { path: to, action: "rename", ts, hash: Some(&hex::encode(&root)), detail: Some(from) },
        )?;
        queue_meta(&mut tx, folder, to, &root)?;
        files::vacate_renamed(&mut tx, folder.id, from, ts)?;
        folders::mark_stale(&mut tx, folder.id)?;
        db::notify(&mut tx, db::Topic::Folders, &folder.key)?;
        Ok(())
    })();
    if let Err(e) = recorded.and_then(|()| tx.commit().map_err(Into::into)) {
        let _ = std::fs::rename(&dst, &src);
        return Err(e);
    }
    remove_empty_dirs(&src, &store.live_dir(&folder.key));
    Ok(())
}

/// Removes the folders a move left empty, up to (not including) `stop`.
fn remove_empty_dirs(moved: &Path, stop: &Path) {
    let mut dir = moved.parent();
    while let Some(d) = dir.filter(|d| *d != stop && d.starts_with(stop)) {
        if std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// Marks a live path deleted and logs it. Returns the version that was current if the path was live;
/// the caller moves its file out of the live tree with [`archive_deleted`].
fn delete_path(conn: &mut impl GenericClient, folder: &FolderRef, path: &str, ts: i64, in_web_app: bool) -> Result<Option<i64>> {
    let Some(version) = files::delete_path(conn, folder.id, path, ts, in_web_app)? else { return Ok(None) };
    log_event(conn, folder, &NewEvent { path, action: "delete", ts, hash: None, detail: None })?;
    Ok(Some(version))
}

/// Moves the file of a deleted path into the folder of its last version. A failure is only logged:
/// the file stays in the live tree. Returns whether a file was moved.
fn archive_deleted(store: &Store, folder: &FolderRef, path: &str, version_id: i64) -> bool {
    match store.archive(&folder.key, path, version_id) {
        Ok(moved) => moved,
        Err(e) => {
            tracing::error!("could not move deleted file {path} of {} out of the live tree: {e:#}", folder.key);
            false
        }
    }
}

/// Marks a path deleted. The data stays until the retention period has passed. `in_web_app` is a delete in the
/// web app: the folder keeps its file, and its later changes to it are no longer taken (see [`deleted_in_web_app`]).
/// Returns whether anything changed.
pub fn mark_deleted(conn: &mut Client, store: &Store, folder: &FolderRef, path: &str, in_web_app: bool) -> Result<bool> {
    let mut tx = conn.transaction()?;
    db::lock_path(&mut tx, folder.id, path)?;
    let changed = delete_locked(&mut tx, store, folder, path, in_web_app)?;
    tx.commit()?;
    Ok(changed)
}

/// [`mark_deleted`] for a path whose lock the transaction already holds.
fn delete_locked(tx: &mut postgres::Transaction, store: &Store, folder: &FolderRef, path: &str, in_web_app: bool) -> Result<bool> {
    let ts = unix_now();
    let deleted = delete_path(tx, folder, path, ts, in_web_app)?;
    if in_web_app && deleted.is_some() {
        // A file a source collected stays deleted until it changes at the source, whatever the retention period does.
        sql::files::keep_source_item_deleted().bind(tx, &folder.id, &path).map(drop)?;
    }
    folders::mark_stale(tx, folder.id)?;
    db::notify(tx, db::Topic::Folders, &folder.key)?;
    if let Some(version_id) = deleted {
        archive_deleted(store, folder, path, version_id);
    }
    Ok(deleted.is_some())
}

/// Records a file somebody put in, or changed in, the live tree directly. The file is already in
/// place, so only the database changes; a file it overwrote is gone, there is nothing to keep as the
/// previous version. Returns whether a version was added. Nothing happens if the file changed again
/// since it was hashed (`seen` no longer matches); the next scan sees it.
pub fn adopt_live(conn: &mut Client, store: &Store, folder: &FolderRef, found: &Found) -> Result<bool> {
    let Found { path, seen, renamed_from } = *found;
    let mut tx = conn.transaction()?;
    let mut locked = vec![path];
    locked.extend(renamed_from);
    locked.sort_unstable();
    for p in locked {
        db::lock_path(&mut tx, folder.id, p)?;
    }
    let still_there = std::fs::metadata(store.live_path(&folder.key, path)?).is_ok_and(|m| m.len() == seen.size && fafnirx_common::mtime_ns(&m) == seen.mtime);
    if !still_there {
        return Ok(false);
    }
    let ts = unix_now();
    let root: &[u8] = &seen.root;
    let existing = files::lock_row(&mut tx, folder.id, path)?;
    let live = existing.as_ref().and_then(files::Row::live_version);
    if let Some(cur) = live {
        if files::has_content(&mut tx, cur, root)? {
            // Same content, only the modification time differs: remember it so the file is not hashed again.
            sql::files::set_version_mtime().bind(&mut tx, &seen.mtime, &cur).map(drop)?;
            tx.commit()?;
            return Ok(false);
        }
    }
    let file_id = match &existing {
        Some(row) => row.id,
        None => files::ensure(&mut tx, folder.id, path)?,
    };
    let version_id = files::insert_version(
        &mut tx,
        &files::NewVersion { file_id, root, size: seen.size, mtime: seen.mtime, mode: seen.mode, created_at: ts },
    )?;
    make_current(&mut tx, file_id, version_id, ts)?;
    // A rename only if the old path still holds exactly this content; it is then vacated, not deleted.
    let renamed_from = match renamed_from {
        Some(old) if files::live_content(&mut tx, folder.id, old)?.is_some_and(|(r, _)| r == seen.root) => Some(old),
        _ => None,
    };
    let (action, detail) = match (renamed_from, live) {
        (Some(old), _) => ("rename", old),
        (None, Some(_)) => ("update", "changed in the file store"),
        (None, None) => ("add", "added to the file store"),
    };
    log_event(
        &mut tx,
        folder,
        &NewEvent { path, action, ts, hash: Some(&hex::encode(seen.root)), detail: Some(detail) },
    )?;
    queue_meta(&mut tx, folder, path, root)?;
    if let Some(old) = renamed_from {
        files::vacate_renamed(&mut tx, folder.id, old, ts)?;
    }
    folders::mark_stale(&mut tx, folder.id)?;
    tx.commit()?;
    Ok(true)
}

/// Marks a path deleted if its file is gone from the live tree (somebody removed it there). Returns
/// whether anything changed.
pub fn forget_absent(conn: &mut Client, store: &Store, folder: &FolderRef, path: &str) -> Result<bool> {
    let mut tx = conn.transaction()?;
    db::lock_path(&mut tx, folder.id, path)?;
    if store.live_path(&folder.key, path)?.exists() {
        return Ok(false);
    }
    let changed = delete_locked(&mut tx, store, folder, path, false)?;
    tx.commit()?;
    Ok(changed)
}

#[cfg(test)]
#[path = "../../tests/unit/files/ingest.rs"]
mod tests;
