//! The queue of background work, to look at and to steer: what is being read or made, what waits, what failed and
//! why. One list for all of it, whatever does the work: the jobs in the database (details, pictures), the runs of sources and
//! mirrors, and what the modules that are set up report (files being fetched, segments being cut), each row saying what it is
//! for and which module (`plugin`) does it.
//!
//! Everybody sees the jobs of the folders they can see. Only an administrator holds the queue back, retries, cancels or asks
//! for files to be read again.

use crate::library::meta::PAUSED;
use crate::platform::id::Id;
use crate::repo::jobs::{self, JobKind, JobStatus};
use crate::repo::settings;
use crate::web::access::{Access, Ctx};
use crate::web::{done, run, ApiError, Done, Res};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Jobs listed at most.
const LIST: i64 = 200;
/// Files looked up at a time when queueing them to be read again.
const REBUILD_BATCH: i64 = 1000;

#[derive(Serialize, Default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "QueueSummary"))]
pub struct Summary {
    kind: String,
    queued: i64,
    running: i64,
    failed: i64,
    /// Finished in the last day.
    done: i64,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "QueueJob"))]
pub struct Job {
    id: Id,
    kind: String,
    #[cfg_attr(test, ts(type = "'queued' | 'running' | 'failed' | 'done'"))]
    status: JobStatus,
    folder: String,
    path: String,
    attempts: i64,
    error: Option<String>,
    /// What a finished job found (the picture a lookup chose).
    note: Option<String>,
    created_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    /// A job that failed and will be tried again waits until then.
    retry_at: Option<i64>,
    /// The module or kind of source that does the work; empty for the master's own.
    plugin: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "QueueState"))]
pub struct Queue {
    /// Held back by an administrator: nothing new is started.
    paused: bool,
    /// Whether this user may steer the queue.
    can_control: bool,
    summary: Vec<Summary>,
    jobs: Vec<Job>,
}

#[derive(Deserialize)]
struct JobPayload {
    #[serde(default)]
    folder: String,
    #[serde(default)]
    path: String,
}

impl From<jobs::Item> for Job {
    fn from(j: jobs::Item) -> Self {
        let p: JobPayload = serde_json::from_str(&j.payload).unwrap_or(JobPayload { folder: String::new(), path: String::new() });
        let status = JobStatus::parse(&j.status).unwrap_or(JobStatus::Failed);
        let waiting_to_retry = status == JobStatus::Queued && j.attempts > 0;
        // A run of a source is for the folder it writes into and named `<owner>/<name>`; a mirror by its name. A source a module reads
        // names the module.
        let (folder, path, plugin) = match &j.link {
            Some(l) => (
                l.folder.clone().unwrap_or_default(),
                l.owner.as_ref().map_or_else(|| l.name.clone(), |owner| format!("{owner}/{}", l.name)),
                if j.kind == JobKind::Source.as_str() && l.kind != "rclone" {
                    l.kind.clone()
                } else {
                    String::new()
                },
            ),
            None => (p.folder, p.path, String::new()),
        };
        Job {
            id: Id(j.id),
            kind: j.kind,
            status,
            folder,
            path,
            attempts: j.attempts,
            error: j.error,
            note: j.note,
            created_at: j.created_at,
            started_at: j.started_at,
            finished_at: j.finished_at,
            retry_at: waiting_to_retry.then_some(j.run_after),
            plugin,
        }
    }
}

/// The counts per kind, in a stable order, with the kinds that have nothing left out.
fn summarize(counts: Vec<jobs::Count>) -> Vec<Summary> {
    let mut by_kind: BTreeMap<String, Summary> = BTreeMap::new();
    for c in counts {
        let s = by_kind
            .entry(c.kind.clone())
            .or_insert_with(|| Summary { kind: c.kind.clone(), ..Default::default() });
        match JobStatus::parse(&c.status) {
            Some(JobStatus::Queued) => s.queued = c.n,
            Some(JobStatus::Running) => s.running = c.n,
            Some(JobStatus::Failed) => s.failed = c.n,
            Some(JobStatus::Done) => s.done = c.n,
            None => {}
        }
    }
    by_kind.into_values().collect()
}

