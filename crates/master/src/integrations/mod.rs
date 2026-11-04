//! Scheduler for third-party sources (inbound) and mirrors (outbound).
//!
//! Both are configured in the web UI and kept in the database, which is what the scheduler reads. A run starts when the
//! interval has passed, when it has never run, or when someone pressed "run now" (`run_requested`). Mirrors run one job at
//! a time; sources read through rclone one at a time, and those a module reads two at a time, apart from the rclone ones.

pub mod api;
pub mod fetch_source;
pub mod mirror;
pub mod rclone_source;
pub mod scope;
pub mod spec;
pub mod syncsource;

use crate::platform::db::{self, Db, Device};
use crate::platform::store::Store;
use crate::repo::{alerts, mirrors, sources};
use anyhow::{anyhow, Result};
use fafnirx_common::unix_now;
use fafnirx_db::client::GenericClient;
use fafnirx_db::queries as sql;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// Sources a module reads (see `fetch_source`) that run at the same time.
const MODULE_RUNS: usize = 2;

/// The tables the scheduler runs jobs on. Their names are spliced into SQL, so they come from this
/// enum and never from a string.
#[derive(Clone, Copy)]
pub enum Table {
    Sources,
    Mirrors,
}

impl Table {
    pub fn as_str(self) -> &'static str {
        match self {
            Table::Sources => "sources",
            Table::Mirrors => "mirrors",
        }
    }
}

impl std::fmt::Display for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A source that is due. Its device is the folder it writes into, `prefix` the subfolder of it (empty, or a path and a slash),
/// and `key` (`<owner>/<name>`) names it in logs, alerts and live events.
pub struct SourceRow {
    pub id: i64,
    pub kind: String,
    pub config: String,
    pub device: Device,
    pub prefix: String,
    pub key: String,
}

pub struct MirrorRow {
    pub id: i64,
    pub name: String,
    /// `backup` (compressed and encrypted with the two passwords) or `replica` (the files as they are).
    pub kind: String,
    pub crypt_password: String,
    pub crypt_salt: String,
    pub remote_type: String,
    pub options: String,
    pub remote_path: String,
    /// A remote of the rclone config file ("name:path"); when set, the type, options and path are not used.
    pub remote: String,
    /// The folder it holds (see `scope`); empty for everything.
    pub scope: String,
    pub keep_runs: i64,
}

#[derive(Default, Debug)]
pub struct Outcome {
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub skipped: usize,
    /// Files left for the next run because this one reached its limit.
    pub deferred: usize,
    pub failed: Vec<String>,
}

impl Outcome {
    pub fn report(&self) -> JobReport {
        let later = if self.deferred > 0 { format!(", {} left for the next run", self.deferred) } else { String::new() };
        JobReport {
            summary: format!(
                "{} added, {} updated, {} deleted, {} unchanged, {} skipped{later}",
                self.added, self.updated, self.deleted, self.unchanged, self.skipped
            ),
            problems: self.failed.clone(),
        }
    }
}

/// What a finished job tells the web UI: a summary line and the problems found along the way.
pub struct JobReport {
    pub summary: String,
    pub problems: Vec<String>,
}

impl JobReport {
    fn message(&self) -> String {
        if self.problems.is_empty() {
            return self.summary.clone();
        }
        let shown = self.problems.iter().take(5).map(String::as_str).collect::<Vec<_>>().join("\n");
        format!("{}\n{} problems:\n{shown}", self.summary, self.problems.len())
    }
}

