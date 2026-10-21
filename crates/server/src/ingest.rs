//! Recording files in the database. Shared by the gRPC Commit/MarkDeleted calls (devices)
//! and by sources that the engine collects itself. The chunks must already be in the store.

use crate::db::setting;
use fafnirx_common::{chunks_of, unix_now, HASH_LEN};
use postgres::{Client, GenericClient};

/// (root hash, size, chunk hashes) of a file whose chunks are in the store.
pub type Content = ([u8; HASH_LEN], u64, Vec<[u8; HASH_LEN]>);

pub struct NewVersion<'a> {
    pub device_id: i64,
    pub path: &'a str,
    pub root: [u8; HASH_LEN],
    pub size: u64,
    pub chunks: &'a [[u8; HASH_LEN]],
    /// Unix nanoseconds.
    pub mtime: i64,
    pub mode: u32,
    /// Set when this path is a rename of another path whose content is identical.
    pub renamed_from: Option<&'a str>,
}

/// Keeps the newest `version_count` versions of a file; older ones are marked pruned and removed by retention.
fn prune_versions(conn: &mut impl GenericClient, file_id: i64, ts: i64) -> Result<(), postgres::Error> {
    let keep = setting(conn, "version_count", 5).max(1);
    conn.execute(
        "UPDATE versions SET pruned_at = $1 WHERE file_id = $2 AND pruned_at IS NULL AND id NOT IN
         (SELECT id FROM versions WHERE file_id = $2 AND pruned_at IS NULL ORDER BY id DESC LIMIT $3)",
        &[&ts, &file_id, &keep],
    )?;
    Ok(())
}

/// Appends to the change log shown in the web UI.
fn log_event(
    conn: &mut impl GenericClient,
    device_id: i64,
    path: &str,
    action: &str,
    ts: i64,
    hash: Option<&str>,
    detail: Option<&str>,
) -> Result<(), postgres::Error> {
    conn.execute(
        "INSERT INTO events (device_id, path, action, ts, hash, detail) VALUES ($1, $2, $3, $4, $5, $6)",
        &[&device_id, &path, &action, &ts, &hash, &detail],
    )?;
    Ok(())
}

/// Adds a version of a file and returns its id. Committing the content a file already has is a no-op
/// that returns the current version.
pub fn commit_version(conn: &mut Client, v: &NewVersion) -> Result<u64, postgres::Error> {
    let ts = unix_now();
    let root: &[u8] = &v.root;
    let mut tx = conn.transaction()?;
    // The row lock keeps two commits of the same path from interleaving.
    let existing: Option<(i64, Option<i64>, Option<i64>)> = tx
        .query_opt(
            "SELECT id, current_version_id, deleted_at FROM files WHERE device_id = $1 AND path = $2 FOR UPDATE",
            &[&v.device_id, &v.path],
        )?
        .map(|row| (row.get(0), row.get(1), row.get(2)));
    let file_id = match existing {
        Some((id, current, deleted_at)) => {
            if let (Some(cur), None) = (current, deleted_at) {
                let same: bool =
                    tx.query_one("SELECT root_hash = $2 FROM versions WHERE id = $1", &[&cur, &root])?.get(0);
                if same {
                    return Ok(cur as u64);
                }
            }
            id
        }
        None => tx
            .query_one(
                "INSERT INTO files (device_id, path) VALUES ($1, $2)
                 ON CONFLICT (device_id, path) DO UPDATE SET path = excluded.path RETURNING id",
                &[&v.device_id, &v.path],
            )?
            .get(0),
    };
    let was_live = matches!(existing, Some((_, Some(_), None)));
    let manifest: Vec<u8> = v.chunks.iter().flatten().copied().collect();
    let version_id: i64 = tx
        .query_one(
            "INSERT INTO versions (file_id, root_hash, size, mtime, mode, manifest, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
            &[&file_id, &root, &(v.size as i64), &v.mtime, &i64::from(v.mode), &manifest, &ts],
        )?
        .get(0);
    tx.execute("UPDATE files SET current_version_id = $1, deleted_at = NULL WHERE id = $2", &[&version_id, &file_id])?;
    prune_versions(&mut tx, file_id, ts)?;
    let (action, detail) = match v.renamed_from {
        Some(old) => ("rename", Some(old)),
        None if was_live => ("update", None),
        None => ("add", None),
    };
    log_event(&mut tx, v.device_id, v.path, action, ts, Some(&hex::encode(v.root)), detail)?;
    if let Some(old) = v.renamed_from {
        delete_path(&mut tx, v.device_id, old, ts, Some(&format!("renamed to {}", v.path)))?;
    }
    tx.execute("UPDATE devices SET last_seen = $1, last_collected = $1 WHERE id = $2", &[&ts, &v.device_id])?;
    tx.commit()?;
    Ok(version_id as u64)
}

