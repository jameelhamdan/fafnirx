//! Background jobs: offline/disk alerts every minute, version pruning,
//! retention (with removal of the stored files) and cleanup of unfinished uploads and the cache every hour.

use crate::platform::db::Db;
use crate::platform::store::Store;
use crate::repo::{alerts, devices, retention, settings};
use anyhow::Result;
use fafnirx_common::unix_now;
use fafnirx_db::client::GenericClient;
use fafnirx_db::queries as sql;
use postgres::Client;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const DISK_ALERT_PERCENT: f64 = 80.0;
/// An unfinished upload that nobody has touched for this long is abandoned.
const STALE_UPLOAD: Duration = Duration::from_secs(24 * 3600);
/// An upload error that the client has not reported again for this long is considered gone.
const UPLOAD_ERROR_TTL_SECS: i64 = 48 * 3600;
/// A computer says every 20 seconds that it holds deletions back; after this long without it the alert is closed.
const HELD_ALERT_TTL_SECS: i64 = 15 * 60;
/// The change log and the alerts that were resolved are only for looking at recent activity.
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
    if let Err(e) = offline_alerts(conn)
        .and_then(|_| disk_alert(conn, data_dir))
        .and_then(|_| expire_quiet_alerts(conn))
    {
        tracing::error!("alert job failed: {e}");
    }
    // Devices that are not syncing (a local folder, a source, a change in the web app) get their totals here.
    if let Err(e) = crate::devices::totals::refresh_stale(conn) {
        tracing::error!("device totals failed: {e:#}");
    }
}

/// Clients report an error, and deletions they hold back, again with every retry or heartbeat, so one that went quiet has
/// been fixed (or the folder is no longer attached).
fn expire_quiet_alerts(conn: &mut impl GenericClient) -> Result<(), postgres::Error> {
    alerts::expire_quiet(conn, "upload_error", unix_now() - UPLOAD_ERROR_TTL_SECS)?;
    alerts::expire_quiet(conn, "deletions_held", unix_now() - HELD_ALERT_TTL_SECS)?;
    Ok(())
}

fn offline_alerts(conn: &mut impl GenericClient) -> Result<(), postgres::Error> {
    let limit = settings::get_i64(conn, "offline_after_secs", 60);
    // Only clients heartbeat; sources and local folders also get a last_seen when they collect something.
    let rows = devices::clients_last_seen(conn)?;
    for (name, last_seen) in rows {
        let idle = unix_now() - last_seen;
        let message = (idle > limit).then(|| format!("Device {name} has not been seen for {idle} s"));
        alerts::set(conn, "device_offline", &name, message.as_deref())?;
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
    let message = (used > DISK_ALERT_PERCENT).then(|| format!("Master disk is {used:.0}% full"));
    alerts::set(conn, "disk_usage", "", message.as_deref())
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
    let known: std::collections::HashSet<i64> = sql::retention::all_version_ids().bind(&mut *db.get()?).all()?.into_iter().collect();
    let orphans = store.sweep_versions(&known, STALE_UPLOAD);
    if orphans > 0 {
        tracing::info!("removed {orphans} stored versions that no record points to");
    }
    crate::library::meta::clean_cache(cache_root);
    crate::repo::jobs::purge_done(&mut *db.get()?)?;
    Ok(())
}

/// Removes deleted files and pruned versions older than the retention period from the database and
/// returns the ids of the removed versions, whose stored files the caller deletes.
fn retention(conn: &mut Client) -> Result<Vec<i64>> {
    let cutoff = unix_now() - settings::get_i64(conn, "retention_days", 30) * 86400;
    let mut tx = conn.transaction()?;
    // The rows are locked first: a file that came back in the meantime is no longer expired once its lock is
    // released, so it keeps its history.
    let (expired, device_ids) = retention::lock_expired_files(&mut tx, cutoff)?;
    let mut removed = retention::delete_versions_of(&mut tx, &expired)?;
    let files = retention::delete_files(&mut tx, &expired)?;
    // Deletions before this time are no longer all known: a computer that was last in step before it has to be sent the whole
    // list of files instead of the changes (`devices::grpc::list_changes`). It only moves forward.
    let known_from = settings::get_i64(&mut tx, settings::DELETIONS_KNOWN_FROM, 0).max(cutoff);
    settings::set(&mut tx, settings::DELETIONS_KNOWN_FROM, &known_from.to_string())?;
    sql::devices::mark_totals_stale().bind(&mut tx, &device_ids).map(drop)?;
    let pruned = sql::retention::delete_pruned_versions().bind(&mut tx, &cutoff).all()?;
    sql::events::purge_events().bind(&mut tx, &(unix_now() - EVENT_KEEP_DAYS * 86400)).map(drop)?;
    sql::alerts::purge_resolved_alerts()
        .bind(&mut tx, &(unix_now() - EVENT_KEEP_DAYS * 86400))
        .map(drop)?;
    tx.commit()?;
    if files > 0 || !pruned.is_empty() {
        tracing::info!("retention removed {files} files and {} old versions", pruned.len());
    }
    removed.extend(pruned);
    Ok(removed)
}

#[cfg(test)]
#[path = "../../tests/unit/monitoring/jobs.rs"]
mod tests;