/// Whether a user sees the file a module names: `users/<key>/<path>` for the files of a folder of a user or a device, `<name>/<path>` for a
/// file of a master folder, which everybody sees (see `store::tree_path`).
fn sees_path(who: &Access, p: &str) -> bool {
    use crate::platform::store::USERS_DIR;
    let mut parts = p.strip_prefix(USERS_DIR).and_then(|r| r.strip_prefix('/')).unwrap_or("").splitn(3, '/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(user), Some(folder), Some(_)) => who.can_see(&format!("{user}/{folder}")),
        _ => true,
    }
}

/// Keeps what `who` may see of a module's report: the tasks (`active`, `failed`) of the files they can see.
fn visible_tasks(who: &Access, mut report: serde_json::Value) -> serde_json::Value {
    for list in ["active", "failed"] {
        if let Some(items) = report.get_mut(list).and_then(|v| v.as_array_mut()) {
            items.retain(|i| i.get("path").and_then(|v| v.as_str()).is_some_and(|p| sees_path(who, p)));
        }
    }
    report
}

/// A row that does not come from the `jobs` table has no id of its own: this one is made from what names it, so that it is the
/// same row every time the list is asked for.
fn id_of(key: &str) -> Id {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(key.as_bytes());
    Id(uuid::Uuid::from_slice(&digest[..16]).unwrap_or_default())
}

/// What a module reports doing (`active`) and what failed lately (`failed`), as rows of the list. The reports of all modules have
/// that shape (`fafnirx-module-kit`'s tasks); the module's own word for the task is the kind of the row.
fn module_jobs(module: &str, report: &serde_json::Value) -> Vec<Job> {
    let text = |v: &serde_json::Value, key: &str| v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let number = |v: &serde_json::Value, key: &str| v.get(key).and_then(|x| x.as_i64());
    let mut out = Vec::new();
    for a in report.get("active").and_then(|v| v.as_array()).into_iter().flatten() {
        let (running, since) = (a.get("running").and_then(|x| x.as_bool()).unwrap_or(true), number(a, "since").unwrap_or_default());
        out.push(Job {
            id: id_of(&format!("{module}/{}/{}/{since}", text(a, "kind"), text(a, "path"))),
            kind: text(a, "kind"),
            status: if running { JobStatus::Running } else { JobStatus::Queued },
            folder: String::new(),
            path: text(a, "path"),
            attempts: 0,
            error: None,
            note: None,
            created_at: since,
            started_at: running.then_some(since),
            finished_at: None,
            retry_at: None,
            plugin: module.into(),
        });
    }
    for f in report.get("failed").and_then(|v| v.as_array()).into_iter().flatten() {
        let at = number(f, "at").unwrap_or_default();
        out.push(Job {
            id: id_of(&format!("{module}/failed/{}/{}/{at}", text(f, "kind"), text(f, "path"))),
            kind: text(f, "kind"),
            status: JobStatus::Failed,
            folder: String::new(),
            path: text(f, "path"),
            attempts: 1,
            error: Some(text(f, "error")),
            note: None,
            created_at: at,
            started_at: None,
            finished_at: Some(at),
            retry_at: None,
            plugin: module.into(),
        });
    }
    out
}

/// Adds the rows that do not come from the `jobs` table to the counts per kind.
fn count_rows(summary: &mut Vec<Summary>, rows: &[Job]) {
    for job in rows {
        let at = summary.iter().position(|s| s.kind == job.kind).unwrap_or_else(|| {
            summary.push(Summary { kind: job.kind.clone(), ..Default::default() });
            summary.len() - 1
        });
        let s = &mut summary[at];
        match job.status {
            JobStatus::Queued => s.queued += 1,
            JobStatus::Running => s.running += 1,
            JobStatus::Failed => s.failed += 1,
            JobStatus::Done => s.done += 1,
        }
    }
}

