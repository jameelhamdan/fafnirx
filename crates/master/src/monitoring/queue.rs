//! The queue of background work, to look at and to steer: what is being read, drawn or converted, what waits, what failed and
//! why, and, when the streaming service is set up, what it is making (video segments and subtitle tracks).
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
/// Files asked to be read again at most, in one request.
const REBUILD_MAX: i64 = 5000;

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
    /// Without ffmpeg videos get no details, pictures or playable copies.
    ffmpeg: bool,
    /// Whether this user may steer the queue.
    can_control: bool,
    summary: Vec<Summary>,
    jobs: Vec<Job>,
    /// What the streaming service is making, as it reports it; absent when streaming is off or the service does not answer.
    stream: Option<serde_json::Value>,
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

/// Whether a user sees the video the streaming service names (`<user>/<device>/<path>`, or just `<path>` for a master file).
fn sees_video(who: &Access, p: &str) -> bool {
    let mut parts = p.splitn(3, '/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(user), Some(device), Some(_)) => who.can_see(&format!("{user}/{device}")),
        _ => who.can_see(crate::platform::store::MASTER_FOLDER),
    }
}

/// Keeps what `who` may see of the streaming service's report.
fn visible_tasks(who: &Access, mut report: serde_json::Value) -> serde_json::Value {
    for list in ["active", "failed"] {
        if let Some(items) = report.get_mut(list).and_then(|v| v.as_array_mut()) {
            items.retain(|i| i.get("video").and_then(|v| v.as_str()).is_some_and(|p| sees_video(who, p)));
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
    let stream = match &state.stream {
        Some(backend) => backend.tasks().await.map(|report| visible_tasks(&who, report)),
        None => None,
    };
    Ok(Json(Queue {
        paused,
        ffmpeg: crate::library::media::available(),
        can_control: who.is_admin(),
        summary,
        jobs: listed.into_iter().map(Job::from).collect(),
        stream,
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

/// Queues reading (and drawing the pictures of) photos and videos: those never read, or all of them.
pub async fn rebuild(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(b): Json<Rebuild>,
) -> Res<Changed> {
    who.require_admin()?;
    let changed = run(&state, move |c| {
        let files = jobs::media_to_read(c, b.everything, REBUILD_MAX)?;
        let n = files.len() as u64;
        for (device, path, root) in files {
            jobs::enqueue(
                c,
                "meta",
                &serde_json::json!({ "device": device, "path": path, "root": hex::encode(root) }),
            )?;
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

        let job = jobs::claim(c, 0, false).unwrap().unwrap();
        jobs::fail(c, job.id, true, 0, "no frame").unwrap();
        let listed = jobs::items(c, true, -1, LIST).unwrap();
        assert_eq!((listed[0].status.as_str(), listed[0].error.as_deref()), ("failed", Some("no frame")));
        assert_eq!(jobs::retry_failed(c, None).unwrap(), 1);

        let job = jobs::claim(c, 0, false).unwrap().unwrap();
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
