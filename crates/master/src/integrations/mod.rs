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

use crate::platform::db::{self, Db};
use crate::platform::store::Store;
use crate::repo::alerts;
use crate::repo::links::{self, Dir, Due, RunStatus, SourceKind};
use anyhow::{anyhow, Result};
use fafnirx_common::unix_now;
use fafnirx_db::client::GenericClient;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// Sources a module reads (see `fetch_source`) that run at the same time.
const MODULE_RUNS: usize = 2;

/// What one run of a source did (files collected).
#[derive(Default, Debug)]
pub struct SourceRun {
    pub added: usize,
    /// Changed, or moved on the source.
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub skipped: usize,
    /// Files left for the next run because this one reached its limit.
    pub deferred: usize,
    pub failed: Vec<String>,
}

/// What one run of a mirror did (files sent).
#[derive(Default, Debug)]
pub struct MirrorRun {
    pub uploaded: usize,
    /// Moved on the mirror.
    pub moved: usize,
    pub removed: usize,
    /// Bytes uploaded.
    pub bytes: u64,
    /// Files the mirror holds after the run.
    pub total_files: usize,
    /// Problems that did not fail the run (old versions that could not be removed).
    pub warnings: Vec<String>,
    pub failed: Vec<String>,
}

/// What one run did; each kind says it in its own words.
#[derive(Debug)]
pub enum Outcome {
    Source(SourceRun),
    Mirror(MirrorRun),
}

impl Outcome {
    pub fn report(&self) -> JobReport {
        match self {
            Outcome::Source(s) => {
                let later = if s.deferred > 0 { format!(", {} left for the next run", s.deferred) } else { String::new() };
                JobReport {
                    summary: format!(
                        "{} added, {} updated, {} deleted, {} unchanged, {} skipped{later}",
                        s.added, s.updated, s.deleted, s.unchanged, s.skipped
                    ),
                    problems: s.failed.clone(),
                }
            }
            Outcome::Mirror(r) => JobReport {
                summary: format!(
                    "{} uploaded ({} bytes), {} moved, {} removed, {} files on the mirror",
                    r.uploaded, r.bytes, r.moved, r.removed, r.total_files
                ),
                problems: r.failed.iter().chain(&r.warnings).cloned().collect(),
            },
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
    let _ = db::with(&db, |c| Ok(links::fail_interrupted(c, unix_now())?)).await;
    // Storage reads one at a time, as they always did; what a module fetches may take hours, so it has slots of its own and
    // never holds up the rest. Mirrors upload one at a time.
    let rclone_slots = Arc::new(Semaphore::new(1));
    let module_slots = Arc::new(Semaphore::new(MODULE_RUNS));
    let mirror_slots = Arc::new(Semaphore::new(1));
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tick()).await;
            // What is due becomes a job in the queue (the jobs table is the one list of work); then the queued runs start.
            while blocking(&db, schedule).await == Some(true) {}
            while let Some((job, due)) = blocking(&db, claim).await.flatten() {
                let slots = match (due.dir, due.source_kind()) {
                    (Dir::Out, _) => &mirror_slots,
                    (Dir::In, SourceKind::Rclone) => &rclone_slots,
                    (Dir::In, SourceKind::Module(_)) => &module_slots,
                };
                let (id, dir, name) = (due.id, due.dir, due.key());
                let (slots, d, s, dir_path) = (slots.clone(), db.clone(), store.clone(), data_dir.clone());
                tokio::spawn(async move {
                    let _slot = slots.acquire_owned().await;
                    // On its own task, so that a panic fails this run instead of ending the scheduler.
                    let run = tokio::spawn({
                        let (d, s) = (d.clone(), s.clone());
                        async move { run(&d, &s, &dir_path, job, due).await }
                    });
                    if let Err(e) = run.await {
                        record(&d, RunRef { job, id, dir, name }, Err(anyhow!("the run panicked: {e}"))).await;
                    }
                });
            }
        }
    });
}

/// Runs `f` against the database on a blocking thread.
async fn blocking<T: Send + 'static>(db: &Db, f: fn(&Db) -> T) -> Option<T> {
    let db = db.clone();
    tokio::task::spawn_blocking(move || f(&db)).await.ok()
}

