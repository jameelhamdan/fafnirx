//! Scheduler for third-party sources (inbound) and mirrors (outbound).
//!
//! Both are configured in the web UI and kept in the database, which is what the scheduler reads. A run starts when the
//! interval has passed, when it has never run, or when someone pressed "run now" (`run_requested`). Mirrors run one job at
//! a time; sources read through rclone one at a time, and those a module reads two at a time, apart from the rclone ones.

pub mod api;
pub mod fetch_source;
pub mod mirror;
pub mod rclone_source;
pub mod spec;
pub mod syncsource;

use crate::platform::db::{self, Db, Device};
use crate::platform::store::Store;
use crate::repo::links::{self, Dir};
use crate::repo::{alerts, mirrors};
use anyhow::{anyhow, Result};
use fafnirx_common::unix_now;
use fafnirx_db::client::GenericClient;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// Sources a module reads (see `fetch_source`) that run at the same time.
const MODULE_RUNS: usize = 2;

/// A source that is due. Its device is the folder it writes into, and `key` (`<owner>/<name>`) names it in logs, alerts and live events.
pub struct SourceRow {
    pub id: i64,
    pub kind: String,
    pub config: String,
    pub device: Device,
    pub key: String,
}

pub struct MirrorRow {
    pub id: i64,
    pub name: String,
    /// `backup` (encrypted with the two passwords) or `replica` (the files as they are).
    pub kind: String,
    pub crypt_password: String,
    pub crypt_salt: String,
    pub remote_type: String,
    pub options: String,
    pub remote_path: String,
    /// A remote of the rclone config file ("name:path"); when set, the type, options and path are not used.
    pub remote: String,
    /// The folder it holds (its id and name); none for everything.
    pub folder_id: Option<i64>,
    pub folder: String,
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
    let _ = db::with(&db, |c| Ok(links::fail_interrupted(c)?)).await;
    // Storage reads one at a time, as they always did; what a module fetches may take hours, so it has slots of its own and
    // never holds up the rest. Mirrors upload one at a time.
    let rclone_slots = Arc::new(Semaphore::new(1));
    let module_slots = Arc::new(Semaphore::new(MODULE_RUNS));
    let mirror_slots = Arc::new(Semaphore::new(1));
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tick()).await;
            while let Some(due) = blocking_claim(&db).await {
                let (slots, id, dir, name) = match &due {
                    Due::Source(s) => (if s.kind == "rclone" { &rclone_slots } else { &module_slots }, s.id, Dir::In, s.key.clone()),
                    Due::Mirror(m) => (&mirror_slots, m.id, Dir::Out, m.name.clone()),
                };
                let (slots, d, s, dir_path) = (slots.clone(), db.clone(), store.clone(), data_dir.clone());
                tokio::spawn(async move {
                    let _slot = slots.acquire_owned().await;
                    // On its own task, so that a panic fails this run instead of ending the scheduler.
                    let run = tokio::spawn({
                        let (d, s) = (d.clone(), s.clone());
                        async move { run(&d, &s, &dir_path, due).await }
                    });
                    if let Err(e) = run.await {
                        finish_job(&d, id, dir, name, Err(anyhow!("the run panicked: {e}"))).await;
                    }
                });
            }
        }
    });
}

/// What is due: a source to collect or a mirror to bring up to date.
enum Due {
    Source(SourceRow),
    Mirror(MirrorRow),
}

/// Claims the next due run on a blocking thread.
async fn blocking_claim(db: &Db) -> Option<Due> {
    let db = db.clone();
    tokio::task::spawn_blocking(move || claim(&db)).await.ok().flatten()
}

fn claim(db: &Db) -> Option<Due> {
    let mut conn = db.get().ok()?;
    let c = &mut *conn;
    let id = links::claim(c, unix_now()).map_err(|e| tracing::error!("claim a run: {e}")).ok()??;
    let link = links::due(c, id).ok()??;
    let _ = db::notify(c, db::Topic::Jobs, &notify_key(link.dir, &link.key()));
    Some(match link.dir {
        Dir::In => Due::Source(SourceRow {
            id,
            key: link.key(),
            kind: link.kind,
            config: link.config,
            device: Device { id: link.folder_id?, name: link.folder? },
        }),
        Dir::Out => {
            let m: mirrors::Config = serde_json::from_str(&link.config).unwrap_or_default();
            Due::Mirror(MirrorRow {
                id,
                name: link.name,
                kind: link.kind,
                crypt_password: m.crypt_password,
                crypt_salt: m.crypt_salt,
                remote_type: m.remote_type,
                options: m.options,
                remote_path: m.remote_path,
                folder_id: link.folder_id,
                folder: link.folder.unwrap_or_default(),
                keep_runs: m.keep_runs,
                remote: m.remote,
            })
        }
    })
}