fn tick() -> Duration {
    Duration::from_secs(std::env::var("FAFNIRX_TICK_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(15).max(1))
}

pub async fn spawn(db: Db, store: Arc<Store>, data_dir: PathBuf) {
    // A restart interrupts whatever was running, and leaves the staging folders of its mirror runs behind.
    let _ = std::fs::remove_dir_all(data_dir.join("mirror-stage"));
    let _ = db::with(&db, |c| {
        sources::fail_interrupted(c)?;
        sql::mirrors::fail_interrupted_mirrors().bind(c).map(drop)?;
        Ok(())
    })
    .await;
    let (d, s) = (db.clone(), store.clone());
    // Storage reads one at a time, as they always did; what a module fetches may take hours, so it has slots of its own and
    // never holds up the rest.
    let rclone_slots = Arc::new(Semaphore::new(1));
    let module_slots = Arc::new(Semaphore::new(MODULE_RUNS));
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tick()).await;
            while let Some(src) = blocking_claim(&d, claim_source).await {
                let (id, name) = (src.id, src.device.name.clone());
                let (d2, s2) = (d.clone(), s.clone());
                let slots = if src.kind == "rclone" { rclone_slots.clone() } else { module_slots.clone() };
                tokio::spawn(async move {
                    let _slot = slots.acquire_owned().await;
                    // On its own task, so that a panic fails this run instead of ending the scheduler.
                    if let Err(e) = tokio::spawn({
                        let (d3, s3) = (d2.clone(), s2.clone());
                        async move { run_source(&d3, &s3, src).await }
                    })
                    .await
                    {
                        finish_job(&d2, Table::Sources, "source_failed", id, name, Err(anyhow!("the run panicked: {e}"))).await;
                    }
                });
            }
        }
    });
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tick()).await;
            while let Some(m) = blocking_claim(&db, claim_mirror).await {
                let (id, name) = (m.id, m.name.clone());
                let (d2, s2, dir) = (db.clone(), store.clone(), data_dir.clone());
                if let Err(e) = tokio::spawn(async move { run_mirror(&d2, &s2, &dir, m).await }).await {
                    finish_job(&db, Table::Mirrors, "mirror_failed", id, name, Err(anyhow!("the run panicked: {e}"))).await;
                }
            }
        }
    });
}

/// Runs a blocking claim on a blocking thread.
async fn blocking_claim<T: Send + 'static>(db: &Db, claim: fn(&Db) -> Option<T>) -> Option<T> {
    let db = db.clone();
    tokio::task::spawn_blocking(move || claim(&db)).await.ok().flatten()
}

fn claim_source(db: &Db) -> Option<SourceRow> {
    let mut conn = db.get().ok()?;
    let s = sql::sources::claim_source()
        .bind(&mut *conn, &unix_now())
        .opt()
        .map_err(|e| tracing::error!("claim from sources: {e}"))
        .ok()??;
    let key = s.key;
    let _ = db::notify(&mut *conn, "jobs", &key);
    Some(SourceRow {
        id: s.id,
        kind: s.kind,
        config: s.config,
        device: Device { id: s.device_id, name: s.folder },
        prefix: s.folder_prefix,
        key,
    })
}

fn claim_mirror(db: &Db) -> Option<MirrorRow> {
    let mut conn = db.get().ok()?;
    let m = sql::mirrors::claim_mirror()
        .bind(&mut *conn, &unix_now())
        .opt()
        .map_err(|e| tracing::error!("claim from mirrors: {e}"))
        .ok()??;
    let _ = db::notify(&mut *conn, "jobs", "mirrors");
    Some(MirrorRow {
        id: m.id,
        name: m.name,
        kind: m.kind,
        crypt_password: m.crypt_password,
        crypt_salt: m.crypt_salt,
        remote_type: m.remote_type,
        options: m.options,
        remote_path: m.remote_path,
        scope: m.scope,
        keep_runs: m.keep_runs,
        remote: m.remote,
    })
}