/// Marks a live path deleted and logs it. Returns whether the path was live.
fn delete_path(
    conn: &mut impl GenericClient,
    device_id: i64,
    path: &str,
    ts: i64,
    detail: Option<&str>,
) -> Result<bool, postgres::Error> {
    let n = conn.execute(
        "UPDATE files SET deleted_at = $1 WHERE device_id = $2 AND path = $3 AND deleted_at IS NULL",
        &[&ts, &device_id, &path],
    )?;
    if n > 0 {
        log_event(conn, device_id, path, "delete", ts, None, detail)?;
    }
    Ok(n > 0)
}

/// Marks a path deleted. The data stays until the retention period has passed. Returns whether
/// anything changed.
pub fn mark_deleted(conn: &mut impl GenericClient, device_id: i64, path: &str) -> Result<bool, postgres::Error> {
    let ts = unix_now();
    let deleted = delete_path(conn, device_id, path, ts, None)?;
    conn.execute("UPDATE devices SET last_seen = $1 WHERE id = $2", &[&ts, &device_id])?;
    Ok(deleted)
}

/// The live (not deleted) version of a path, if any: (root hash, size, chunk hashes).
pub fn live_manifest(
    conn: &mut impl GenericClient,
    device_id: i64,
    path: &str,
) -> Result<Option<Content>, postgres::Error> {
    let row = conn.query_opt(
        "SELECT v.root_hash, v.size, v.manifest FROM files f JOIN versions v ON v.id = f.current_version_id
         WHERE f.device_id = $1 AND f.path = $2 AND f.deleted_at IS NULL",
        &[&device_id, &path],
    )?;
    Ok(row.and_then(|r| {
        let (root, size, manifest): (Vec<u8>, i64, Vec<u8>) = (r.get(0), r.get(1), r.get(2));
        let root: [u8; HASH_LEN] = root.try_into().ok()?;
        Some((root, size as u64, chunks_of(&manifest)))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing;
    use fafnirx_common::{chunk_hash, root_hash};

    fn version<'a>(
        path: &'a str,
        data: &[u8],
        chunks: &'a mut Vec<[u8; 32]>,
        renamed_from: Option<&'a str>,
    ) -> NewVersion<'a> {
        *chunks = vec![chunk_hash(data)];
        NewVersion {
            device_id: 1,
            path,
            root: root_hash(chunks, data.len() as u64),
            size: data.len() as u64,
            chunks,
            mtime: 0,
            mode: 0,
            renamed_from,
        }
    }

    #[test]
    fn add_update_noop_delete_and_rename() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let mut ch = vec![];
        let v1 = commit_version(&mut c, &version("a", b"one", &mut ch, None)).unwrap();
        assert_eq!(
            commit_version(&mut c, &version("a", b"one", &mut ch, None)).unwrap(),
            v1,
            "same content is a no-op"
        );
        let v2 = commit_version(&mut c, &version("a", b"two", &mut ch, None)).unwrap();
        assert!(v2 > v1);
        let (_, size, chunks) = live_manifest(&mut *c, 1, "a").unwrap().unwrap();
        assert_eq!((size, chunks.len()), (3, 1));

        commit_version(&mut c, &version("b", b"two", &mut ch, Some("a"))).unwrap();
        assert!(live_manifest(&mut *c, 1, "a").unwrap().is_none(), "the old path of a rename is deleted");
        assert!(mark_deleted(&mut *c, 1, "b").unwrap());
        assert!(!mark_deleted(&mut *c, 1, "b").unwrap(), "deleting twice changes nothing");
        let actions: Vec<String> =
            c.query("SELECT action FROM events ORDER BY id", &[]).unwrap().iter().map(|r| r.get(0)).collect();
        assert_eq!(actions, ["add", "update", "rename", "delete", "delete"]);
    }

    #[test]
    fn only_the_newest_versions_stay_unpruned() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let mut ch = vec![];
        for n in 0..7 {
            commit_version(&mut c, &version("a", format!("v{n}").as_bytes(), &mut ch, None)).unwrap();
        }
        let live: i64 = c.query_one("SELECT COUNT(*) FROM versions WHERE pruned_at IS NULL", &[]).unwrap().get(0);
        assert_eq!(live, 5, "version_count defaults to 5");
    }
}
