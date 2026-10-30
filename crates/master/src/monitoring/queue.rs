//! The queue of background work, to look at and to steer: what is being read or made, what waits, what failed and
//! why, and what the modules that are set up report (the video module: segments being cut, subtitle tracks).
//!
//! Everybody sees the jobs of the devices they can see. Only an administrator holds the queue back, retries, cancels or asks
//! for files to be read again.

use crate::library::meta::PAUSED;
use crate::platform::id::Id;
use crate::repo::{jobs, settings};
use crate::web::access::Access;
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::State;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Jobs listed at most.
const LIST: i64 = 200;
/// Files looked up at a time when queueing them to be read again.
const REBUILD_BATCH: i64 = 1000;

#[derive(Serialize, Default)]
pub struct Summary {
    kind: String,
    queued: i64,
    running: i64,
    failed: i64,
    /// Finished in the last day.
    done: i64,
}

#[derive(Serialize)]
pub struct Job {
    id: Id,
    kind: String,
    status: String,
    device: String,
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
}

#[derive(Serialize)]
pub struct Queue {
    /// Held back by an administrator: nothing new is started.
    paused: bool,
    /// Whether this user may steer the queue.
    can_control: bool,
    summary: Vec<Summary>,
    jobs: Vec<Job>,
    /// What each module that reports its work is doing, by module id, as it reports it (the web app's module of the same
    /// name shows it). A module that is off or does not answer is not listed.
    modules: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct JobPayload {
    #[serde(default)]
    device: String,
    #[serde(default)]
    path: String,
}

impl From<jobs::Item> for Job {
    fn from(j: jobs::Item) -> Self {
        let p: JobPayload =
            serde_json::from_str(&j.payload).unwrap_or(JobPayload { device: String::new(), path: String::new() });
        let waiting_to_retry = j.status == "queued" && j.attempts > 0;
        Job {
            id: Id(j.id),
            kind: j.kind,
            status: j.status,
            device: p.device,
            path: p.path,
            attempts: j.attempts,
            error: j.error,
            note: j.note,
            created_at: j.created_at,
            started_at: j.started_at,
            finished_at: j.finished_at,
            retry_at: waiting_to_retry.then_some(j.run_after),
        }
    }
}

/// The counts per kind, in a stable order, with the kinds that have nothing left out.
fn summarize(counts: Vec<jobs::Count>) -> Vec<Summary> {
    let mut by_kind: BTreeMap<String, Summary> = BTreeMap::new();
    for c in counts {
        let s = by_kind.entry(c.kind.clone()).or_insert_with(|| Summary { kind: c.kind.clone(), ..Default::default() });
        match c.status.as_str() {
            "queued" => s.queued = c.n,
            "running" => s.running = c.n,
            "failed" => s.failed = c.n,
            "done" => s.done = c.n,
            _ => {}
        }
    }
    by_kind.into_values().collect()
}

/// Whether a user sees the file a module names: `users/<user>/<device>/<path>` for the files of a device, just `<path>` for a
/// master file (see `store::tree_path`).
fn sees_path(who: &Access, p: &str) -> bool {
    use crate::platform::store::{MASTER_FOLDER, USERS_DIR};
    let mut parts = p.strip_prefix(USERS_DIR).and_then(|r| r.strip_prefix('/')).unwrap_or("").splitn(3, '/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(user), Some(device), Some(_)) => who.can_see(&format!("{user}/{device}")),
        _ => who.can_see(MASTER_FOLDER),
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

pub async fn show(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Queue> {
    let (all, user) = who.scope_args();
    let (paused, summary, listed) = run(&state, move |c| {
        let paused = settings::get_i64(c, PAUSED, 0) == 1;
        Ok((paused, summarize(jobs::counts(c, all, user)?), jobs::items(c, all, user, LIST)?))
    })
    .await?;
    let modules =
        state.modules.statuses().await.into_iter().map(|(id, report)| (id, visible_tasks(&who, report))).collect();
    Ok(Json(Queue {
        paused,
        can_control: who.is_admin(),
        summary,
        jobs: listed.into_iter().map(Job::from).collect(),
        modules,
    }))
}

#[derive(Deserialize)]
pub struct Pause {
    paused: bool,
}

pub async fn pause(State(state): State<Shared>, Extension(who): Extension<Access>, Json(b): Json<Pause>) -> Res<Done> {
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
pub async fn retry(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(b): Json<Target>,
) -> Res<Changed> {
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
pub async fn cancel(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(b): Json<Cancel>,
) -> Res<Changed> {
    who.require_admin()?;
    if !matches!(b.status.as_str(), "failed" | "queued") {
        return Err(ApiError::bad("Only failed or waiting jobs can be removed"));
    }
    let changed = run(&state, move |c| Ok(jobs::drop_where(c, &b.status, b.id.map(|i| i.0))?)).await?;
    Ok(Json(Changed { changed }))
}

#[derive(Deserialize)]
pub struct Rebuild {
    /// Read every photo and video again, not only those that were never read.
    #[serde(default)]
    everything: bool,
}

/// Queues reading photos and videos: those never read, or all of them.
pub async fn rebuild(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(b): Json<Rebuild>,
) -> Res<Changed> {
    who.require_admin()?;
    let changed = run(&state, move |c| {
        let (mut after, mut n) = (None, 0u64);
        // Batch by batch, newest first, until there are no more.
        loop {
            let files = jobs::media_to_read(c, b.everything, after, REBUILD_BATCH)?;
            let Some((last, ..)) = files.last() else { break };
            after = Some(*last);
            n += files.len() as u64;
            for (_, device, path, root) in files {
                jobs::enqueue(
                    c,
                    "meta",
                    &serde_json::json!({ "device": device, "path": path, "root": hex::encode(root) }),
                )?;
            }
        }
        Ok(n)
    })
    .await?;
    Ok(Json(Changed { changed }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(kind: &str, status: &str, n: i64) -> jobs::Count {
        jobs::Count { kind: kind.into(), status: status.into(), n }
    }

    #[test]
    fn process_missing_includes_videos_read_without_details() {
        use crate::platform::db::testing;
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        for (path, root) in [("a/new.jpg", 1u8), ("a/seen.jpg", 2), ("a/blind.mp4", 3), ("a/known.mp4", 4)] {
            c.execute("INSERT INTO files (device_id, path) VALUES (1, $1)", &[&path]).unwrap();
            let version: i64 = c
                .query_one(
                    "INSERT INTO versions (file_id, root_hash, size, mtime, created_at) SELECT id, $2, 1, 0, 0 FROM files WHERE path = $1 RETURNING id",
                    &[&path, &vec![root; 32]],
                )
                .unwrap()
                .get(0);
            c.execute("UPDATE files SET current_version_id = $1 WHERE path = $2", &[&version, &path]).unwrap();
        }
        // The image and the video with details are read; the video that was read without any is not.
        for (root, kind, duration) in [(2u8, "image", None), (3, "video", None), (4, "video", Some(5000i64))] {
            c.execute(
                "INSERT INTO file_meta (root_hash, mime, kind, exif, read_at, duration_ms) VALUES ($1, 'x/y', $2, '{}', 0, $3)",
                &[&vec![root; 32], &kind, &duration],
            )
            .unwrap();
        }
        let mut missing: Vec<String> =
            jobs::media_to_read(&mut *c, false, None, 100).unwrap().into_iter().map(|(_, _, path, _)| path).collect();
        missing.sort();
        assert_eq!(missing, ["a/blind.mp4", "a/new.jpg"], "never read, and a video without details");
        assert_eq!(jobs::media_to_read(&mut *c, true, None, 100).unwrap().len(), 4, "everything");
        let first = jobs::media_to_read(&mut *c, true, None, 3).unwrap();
        let rest = jobs::media_to_read(&mut *c, true, first.last().map(|f| f.0), 3).unwrap();
        assert_eq!((first.len(), rest.len()), (3, 1), "in batches, each after the one before");
    }

    #[test]
    fn counts_are_gathered_per_kind() {
        let s = summarize(vec![count("thumb", "queued", 4), count("meta", "failed", 1), count("thumb", "running", 2)]);
        assert_eq!(s.iter().map(|s| s.kind.as_str()).collect::<Vec<_>>(), ["meta", "thumb"]);
        assert_eq!((s[1].queued, s[1].running, s[1].failed, s[1].done), (4, 2, 0, 0));
        assert_eq!(s[0].failed, 1);
    }

    #[test]
    fn the_same_work_is_queued_once_and_a_finished_job_stays_listed() {
        let db = crate::platform::db::testing::db_with_device();
        let mut c = db.get().unwrap();
        let c = &mut *c;
        let work = serde_json::json!({ "device": "master", "path": "a.mp4", "root": "00" });
        jobs::enqueue(c, "thumb", &work).unwrap();
        jobs::enqueue(c, "thumb", &work).unwrap();
        assert_eq!(jobs::items(c, true, -1, LIST).unwrap().len(), 1, "waiting work is not queued twice");

        let job = jobs::claim(c, 0).unwrap().unwrap();
        jobs::fail(c, job.id, true, 0, "no frame").unwrap();
        let listed = jobs::items(c, true, -1, LIST).unwrap();
        assert_eq!((listed[0].status.as_str(), listed[0].error.as_deref()), ("failed", Some("no frame")));
        assert_eq!(jobs::retry_failed(c, None).unwrap(), 1);

        let job = jobs::claim(c, 0).unwrap().unwrap();
        jobs::complete(c, job.id, Some("picture: poster.jpg")).unwrap();
        let summary = summarize(jobs::counts(c, true, -1).unwrap());
        assert_eq!((summary[0].queued, summary[0].failed, summary[0].done), (0, 0, 1));
        assert_eq!(
            jobs::items(c, false, -1, LIST).unwrap().len(),
            0,
            "a user who sees no device of it does not see it"
        );
        jobs::enqueue(c, "thumb", &work).unwrap();
        assert_eq!(jobs::items(c, true, -1, LIST).unwrap().len(), 2, "once done, the work may be queued again");
        assert_eq!(jobs::purge_done(c).unwrap(), 0, "a job done just now is kept");
    }
}
