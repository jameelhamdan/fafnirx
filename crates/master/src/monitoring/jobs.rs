//! Background jobs: offline/disk alerts every minute, version pruning,
//! retention (with removal of the stored files) and cleanup of unfinished uploads and the cache every hour.

use crate::platform::db::{resolve_alert, setting, upsert_alert, Db};
use crate::platform::store::Store;
use anyhow::Result;
use fafnirx_common::unix_now;
use postgres::{Client, GenericClient};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const DISK_ALERT_PERCENT: f64 = 80.0;
/// An unfinished upload that nobody has touched for this long is abandoned.
const STALE_UPLOAD: Duration = Duration::from_secs(24 * 3600);
/// An upload error that the client has not reported again for this long is considered gone.
const UPLOAD_ERROR_TTL_SECS: i64 = 48 * 3600;
/// The change log is only for looking at recent activity.
const EVENT_KEEP_DAYS: i64 = 90;

pub fn spawn(db: Db, store: Arc<Store>, data_dir: PathBuf) {
    let (minute_db, cache_root) = (db.clone(), data_dir.join("cache"));
    tokio::spawn(async move {
        loop {
            let (d, dir) = (minute_db.clone(), data_dir.clone());
            if let Err(e) = tokio::task::spawn_blocking(move || minute(&d, &dir)).await {
                tracing::error!("minute job panicked: {e}");
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
    crate::files::livetree::spawn(db.clone(), store.clone());
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let (d, s, cache) = (db.clone(), store.clone(), cache_root.clone());
            match tokio::task::spawn_blocking(move || hourly(&d, &s, &cache)).await {
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
        // Only clients heartbeat; sources and local folders also get a last_seen when they collect something.
        .query(
            "SELECT name, last_seen FROM devices WHERE kind = 'device' AND revoked_at IS NULL AND last_seen IS NOT NULL",
            &[],
        )?
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
        upsert_alert(conn, "disk_usage", "", &format!("Master disk is {used:.0}% full"))
    } else {
        resolve_alert(conn, "disk_usage", "")
    }
}

pub fn hourly(db: &Db, store: &Store, cache_root: &Path) -> Result<()> {
    let removed_versions = retention(&mut *db.get()?)?;
    for id in removed_versions {
        store.remove_version(id);
    }
    let abandoned = store.clean_tmp(STALE_UPLOAD);
    if abandoned > 0 {
        tracing::info!("removed {abandoned} abandoned uploads");
    }
    let known: std::collections::HashSet<i64> =
        db.get()?.query("SELECT id FROM versions", &[])?.iter().map(|r| r.get(0)).collect();
    let orphans = store.sweep_versions(&known, STALE_UPLOAD);
    if orphans > 0 {
        tracing::info!("removed {orphans} stored versions that no record points to");
    }
    crate::library::meta::clean_cache(db, cache_root)?;
    Ok(())
}

/// Removes deleted files and pruned versions older than the retention period from the database and
/// returns the ids of the removed versions, whose stored files the caller deletes.
fn retention(conn: &mut Client) -> Result<Vec<i64>> {
    let cutoff = unix_now() - setting(conn, "retention_days", 30) * 86400;
    let mut tx = conn.transaction()?;
    // The rows are locked first: a file that came back in the meantime is no longer expired once its lock is
    // released, so it keeps its history.
    let expired: Vec<i64> = tx
        .query("SELECT id FROM files WHERE deleted_at IS NOT NULL AND deleted_at < $1 FOR UPDATE", &[&cutoff])?
        .iter()
        .map(|r| r.get(0))
        .collect();
    let mut removed: Vec<i64> = tx
        .query("DELETE FROM versions WHERE file_id = ANY($1) RETURNING id", &[&expired])?
        .iter()
        .map(|r| r.get(0))
        .collect();
    let files = tx.execute("DELETE FROM files WHERE id = ANY($1)", &[&expired])?;
    let pruned: Vec<i64> = tx
        .query("DELETE FROM versions WHERE pruned_at IS NOT NULL AND pruned_at < $1 RETURNING id", &[&cutoff])?
        .iter()
        .map(|r| r.get(0))
        .collect();
    tx.execute("DELETE FROM events WHERE ts < $1", &[&(unix_now() - EVENT_KEEP_DAYS * 86400)])?;
    tx.commit()?;
    if files > 0 || !pruned.is_empty() {
        tracing::info!("retention removed {files} files and {} old versions", pruned.len());
    }
    removed.extend(pruned);
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::db::testing;

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
                "INSERT INTO versions (file_id, root_hash, size, mtime, created_at) VALUES ($1, decode('00', 'hex'), 0, 0, 0)",
                &[&f],
            )
            .unwrap();
        }
        let removed = retention(&mut c).unwrap();
        assert_eq!(removed, [1], "the version of the expired file is reported so its files can be deleted");
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
    fn sources_and_local_folders_are_never_offline() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let c = &mut *c;
        c.execute("UPDATE devices SET kind = 'source', last_seen = $1", &[&(unix_now() - 600)]).unwrap();
        offline_alerts(c).unwrap();
        assert_eq!(testing::open_alerts(c), 0, "only clients send heartbeats");
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
