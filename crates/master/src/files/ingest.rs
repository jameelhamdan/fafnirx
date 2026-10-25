//! Recording files in the database and the file store. Shared by the gRPC Commit/MarkDeleted calls
//! (devices), by the web API (uploads, deletes), by sources that the master collects itself and by
//! the file watcher. The content must already be complete in a temporary file.
//!
//! Everything that changes one path takes that path's advisory lock first (see [`db::lock_path`]) and
//! moves files before the lock is released, so the database and the file store never disagree for long
//! and no two processes work on the same path at once.

use crate::platform::db::{self, Device};
use crate::platform::store::Store;
use crate::repo::{devices, events, files, jobs, settings};
use anyhow::Result;
use fafnirx_common::{unix_now, HASH_LEN};
use postgres::{Client, GenericClient};
use std::path::Path;

/// The id of the device that holds the master files (see migration 0006).
pub const MASTER_DEVICE_ID: i64 = 0;

pub struct NewVersion<'a> {
    pub device: &'a Device,
    pub path: &'a str,
    pub root: [u8; HASH_LEN],
    pub size: u64,
    /// Unix nanoseconds.
    pub mtime: i64,
    pub mode: u32,
    /// Set when this path is a rename of another path whose content is identical.
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
fn prune_versions(conn: &mut impl GenericClient, file_id: i64, ts: i64) -> Result<(), postgres::Error> {
    let keep = settings::get_i64(conn, "version_count", 5).max(1);
    files::prune_versions(conn, file_id, keep, ts)
}

/// Makes a version the current one of its file (undeleting it) and prunes the ones that are too old.
fn make_current(conn: &mut impl GenericClient, file_id: i64, version_id: i64, ts: i64) -> Result<(), postgres::Error> {
    files::make_current(conn, file_id, version_id)?;
    prune_versions(conn, file_id, ts)
}

/// Appends to the change log shown in the web UI.
fn log_event(
    conn: &mut impl GenericClient,
    device: &Device,
    path: &str,
    action: &str,
    ts: i64,
    hash: Option<&str>,
    detail: Option<&str>,
) -> Result<(), postgres::Error> {
    events::record(conn, device.id, path, action, ts, hash, detail)?;
    db::notify(conn, "files", &device.name)?;
    db::notify(conn, "activity", &device.name)
}