pub async fn show(Ctx { state, who }: Ctx) -> Res<Queue> {
    let (all, user) = who.scope_args();
    let (paused, mut summary, listed) = run(&state, move |c| {
        let paused = settings::get_i64(c, PAUSED, 0) == 1;
        Ok((paused, summarize(jobs::counts(c, all, user)?), jobs::items(c, all, user, LIST)?))
    })
    .await?;
    let mut others = Vec::new();
    for (id, report) in state.modules.statuses().await {
        others.extend(module_jobs(&id, &visible_tasks(&who, report)));
    }
    count_rows(&mut summary, &others);
    let mut jobs: Vec<Job> = listed.into_iter().map(Job::from).chain(others).collect();
    // What goes on first, then what failed, then what waits, then what is done, the newest first.
    let rank = |j: &Job| match j.status {
        JobStatus::Running => 0,
        JobStatus::Failed => 1,
        JobStatus::Queued => 2,
        JobStatus::Done => 3,
    };
    // (waiting work in the order it will be done, the rest the newest first)
    jobs.sort_by_key(|j| {
        let at = j.finished_at.or(j.started_at).unwrap_or(j.created_at);
        (rank(j), if j.status == JobStatus::Queued { at } else { -at })
    });
    Ok(Json(Queue { paused, can_control: who.is_admin(), summary, jobs }))
}

#[derive(Deserialize)]
pub struct Pause {
    paused: bool,
}

pub async fn pause(Ctx { state, who }: Ctx, Json(b): Json<Pause>) -> Res<Done> {
    who.require_admin()?;
    run(&state, move |c| Ok(settings::set(c, PAUSED, if b.paused { "1" } else { "0" })?)).await?;
    done()
}

#[derive(Deserialize)]
pub struct Target {
    /// One job; all of them when left out.
    id: Option<Id>,
}

#[derive(Serialize)]
pub struct Changed {
    changed: u64,
}

/// Failed jobs run again from the start.
pub async fn retry(Ctx { state, who }: Ctx, Json(b): Json<Target>) -> Res<Changed> {
    who.require_admin()?;
    let changed = run(&state, move |c| Ok(jobs::retry_failed(c, b.id.map(|i| i.0))?)).await?;
    Ok(Json(Changed { changed }))
}

#[derive(Deserialize)]
pub struct Cancel {
    /// `failed` (forget it) or `queued` (do not start it).
    status: String,
    id: Option<Id>,
}

/// Forgets failed jobs, or cancels jobs that have not started. A running job cannot be stopped: it ends by itself.
pub async fn cancel(Ctx { state, who }: Ctx, Json(b): Json<Cancel>) -> Res<Changed> {
    who.require_admin()?;
    let status = JobStatus::parse(&b.status)
        .filter(|s| matches!(s, JobStatus::Failed | JobStatus::Queued))
        .ok_or_else(|| ApiError::bad("Only failed or waiting jobs can be removed"))?;
    let changed = run(&state, move |c| Ok(jobs::drop_where(c, status, b.id.map(|i| i.0))?)).await?;
    Ok(Json(Changed { changed }))
}

#[derive(Deserialize)]
pub struct Rebuild {
    /// Read every photo and video again, not only those that were never read.
    #[serde(default)]
    everything: bool,
}

/// Queues reading photos and videos: those never read, or all of them.
pub async fn rebuild(Ctx { state, who }: Ctx, Json(b): Json<Rebuild>) -> Res<Changed> {
    who.require_admin()?;
    let changed = run(&state, move |c| {
        let (mut after, mut n) = (None, 0u64);
        // Batch by batch, newest first, until there are no more.
        loop {
            let files = jobs::media_to_read(c, b.everything, after, REBUILD_BATCH)?;
            let Some((last, ..)) = files.last() else { break };
            after = Some(*last);
            n += files.len() as u64;
            for (_, folder, path, root) in files {
                jobs::enqueue(c, JobKind::Meta, &serde_json::json!({ "folder": folder, "path": path, "root": hex::encode(root) }))?;
            }
        }
        Ok(n)
    })
    .await?;
    Ok(Json(Changed { changed }))
}

#[cfg(test)]
#[path = "../../tests/unit/monitoring/queue.rs"]
mod tests;