/// Queues the next link that is due as a job. Whether there was one.
fn schedule(db: &Db) -> bool {
    let Ok(mut conn) = db.get() else { return false };
    let c = &mut *conn;
    let Some(id) = links::schedule(c, unix_now()).map_err(|e| tracing::error!("queue a run: {e}")).ok().flatten() else { return false };
    if let Ok(Some(link)) = links::due(c, id) {
        let _ = db::notify(c, db::Topic::Jobs, &notify_key(link.dir, &link.key()));
    }
    true
}

/// Takes the next queued run: its job, and the link it is of.
fn claim(db: &Db) -> Option<(uuid::Uuid, Due)> {
    let mut conn = db.get().ok()?;
    let c = &mut *conn;
    let (job, id) = links::claim_job(c, unix_now()).map_err(|e| tracing::error!("claim a run: {e}")).ok()??;
    let link = links::due(c, id).ok()??;
    let _ = db::notify(c, db::Topic::Jobs, &notify_key(link.dir, &link.key()));
    Some((job, link))
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

/// How a run ended, for the link it was of and, if it ran as one, the job in the queue.
struct Ended<'a> {
    status: RunStatus,
    message: &'a str,
    job: Option<uuid::Uuid>,
}

fn finish(conn: &mut impl GenericClient, id: i64, dir: Dir, name: &str, ended: Ended) {
    let Ended { status, message, job } = ended;
    let message = &sentence(message);
    let ok_at = if status == RunStatus::Failed { None } else { Some(unix_now()) };
    let _ = links::finish(conn, id, status.as_str(), message, ok_at);
    if let Some(job) = job {
        let _ = links::finish_job(conn, job, status, message, unix_now());
    }
    let _ = db::notify(conn, db::Topic::Jobs, &notify_key(dir, name));
    let _ = alerts::set(conn, dir.alert_kind(), name, (status != RunStatus::Ok).then_some(message));
}

async fn run(db: &Db, store: &Arc<Store>, data_dir: &std::path::Path, job: uuid::Uuid, due: Due) {
    let (id, dir, name, kind) = (due.id, due.dir, due.key(), due.source_kind());
    tracing::info!("{} {name}: starting", dir.word());
    let result = match (dir, kind) {
        (Dir::Out, _) => mirror::run(db.clone(), store.clone(), data_dir.to_path_buf(), mirror::MirrorRow::new(due))
            .await
            .map(Outcome::Mirror),
        (Dir::In, SourceKind::Rclone) => rclone_source::run(db.clone(), store.clone(), due).await.map(Outcome::Source),
        (Dir::In, SourceKind::Module(_)) => fetch_source::run(db.clone(), store.clone(), due).await.map(Outcome::Source),
    };
    record(db, RunRef { job, id, dir, name }, result.map(|o| o.report())).await;
}

/// A run being recorded: its job in the queue, and the link (its id, direction and name) it was of.
struct RunRef {
    job: uuid::Uuid,
    id: i64,
    dir: Dir,
    name: String,
}

/// Logs how a run went and records it on its link and in the queue.
async fn record(db: &Db, of: RunRef, result: Result<JobReport>) {
    let db = db.clone();
    let word = of.dir.word();
    let done = tokio::task::spawn_blocking(move || {
        let Ok(mut conn) = db.get() else { return };
        let RunRef { job, id, dir, name } = of;
        let (status, message) = match result {
            Ok(r) => {
                tracing::info!("{word} {name}: {}", r.summary);
                (if r.problems.is_empty() { RunStatus::Ok } else { RunStatus::Partial }, r.message())
            }
            Err(e) => {
                tracing::error!("{word} {name} failed: {e:#}");
                (RunStatus::Failed, format!("{e:#}"))
            }
        };
        finish(&mut *conn, id, dir, &name, Ended { status, message: &message, job: Some(job) });
    })
    .await;
    if let Err(e) = done {
        tracing::error!("recording the result of a {word} job panicked: {e}");
    }
}

#[cfg(test)]
#[path = "../../tests/unit/integrations/mod.rs"]
mod tests;