/// Who a live event about a link goes to: the owner of a source and the administrators for a mirror.
pub fn notify_key(dir: Dir, key: &str) -> String {
    match dir {
        Dir::In => key.to_string(),
        Dir::Out => "mirrors".to_string(),
    }
}

/// Messages are shown in the web UI: start them with a capital letter.
fn sentence(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn finish(conn: &mut impl GenericClient, id: i64, dir: Dir, name: &str, status: &str, message: &str) {
    let message = &sentence(message);
    let ok_at = if status == "failed" { None } else { Some(unix_now()) };
    let _ = links::finish(conn, id, status, message, ok_at);
    let _ = db::notify(conn, db::Topic::Jobs, &notify_key(dir, name));
    let _ = alerts::set(conn, dir.alert_kind(), name, (status != "ok").then_some(message));
}

async fn run(db: &Db, store: &Arc<Store>, data_dir: &std::path::Path, due: Due) {
    match due {
        Due::Source(src) => {
            let (id, name) = (src.id, src.key.clone());
            tracing::info!("source {name}: starting");
            let result = match src.kind.as_str() {
                "rclone" => crate::integrations::rclone_source::run(db.clone(), store.clone(), src).await.map(|o| o.report()),
                kind if fetch_source::is_kind(kind) => fetch_source::run(db.clone(), store.clone(), src).await.map(|o| o.report()),
                other => Err(anyhow!("unknown source kind {other:?}")),
            };
            finish_job(db, id, Dir::In, name, result).await;
        }
        Due::Mirror(m) => {
            let (id, name) = (m.id, m.name.clone());
            tracing::info!("mirror {name}: starting");
            let result = crate::integrations::mirror::run(db.clone(), store.clone(), data_dir.to_path_buf(), m).await;
            finish_job(db, id, Dir::Out, name, result.map(|o| o.report())).await;
        }
    }
}

/// Logs how a job went and records it on its link.
async fn finish_job(db: &Db, id: i64, dir: Dir, name: String, result: Result<JobReport>) {
    let db = db.clone();
    let word = dir.word();
    let done = tokio::task::spawn_blocking(move || {
        let Ok(mut conn) = db.get() else { return };
        match result {
            Ok(r) => {
                let status = if r.problems.is_empty() { "ok" } else { "partial" };
                tracing::info!("{word} {name}: {}", r.summary);
                finish(&mut *conn, id, dir, &name, status, &r.message());
            }
            Err(e) => {
                tracing::error!("{word} {name} failed: {e:#}");
                finish(&mut *conn, id, dir, &name, "failed", &format!("{e:#}"));
            }
        }
    })
    .await;
    if let Err(e) = done {
        tracing::error!("recording the result of a {word} job panicked: {e}");
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
            "INSERT INTO links (direction, name, kind, config, folder_id, user_id, created_at, interval_secs)
             VALUES ('in', 'd', 'rclone', '{}', (SELECT id FROM devices WHERE name = 'source-d'), 1, 0, 3600)",
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
        let Some(Due::Source(first)) = claim(&db) else { panic!("never run: due immediately") };
        assert_eq!(
            (first.device.name.as_str(), first.key.as_str()),
            ("admin/source-d", "admin/d"),
            "writes in its folder, named by its owner"
        );
        assert!(claim(&db).is_none(), "now running");
        sql("UPDATE links SET last_status = 'ok'", &[]);
        assert!(claim(&db).is_none(), "ran just now, interval not over");
        sql("UPDATE links SET run_requested = 1", &[]);
        assert!(claim(&db).is_some(), "run now");
        sql("UPDATE links SET last_status = 'ok', last_run_at = $1", &[&(unix_now() - 7200)]);
        assert!(claim(&db).is_some(), "interval over");
        sql("UPDATE links SET last_status = 'ok', last_run_at = $1, enabled = 0", &[&(unix_now() - 7200)]);
        assert!(claim(&db).is_none(), "disabled sources only run on request");
    }

    #[test]
    fn failures_open_an_alert_and_success_closes_it() {
        let db = db();
        let mut c = db.get().unwrap();
        let c = &mut *c;
        let id: i64 = c.query_one("SELECT id FROM links", &[]).unwrap().get(0);
        finish(c, id, Dir::In, "d", "failed", "boom");
        assert_eq!(testing::open_alerts(c), 1);
        finish(c, id, Dir::In, "d", "ok", "fine");
        assert_eq!(testing::open_alerts(c), 0);
        let row = c.query_one("SELECT last_status, last_ok_at FROM links", &[]).unwrap();
        let (status, ok_at): (String, Option<i64>) = (row.get(0), row.get(1));
        assert_eq!(status, "ok");
        assert!(ok_at.is_some());
    }
}
