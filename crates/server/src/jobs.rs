//! Background jobs: offline/disk alerts every minute, version pruning,
//! retention and chunk garbage collection every hour.

use crate::db::{resolve_alert, setting, upsert_alert, Db, CHUNK_GATE};
use crate::store::Store;
use anyhow::Result;
use fafnirx_common::{chunks_of, unix_now, HASH_LEN};
use postgres::fallible_iterator::FallibleIterator;
use postgres::types::ToSql;
use postgres::{Client, GenericClient};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const DISK_ALERT_PERCENT: f64 = 80.0;
/// A chunk younger than this is never collected: it may belong to an upload
/// that has not been committed yet.
const GC_GRACE_SECS: i64 = 24 * 3600;
/// An upload error that the client has not reported again for this long is considered gone.
const UPLOAD_ERROR_TTL_SECS: i64 = 48 * 3600;
/// The change log is only for looking at recent activity.
const EVENT_KEEP_DAYS: i64 = 90;

pub fn spawn(db: Db, store: Arc<Store>, data_dir: PathBuf) {
    let minute_db = db.clone();
    tokio::spawn(async move {
        loop {
            let (d, dir) = (minute_db.clone(), data_dir.clone());
            if let Err(e) = tokio::task::spawn_blocking(move || minute(&d, &dir)).await {
                tracing::error!("minute job panicked: {e}");
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let (d, s) = (db.clone(), store.clone());
            match tokio::task::spawn_blocking(move || hourly(&d, &s)).await {
                Ok(Err(e)) => tracing::error!("hourly job failed: {e:#}"),
                Err(e) => tracing::error!("hourly job panicked: {e}"),
                Ok(Ok(())) => {}
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
}

fn minute(db: &Db, data_dir: &std::path::Path) {
    let mut conn = match db.get() {
        Ok(c) => c,
        Err(e) => return tracing::error!("alert job: no database connection: {e}"),
    };
    let conn = &mut *conn;
    if let Err(e) =
        offline_alerts(conn).and_then(|_| disk_alert(conn, data_dir)).and_then(|_| expire_upload_errors(conn))
    {
        tracing::error!("alert job failed: {e}");
    }
}

/// Clients report an error again every time they retry, so one that went quiet has been fixed.
fn expire_upload_errors(conn: &mut impl GenericClient) -> Result<(), postgres::Error> {
    conn.execute(
        "UPDATE alerts SET resolved_at = $1
         WHERE kind = 'upload_error' AND resolved_at IS NULL AND COALESCE(updated_at, created_at) < $2",
        &[&unix_now(), &(unix_now() - UPLOAD_ERROR_TTL_SECS)],
    )?;
    Ok(())
}

fn offline_alerts(conn: &mut impl GenericClient) -> Result<(), postgres::Error> {
    let limit = setting(conn, "offline_after_secs", 60);
    let rows: Vec<(String, i64)> = conn
        .query("SELECT name, last_seen FROM devices WHERE revoked_at IS NULL AND last_seen IS NOT NULL", &[])?
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    for (name, last_seen) in rows {
        let idle = unix_now() - last_seen;
        if idle > limit {
            upsert_alert(conn, "device_offline", &name, &format!("Device {name} has not been seen for {idle} s"))?;
        } else {
            resolve_alert(conn, "device_offline", &name)?;
        }
    }
    Ok(())
}

fn disk_alert(conn: &mut impl GenericClient, data_dir: &std::path::Path) -> Result<(), postgres::Error> {
    let Some((total, avail)) = fafnirx_common::disk_usage(data_dir) else {
        return Ok(());
    };
    if total == 0 {
        return Ok(());
    }
    let used = 100.0 * (total - avail) as f64 / total as f64;
    if used > DISK_ALERT_PERCENT {
        upsert_alert(conn, "disk_usage", "", &format!("Orchestrator disk is {used:.0}% full"))
    } else {
        resolve_alert(conn, "disk_usage", "")
    }
}

pub fn hourly(db: &Db, store: &Store) -> Result<()> {
    let referenced = {
        let mut conn = db.get()?;
        retention(&mut conn)?;
        referenced_chunks(&mut conn)?
    };
    // Finding candidates walks the disk, so no database lock is held. By the time they are deleted
    // a commit may have started using one (a client re-sent identical content that the store already
    // had), so the check is repeated under the exclusive CHUNK_GATE right before deleting.
    let cutoff = unix_now() - GC_GRACE_SECS;
    let candidates: Vec<_> =
        store.all().into_iter().filter(|(h, _, mtime)| *mtime < cutoff && !referenced.contains(h)).collect();
    if candidates.is_empty() {
        return Ok(());
    }
    let mut conn = db.get()?;
    let _gate = CHUNK_GATE.write().map_err(|_| anyhow::anyhow!("chunk gate poisoned"))?;
    let still_referenced = referenced_chunks(&mut conn)?;
    let removed = candidates
        .iter()
        .filter(|(h, path, _)| !still_referenced.contains(h) && std::fs::remove_file(path).is_ok())
        .count();
    if removed > 0 {
        tracing::info!("garbage collected {removed} chunks");
    }
    Ok(())
}

/// Removes deleted files and pruned versions older than the retention period.
fn retention(conn: &mut Client) -> Result<()> {
    let cutoff = unix_now() - setting(conn, "retention_days", 30) * 86400;
    let mut tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM versions WHERE file_id IN (SELECT id FROM files WHERE deleted_at IS NOT NULL AND deleted_at < $1)",
        &[&cutoff],
    )?;
    let files = tx.execute("DELETE FROM files WHERE deleted_at IS NOT NULL AND deleted_at < $1", &[&cutoff])?;
    let versions = tx.execute("DELETE FROM versions WHERE pruned_at IS NOT NULL AND pruned_at < $1", &[&cutoff])?;
    tx.execute("DELETE FROM events WHERE ts < $1", &[&(unix_now() - EVENT_KEEP_DAYS * 86400)])?;
    tx.commit()?;
    if files + versions > 0 {
        tracing::info!("retention removed {files} files and {versions} old versions");
    }
    Ok(())
}

/// Every chunk hash named by a stored version. Rows are streamed: the manifests of a large store add up.
fn referenced_chunks(conn: &mut Client) -> Result<HashSet<[u8; HASH_LEN]>> {
    let mut set = HashSet::new();
    let mut rows = conn.query_raw("SELECT manifest FROM versions", std::iter::empty::<&dyn ToSql>())?;
    while let Some(row) = rows.next()? {
        set.extend(chunks_of(&row.get::<_, Vec<u8>>(0)));
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing;

    #[test]
    fn retention_removes_only_expired() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let old = unix_now() - 40 * 86400;
        let fresh = unix_now() - 86400;
        c.execute("INSERT INTO files (device_id, path, deleted_at) VALUES (1, 'old', $1)", &[&old]).unwrap();
        c.execute("INSERT INTO files (device_id, path, deleted_at) VALUES (1, 'fresh', $1)", &[&fresh]).unwrap();
        c.execute("INSERT INTO files (device_id, path) VALUES (1, 'live')", &[]).unwrap();
        for f in 1..=3i64 {
            c.execute(
                "INSERT INTO versions (file_id, root_hash, size, mtime, manifest, created_at) VALUES ($1, decode('00', 'hex'), 0, 0, ''::bytea, 0)",
                &[&f],
            )
            .unwrap();
        }
        retention(&mut c).unwrap();
        let paths: Vec<String> =
            c.query("SELECT path FROM files ORDER BY path", &[]).unwrap().iter().map(|r| r.get(0)).collect();
        assert_eq!(paths, ["fresh", "live"]);
        let versions: i64 = c.query_one("SELECT COUNT(*) FROM versions", &[]).unwrap().get(0);
        assert_eq!(versions, 2);
    }

    #[test]
    fn upload_errors_expire_after_48_hours_without_a_new_report() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let c = &mut *c;
        upsert_alert(c, "upload_error", "d", "disk full").unwrap();
        upsert_alert(c, "disk_usage", "", "90% full").unwrap();
        let age = |c: &mut Client, kind: &str, hours: i64| {
            c.execute(
                "UPDATE alerts SET created_at = $1, updated_at = $1 WHERE kind = $2",
                &[&(unix_now() - hours * 3600), &kind],
            )
            .unwrap();
        };

        age(c, "upload_error", 47);
        expire_upload_errors(c).unwrap();
        assert_eq!(testing::open_alerts(c), 2, "47 hours: still open");

        age(c, "upload_error", 49);
        age(c, "disk_usage", 500);
        expire_upload_errors(c).unwrap();
        assert_eq!(testing::open_alerts(c), 1, "49 hours: the upload error is gone, other alerts are untouched");

        // The client reporting the same error again keeps (and reopens) it.
        upsert_alert(c, "upload_error", "d", "disk full").unwrap();
        age(c, "upload_error", 47);
        upsert_alert(c, "upload_error", "d", "disk full").unwrap();
        expire_upload_errors(c).unwrap();
        assert_eq!(testing::open_alerts(c), 2, "a repeated report refreshes the alert");
    }

    #[test]
    fn offline_alert_opens_and_resolves() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let c = &mut *c;
        c.execute("UPDATE devices SET last_seen = $1", &[&(unix_now() - 600)]).unwrap();
        offline_alerts(c).unwrap();
        assert_eq!(testing::open_alerts(c), 1);
        c.execute("UPDATE devices SET last_seen = $1", &[&unix_now()]).unwrap();
        offline_alerts(c).unwrap();
        assert_eq!(testing::open_alerts(c), 0);
    }
}
