//! Scheduler for third-party sources (inbound) and mirrors (outbound).
//!
//! Both are configured in the web UI and stored in the database. A run starts when the interval has
//! passed, when it has never run, or when someone pressed "run now" (`run_requested`). Sources and
//! mirrors each run one job at a time.

pub mod api;
pub mod gdrive;
pub mod mirror;

use crate::platform::db::{self, resolve_alert, upsert_alert, Db, Device};
use crate::platform::store::Store;
use anyhow::{anyhow, Result};
use fafnirx_common::unix_now;
use postgres::{GenericClient, Row};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct SourceRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub config: String,
    pub device: Device,
}

pub struct MirrorRow {
    pub id: i64,
    pub name: String,
    pub remote_type: String,
    pub options: String,
    pub remote_path: String,
    pub keep_runs: i64,
}

#[derive(Default, Debug)]
pub struct Outcome {
    pub added: usize,
    pub updated: usize,
    pub renamed: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub failed: Vec<String>,
}

impl Outcome {
    pub fn report(&self) -> JobReport {
        JobReport {
            summary: format!(
                "{} added, {} updated, {} renamed, {} deleted, {} unchanged, {} skipped (not raw files)",
                self.added, self.updated, self.renamed, self.deleted, self.unchanged, self.skipped
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
    // A restart interrupts whatever was running.
    let _ = db::with(&db, |c| {
        for table in ["sources", "mirrors"] {
            c.execute(
                &format!("UPDATE {table} SET last_status = 'failed', last_message = 'interrupted by a restart' WHERE last_status = 'running'"),
                &[],
            )?;
        }
        Ok(())
    })
    .await;
    let (d, s) = (db.clone(), store.clone());
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tick()).await;
            while let Some(src) = blocking_claim(&d, claim_source).await {
                run_source(&d, &s, src).await;
            }
        }
    });
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tick()).await;
            while let Some(m) = blocking_claim(&db, claim_mirror).await {
                run_mirror(&db, &store, &data_dir, m).await;
            }
        }
    });
}

const DUE: &str = "(run_requested = 1 OR (enabled = 1 AND (last_run_at IS NULL OR last_run_at + interval_secs <= $1)))";

/// Runs a blocking claim on a blocking thread.
async fn blocking_claim<T: Send + 'static>(db: &Db, claim: fn(&Db) -> Option<T>) -> Option<T> {
    let db = db.clone();
    tokio::task::spawn_blocking(move || claim(&db)).await.ok().flatten()
}

/// Picks one due row of `table` (a source or a mirror), marks it running and builds it from `columns`,
/// which follow `id` (column 0). One statement, so a row can only be claimed once.
fn claim<T>(db: &Db, table: &str, columns: &str, build: fn(&Row) -> T) -> Option<T> {
    let mut conn = db.get().ok()?;
    let row = conn
        .query_opt(
            &format!(
                "UPDATE {table} SET last_status = 'running', last_run_at = $1, run_requested = 0, last_message = ''
                 WHERE id = (SELECT id FROM {table} WHERE {DUE} AND last_status IS DISTINCT FROM 'running'
                             ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED)
                 RETURNING id, {columns}"
            ),
            &[&unix_now()],
        )
        .map_err(|e| tracing::error!("claim from {table}: {e}"))
        .ok()??;
    let _ = db::notify(&mut *conn, "jobs", table);
    Some(build(&row))
}

fn claim_source(db: &Db) -> Option<SourceRow> {
    claim(db, "sources", "name, kind, config, device_id, (SELECT d.name FROM devices d WHERE d.id = device_id)", |r| {
        SourceRow {
            id: r.get(0),
            name: r.get(1),
            kind: r.get(2),
            config: r.get(3),
            device: Device { id: r.get(4), name: r.get(5) },
        }
    })
}

fn claim_mirror(db: &Db) -> Option<MirrorRow> {
    claim(db, "mirrors", "name, remote_type, options, remote_path, keep_runs", |r| MirrorRow {
        id: r.get(0),
        name: r.get(1),
        remote_type: r.get(2),
        options: r.get(3),
        remote_path: r.get(4),
        keep_runs: r.get(5),
    })
}

/// Messages are shown in the web UI: start them with a capital letter.
fn sentence(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn finish(
    conn: &mut impl GenericClient,
    table: &str,
    alert_kind: &str,
    id: i64,
    name: &str,
    status: &str,
    message: &str,
) {
    let message = &sentence(message);
    let ts = unix_now();
    let ok_at = if status == "failed" { None } else { Some(ts) };
    let _ = conn.execute(
        &format!("UPDATE {table} SET last_status = $1, last_message = $2, last_ok_at = COALESCE($3, last_ok_at) WHERE id = $4"),
        &[&status, message, &ok_at, &id],
    );
    let _ = db::notify(conn, "jobs", table);
    if status == "ok" {
        let _ = resolve_alert(conn, alert_kind, name);
    } else {
        let _ = upsert_alert(conn, alert_kind, name, message);
    }
}

async fn run_source(db: &Db, store: &Arc<Store>, src: SourceRow) {
    let (id, name) = (src.id, src.name.clone());
    tracing::info!("source {name}: starting");
    let result = match src.kind.as_str() {
        "gdrive" => crate::integrations::gdrive::run(db.clone(), store.clone(), src).await.map(|o| o.report()),
        other => Err(anyhow!("unknown source kind {other:?}")),
    };
    finish_job(db, "sources", "source_failed", id, name, result).await;
}

async fn run_mirror(db: &Db, store: &Arc<Store>, data_dir: &std::path::Path, m: MirrorRow) {
    let (id, name) = (m.id, m.name.clone());
    tracing::info!("mirror {name}: starting");
    let result = crate::integrations::mirror::run(db.clone(), store.clone(), data_dir.to_path_buf(), m).await;
    finish_job(db, "mirrors", "mirror_failed", id, name, result.map(|o| o.report())).await;
}

/// Logs how a job went and records it on its row.
async fn finish_job(
    db: &Db,
    table: &'static str,
    alert_kind: &'static str,
    id: i64,
    name: String,
    result: Result<JobReport>,
) {
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
            "INSERT INTO devices (name, kind, token_hash, created_at) VALUES ('source-d', 'source', '!', 0)",
            &[],
        )
        .unwrap();
        c.execute(
            "INSERT INTO sources (name, kind, config, device_id, created_at, interval_secs)
             VALUES ('d', 'gdrive', '{}', (SELECT id FROM devices), 0, 3600)",
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
        assert_eq!(first.name, "d");
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
        finish(c, "sources", "source_failed", id, "d", "failed", "boom");
        assert_eq!(testing::open_alerts(c), 1);
        finish(c, "sources", "source_failed", id, "d", "ok", "fine");
        assert_eq!(testing::open_alerts(c), 0);
        let row = c.query_one("SELECT last_status, last_ok_at FROM sources", &[]).unwrap();
        let (status, ok_at): (String, Option<i64>) = (row.get(0), row.get(1));
        assert_eq!(status, "ok");
        assert!(ok_at.is_some());
    }
}