/// Messages are shown in the web UI: start them with a capital letter.
fn sentence(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn finish(conn: &mut impl GenericClient, table: Table, alert_kind: &str, id: i64, name: &str, status: &str, message: &str) {
    let message = &sentence(message);
    let ts = unix_now();
    let ok_at = if status == "failed" { None } else { Some(ts) };
    let _ = match table {
        Table::Sources => sources::finish(conn, id, status, message, ok_at),
        Table::Mirrors => mirrors::finish(conn, id, status, message, ok_at),
    };
    let _ = db::notify(
        conn,
        "jobs",
        &match table {
            Table::Sources => name.to_string(),
            Table::Mirrors => "mirrors".to_string(),
        },
    );
    if status == "ok" {
        let _ = alerts::resolve(conn, alert_kind, name);
    } else {
        let _ = alerts::upsert(conn, alert_kind, name, message);
    }
}

async fn run_source(db: &Db, store: &Arc<Store>, src: SourceRow) {
    let (id, name) = (src.id, src.key.clone());
    tracing::info!("source {name}: starting");
    let result = match src.kind.as_str() {
        "rclone" => crate::integrations::rclone_source::run(db.clone(), store.clone(), src).await.map(|o| o.report()),
        kind if fetch_source::is_kind(kind) => fetch_source::run(db.clone(), store.clone(), src).await.map(|o| o.report()),
        other => Err(anyhow!("unknown source kind {other:?}")),
    };
    finish_job(db, Table::Sources, "source_failed", id, name, result).await;
}

async fn run_mirror(db: &Db, store: &Arc<Store>, data_dir: &std::path::Path, m: MirrorRow) {
    let (id, name) = (m.id, m.name.clone());
    tracing::info!("mirror {name}: starting");
    let result = crate::integrations::mirror::run(db.clone(), store.clone(), data_dir.to_path_buf(), m).await;
    finish_job(db, Table::Mirrors, "mirror_failed", id, name, result.map(|o| o.report())).await;
}

/// Logs how a job went and records it on its row.
async fn finish_job(db: &Db, table: Table, alert_kind: &'static str, id: i64, name: String, result: Result<JobReport>) {
    let db = db.clone();
    let done = tokio::task::spawn_blocking(move || {
        let Ok(mut conn) = db.get() else { return };
        match result {
            Ok(r) => {
                let status = if r.problems.is_empty() { "ok" } else { "partial" };
                tracing::info!("{table} {name}: {}", r.summary);
                finish(&mut *conn, table, alert_kind, id, &name, status, &r.message());
            }
            Err(e) => {
                tracing::error!("{table} {name} failed: {e:#}");
                finish(&mut *conn, table, alert_kind, id, &name, "failed", &format!("{e:#}"));
            }
        }
    })
    .await;
    if let Err(e) = done {
        tracing::error!("recording the result of {table} job panicked: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::db::testing;

    fn db() -> Db {
        let db = testing::db();
        let mut c = db.get().unwrap();
        c.execute(
            "INSERT INTO devices (name, kind, token_hash, created_at, user_id) VALUES ('source-d', 'local', '!', 0, 1)",
            &[],
        )
        .unwrap();
        c.execute(
            "INSERT INTO sources (name, kind, config, device_id, user_id, created_at, interval_secs)
             VALUES ('d', 'rclone', '{}', (SELECT id FROM devices WHERE name = 'source-d'), 1, 0, 3600)",
            &[],
        )
        .unwrap();
        drop(c);
        db
    }

    #[test]
    fn claims_never_run_then_waits_for_the_interval_or_a_request() {
        let db = db();
        let sql = |q: &str, p: &[&(dyn postgres::types::ToSql + Sync)]| db.get().unwrap().execute(q, p).unwrap();
        let first = claim_source(&db).expect("never run: due immediately");
        assert_eq!(
            (first.device.name.as_str(), first.key.as_str()),
            ("admin/source-d", "admin/d"),
            "writes in its folder, named by its owner"
        );
        assert!(claim_source(&db).is_none(), "now running");
        sql("UPDATE sources SET last_status = 'ok'", &[]);
        assert!(claim_source(&db).is_none(), "ran just now, interval not over");
        sql("UPDATE sources SET run_requested = 1", &[]);
        assert!(claim_source(&db).is_some(), "run now");
        sql("UPDATE sources SET last_status = 'ok', last_run_at = $1", &[&(unix_now() - 7200)]);
        assert!(claim_source(&db).is_some(), "interval over");
        sql("UPDATE sources SET last_status = 'ok', last_run_at = $1, enabled = 0", &[&(unix_now() - 7200)]);
        assert!(claim_source(&db).is_none(), "disabled sources only run on request");
    }

    #[test]
    fn failures_open_an_alert_and_success_closes_it() {
        let db = db();
        let mut c = db.get().unwrap();
        let c = &mut *c;
        let id: i64 = c.query_one("SELECT id FROM sources", &[]).unwrap().get(0);
        finish(c, Table::Sources, "source_failed", id, "d", "failed", "boom");
        assert_eq!(testing::open_alerts(c), 1);
        finish(c, Table::Sources, "source_failed", id, "d", "ok", "fine");
        assert_eq!(testing::open_alerts(c), 0);
        let row = c.query_one("SELECT last_status, last_ok_at FROM sources", &[]).unwrap();
        let (status, ok_at): (String, Option<i64>) = (row.get(0), row.get(1));
        assert_eq!(status, "ok");
        assert!(ok_at.is_some());
    }
}