/// Queues reading the kind, size and EXIF data of a new content, once per content.
fn queue_meta(conn: &mut impl GenericClient, device: &Device, path: &str, root: &[u8]) -> Result<(), postgres::Error> {
    if !crate::library::meta::wants(path) {
        return Ok(());
    }
    if !files::content_known(conn, root)? {
        jobs::enqueue(
            conn,
            "meta",
            &serde_json::json!({ "device": device.name, "path": path, "root": hex::encode(root) }),
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

/// The master files share the root of `files/` with the folders of the users: a master path cannot start with the
/// name of a user, whose folder it would be (the other way round is checked when a user is added).
pub fn check_master_path(conn: &mut impl GenericClient, device: &Device, path: &str) -> Result<()> {
    if device.id != MASTER_DEVICE_ID {
        return Ok(());
    }
    let first = path.split('/').next().unwrap_or("").to_lowercase();
    if conn.query_opt("SELECT 1 FROM users WHERE name = $1", &[&first])?.is_some() {
        return Err(crate::web::ApiError::conflict(format!(
            "{first} is the folder of a user: the master files cannot use that name"
        ))
        .into());
    }
    Ok(())
}

fn commit_inner(conn: &mut Client, store: &Store, staged: &Path, v: &NewVersion) -> Result<u64> {
    let ts = unix_now();
    let root: &[u8] = &v.root;
    let dev = v.device;
    check_master_path(conn, dev, v.path)?;
    let mut tx = conn.transaction()?;
    // Both paths of a rename, in a fixed order so that two renames cannot wait for each other.
    let mut locked = vec![v.path];
    locked.extend(v.renamed_from);
    locked.sort_unstable();
    for path in locked {
        db::lock_path(&mut tx, dev.id, path)?;
    }
    let existing = files::lock_row(&mut tx, dev.id, v.path)?;
    let live = existing.as_ref().and_then(files::Row::live_version);
    if let Some(cur) = live {
        if files::has_content(&mut tx, cur, root)? {
            return Ok(cur as u64);
        }
    }
    let file_id = match &existing {
        Some(row) => row.id,
        None => files::ensure(&mut tx, dev.id, v.path)?,
    };
    let version_id = files::insert_version(
        &mut tx,
        &files::NewVersion { file_id, root, size: v.size, mtime: v.mtime, mode: v.mode, created_at: ts },
    )?;

    // The file store is changed before the commit and put back if anything fails after that.
    let replaced = match live {
        Some(cur) if store.archive(&dev.name, v.path, cur)? => Some(cur),
        _ => None,
    };
    let undo = |store: &Store| {
        store.discard_live(&dev.name, v.path);
        if let Some(cur) = replaced {
            let _ = store.unarchive(&dev.name, v.path, cur);
        }
    };
    if let Err(e) = store.install(&dev.name, v.path, staged, v.mtime, v.mode) {
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
        log_event(&mut tx, dev, v.path, action, ts, Some(&hex::encode(v.root)), detail)?;
        queue_meta(&mut tx, dev, v.path, root)?;
        let vacated = match v.renamed_from {
            Some(old) => {
                delete_path(&mut tx, dev, old, ts, false, Some(&format!("renamed to {}", v.path)))?.map(|id| (old, id))
            }
            None => None,
        };
        devices::touch_collected(&mut tx, dev.id, ts)?;
        db::notify(&mut tx, "devices", &dev.name)?;
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
    let archived_old = vacated.filter(|(old, id)| archive_deleted(store, dev, old, *id));
    if let Err(e) = tx.commit() {
        undo(store);
        if let Some((old, id)) = archived_old {
            let _ = store.unarchive(&dev.name, old, id);
        }
        return Err(e.into());
    }
    Ok(version_id as u64)
}

/// Marks a live path deleted and logs it. Returns the version that was current if the path was live;
/// the caller moves its file out of the live tree with [`archive_deleted`].
fn delete_path(
    conn: &mut impl GenericClient,
    device: &Device,
    path: &str,
    ts: i64,
    in_web_app: bool,
    detail: Option<&str>,
) -> Result<Option<i64>> {
    let Some(version) = files::delete_path(conn, device.id, path, ts, in_web_app)? else { return Ok(None) };
    log_event(conn, device, path, "delete", ts, None, detail)?;
    Ok(Some(version))
}

/// Moves the file of a deleted path into the folder of its last version. A failure is only logged:
/// the file stays in the live tree. Returns whether a file was moved.
fn archive_deleted(store: &Store, device: &Device, path: &str, version_id: i64) -> bool {
    match store.archive(&device.name, path, version_id) {
        Ok(moved) => moved,
        Err(e) => {
            tracing::error!("could not move deleted file {path} of {} out of the live tree: {e:#}", device.name);
            false
        }
    }
}

/// Marks a path deleted. The data stays until the retention period has passed. `in_web_app` is a delete in the
/// web app: the device keeps its file, and its later changes to it are no longer taken (see [`deleted_in_web_app`]).
/// Returns whether anything changed.
pub fn mark_deleted(conn: &mut Client, store: &Store, device: &Device, path: &str, in_web_app: bool) -> Result<bool> {
    let mut tx = conn.transaction()?;
    db::lock_path(&mut tx, device.id, path)?;
    let changed = delete_locked(&mut tx, store, device, path, in_web_app)?;
    tx.commit()?;
    Ok(changed)
}

/// [`mark_deleted`] for a path whose lock the transaction already holds.
fn delete_locked(
    tx: &mut postgres::Transaction,
    store: &Store,
    device: &Device,
    path: &str,
    in_web_app: bool,
) -> Result<bool> {
    let ts = unix_now();
    let deleted = delete_path(tx, device, path, ts, in_web_app, None)?;
    if in_web_app && deleted.is_some() {
        // A file a source collected stays deleted until it changes at the source, whatever the retention period does.
        files::keep_source_item_deleted(tx, device.id, path)?;
    }
    devices::touch(tx, device.id, ts)?;
    db::notify(tx, "devices", &device.name)?;
    if let Some(version_id) = deleted {
        archive_deleted(store, device, path, version_id);
    }
    Ok(deleted.is_some())
}

/// Records a file somebody put in, or changed in, the live tree directly. The file is already in
/// place, so only the database changes; a file it overwrote is gone, there is nothing to keep as the
/// previous version. Returns whether a version was added. Nothing happens if the file changed again
/// since it was hashed (`seen` no longer matches); the next scan sees it.
pub fn adopt_live(conn: &mut Client, store: &Store, device: &Device, path: &str, seen: &Seen) -> Result<bool> {
    let mut tx = conn.transaction()?;
    db::lock_path(&mut tx, device.id, path)?;
    let still_there = std::fs::metadata(store.live_path(&device.name, path)?)
        .is_ok_and(|m| m.len() == seen.size && fafnirx_common::mtime_ns(&m) == seen.mtime);
    if !still_there {
        return Ok(false);
    }
    let ts = unix_now();
    let root: &[u8] = &seen.root;
    let existing = files::lock_row(&mut tx, device.id, path)?;
    let live = existing.as_ref().and_then(files::Row::live_version);
    if let Some(cur) = live {
        if files::has_content(&mut tx, cur, root)? {
            // Same content, only the modification time differs: remember it so the file is not hashed again.
            files::set_version_mtime(&mut tx, cur, seen.mtime)?;
            tx.commit()?;
            return Ok(false);
        }
    }
    let file_id = match &existing {
        Some(row) => row.id,
        None => files::ensure(&mut tx, device.id, path)?,
    };
    let version_id = files::insert_version(
        &mut tx,
        &files::NewVersion { file_id, root, size: seen.size, mtime: seen.mtime, mode: seen.mode, created_at: ts },
    )?;
    make_current(&mut tx, file_id, version_id, ts)?;
    let (action, detail) = match live {
        Some(_) => ("update", "changed in the file store"),
        None => ("add", "added to the file store"),
    };
    log_event(&mut tx, device, path, action, ts, Some(&hex::encode(seen.root)), Some(detail))?;
    queue_meta(&mut tx, device, path, root)?;
    devices::mark_stale(&mut tx, device.id)?;
    tx.commit()?;
    Ok(true)
}

/// Marks a path deleted if its file is gone from the live tree (somebody removed it there). Returns
/// whether anything changed.
pub fn forget_absent(conn: &mut Client, store: &Store, device: &Device, path: &str) -> Result<bool> {
    let mut tx = conn.transaction()?;
    db::lock_path(&mut tx, device.id, path)?;
    if store.live_path(&device.name, path)?.exists() {
        return Ok(false);
    }
    let changed = delete_locked(&mut tx, store, device, path, false)?;
    tx.commit()?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::db::testing;
    use crate::repo::files::{deleted_in_web_app, live_path_with_root};
    use fafnirx_common::{chunk_hash, root_hash};

    /// Writes `data` to a temporary file and commits it as `path`.
    fn commit(c: &mut Client, store: &Store, path: &str, data: &[u8], renamed_from: Option<&str>) -> Result<u64> {
        let staged = store.temp_path();
        std::fs::write(&staged, data).unwrap();
        let device = testing::device();
        let v = NewVersion {
            device: &device,
            path,
            root: root_hash(&[chunk_hash(data)], data.len() as u64),
            size: data.len() as u64,
            mtime: 0,
            mode: 0,
            renamed_from,
        };
        commit_version(c, store, &staged, &v)
    }

    fn live(store: &Store, path: &str) -> Option<Vec<u8>> {
        std::fs::read(store.live_path("admin/d", path).unwrap()).ok()
    }

    #[test]
    fn add_update_noop_delete_and_rename() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let device = testing::device();

        let v1 = commit(&mut c, &store, "a", b"one", None).unwrap();
        assert_eq!(live(&store, "a").unwrap(), b"one", "stored as a normal file under its own name");
        assert_eq!(commit(&mut c, &store, "a", b"one", None).unwrap(), v1, "same content is a no-op");
        let v2 = commit(&mut c, &store, "a", b"two", None).unwrap();
        assert!(v2 > v1);
        assert_eq!(live(&store, "a").unwrap(), b"two");
        assert_eq!(
            std::fs::read(dir.path().join(format!("versions/{v1}/admin/d/a"))).unwrap(),
            b"one",
            "the replaced file moves to its version"
        );

        let root = root_hash(&[chunk_hash(b"two")], 3);
        assert_eq!(live_path_with_root(&mut *c, 1, &root, None).unwrap().as_deref(), Some("a"));
        commit(&mut c, &store, "b", b"two", Some("a")).unwrap();
        assert!(live(&store, "a").is_none(), "the old path of a rename leaves the live tree");
        assert_eq!(live(&store, "b").unwrap(), b"two");
        assert_eq!(live_path_with_root(&mut *c, 1, &root, Some("a")).unwrap().as_deref(), Some("b"));

        assert!(mark_deleted(&mut c, &store, &device, "b", false).unwrap());
        assert!(live(&store, "b").is_none());
        assert!(!mark_deleted(&mut c, &store, &device, "b", false).unwrap(), "deleting twice changes nothing");
        assert!(!deleted_in_web_app(&mut *c, device.id, "b").unwrap(), "a delete on the device is the device's own");
        let actions: Vec<String> =
            c.query("SELECT action FROM events ORDER BY id", &[]).unwrap().iter().map(|r| r.get(0)).collect();
        assert_eq!(actions, ["add", "update", "rename", "delete", "delete"]);

        commit(&mut c, &store, "b", b"back", None).unwrap();
        assert_eq!(live(&store, "b").unwrap(), b"back", "a deleted path can be added again");

        assert!(mark_deleted(&mut c, &store, &device, "b", true).unwrap());
        assert!(deleted_in_web_app(&mut *c, device.id, "b").unwrap(), "a delete in the web app is remembered");
        commit(&mut c, &store, "b", b"restored", None).unwrap();
        assert!(!deleted_in_web_app(&mut *c, device.id, "b").unwrap(), "and ends when the file is brought back");

        // A file that a source collected is remembered as removed in the web app in the source's own record.
        c.execute(
            "INSERT INTO sources (name, kind, config, device_id, created_at) VALUES ('s', 'gdrive', '{}', 1, 0)",
            &[],
        )
        .unwrap();
        c.execute("INSERT INTO source_items (source_id, remote_id, path, remote_version) SELECT id, 'r', 'b', 'm' FROM sources", &[]).unwrap();
        let flag = |c: &mut postgres::Client| -> bool {
            c.query_one("SELECT removed_in_web_app FROM source_items", &[]).unwrap().get(0)
        };
        assert!(mark_deleted(&mut c, &store, &device, "b", false).unwrap());
        assert!(!flag(&mut c), "a delete by the source itself is not one made in the web app");
        commit(&mut c, &store, "b", b"again", None).unwrap();
        assert!(mark_deleted(&mut c, &store, &device, "b", true).unwrap());
        assert!(flag(&mut c));
    }

    #[test]
    fn a_failed_install_leaves_the_current_file_alone() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        commit(&mut c, &store, "a", b"one", None).unwrap();

        let missing = store.temp_path();
        let device = testing::device();
        let v = NewVersion {
            device: &device,
            path: "a",
            root: root_hash(&[chunk_hash(b"two")], 3),
            size: 3,
            mtime: 0,
            mode: 0,
            renamed_from: None,
        };
        assert!(commit_version(&mut c, &store, &missing, &v).is_err());
        assert_eq!(live(&store, "a").unwrap(), b"one");
        let versions: i64 = c.query_one("SELECT COUNT(*) FROM versions", &[]).unwrap().get(0);
        assert_eq!(versions, 1, "no version row for the failed commit");
    }

    #[test]
    fn only_the_newest_versions_stay_unpruned() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        for n in 0..7 {
            commit(&mut c, &store, "a", format!("v{n}").as_bytes(), None).unwrap();
        }
        let live: i64 = c.query_one("SELECT COUNT(*) FROM versions WHERE pruned_at IS NULL", &[]).unwrap().get(0);
        assert_eq!(live, 5, "version_count defaults to 5");
    }

    #[test]
    fn images_queue_a_metadata_job_once_per_content() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        commit(&mut c, &store, "photos/a.jpg", b"jpeg", None).unwrap();
        commit(&mut c, &store, "notes.txt", b"text", None).unwrap();
        let jobs: i64 = c.query_one("SELECT COUNT(*) FROM jobs WHERE kind = 'meta'", &[]).unwrap().get(0);
        assert_eq!(jobs, 1, "only the image");
    }
}
