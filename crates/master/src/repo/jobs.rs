//! The queue of background work (`library::meta`): reading metadata, making thumbnails. Look at it and
//! steer it with `monitoring::queue`.

use crate::platform::pg::Pg;
use fafnirx_common::unix_now;
use fafnirx_db::client::Params;
use fafnirx_db::queries::jobs as q;
use postgres::{Error, GenericClient};
use uuid::Uuid;

/// How long a job that worked stays in the list.
pub const DONE_KEEP_SECS: i64 = 24 * 3600;

/// Queues background work of a kind, with its payload as JSON; the same work that is already waiting is not queued again.
pub fn enqueue(c: &mut impl GenericClient, kind: &str, payload: &serde_json::Value) -> Result<(), Error> {
    q::enqueue_job().bind(&mut Pg(c), &kind, &payload.to_string(), &unix_now())?;
    Ok(())
}

/// A job taken from the queue.
pub struct Claimed {
    pub id: Uuid,
    pub kind: String,
    pub payload: String,
    /// How many times it has been started, this time included.
    pub attempts: i64,
}

/// A restart interrupts whatever was running: it runs again.
pub fn requeue_running(c: &mut impl GenericClient) -> Result<(), Error> {
    q::requeue_running_jobs().bind(&mut Pg(c))?;
    Ok(())
}

/// Takes one due job from the queue and marks it running. One statement, so a job is only taken once.
pub fn claim(c: &mut impl GenericClient, now: i64) -> Result<Option<Claimed>, Error> {
    Ok(q::claim_job().bind(&mut Pg(c), &now).opt()?.map(|j| Claimed {
        id: j.id,
        kind: j.kind,
        payload: j.payload,
        attempts: j.attempts,
    }))
}

/// A job that worked is done; it stays in the list for a day.
pub fn complete(c: &mut impl GenericClient, id: Uuid, note: Option<&str>) -> Result<(), Error> {
    q::complete_job().bind(&mut Pg(c), &unix_now(), &note, &id)?;
    Ok(())
}

/// A job that failed: queued again after `run_after`, or failed for good, with what went wrong.
pub fn fail(c: &mut impl GenericClient, id: Uuid, give_up: bool, run_after: i64, error: &str) -> Result<(), Error> {
    let status = if give_up { "failed" } else { "queued" };
    q::retry_job().params(&mut Pg(c), &q::RetryJobParams { id, status, run_after, error, now: unix_now() })?;
    Ok(())
}

/// Forgets the jobs that worked more than a day ago.
pub fn purge_done(c: &mut impl GenericClient) -> Result<u64, Error> {
    q::purge_done_jobs().bind(&mut Pg(c), &(unix_now() - DONE_KEEP_SECS))
}

/// Jobs of one kind in one state.
pub struct Count {
    pub kind: String,
    pub status: String,
    pub n: i64,
}

/// A job as the list shows it.
pub struct Item {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub attempts: i64,
    pub run_after: i64,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    pub note: Option<String>,
    pub payload: String,
}

/// How many jobs there are of each kind in each state, for the jobs `(all, user)` can see (see `Access::scope_args`).
pub fn counts(c: &mut impl GenericClient, all: bool, user: i64) -> Result<Vec<Count>, Error> {
    let params = q::QueueCountsParams { since: unix_now() - DONE_KEEP_SECS, all, scope_user: user };
    Ok(q::queue_counts()
        .params(&mut Pg(c), &params)
        .all()?
        .into_iter()
        .map(|r| Count { kind: r.kind, status: r.status, n: r.n })
        .collect())
}

/// The jobs worth looking at: running, failed and waiting first, then those done lately.
pub fn items(c: &mut impl GenericClient, all: bool, user: i64, limit: i64) -> Result<Vec<Item>, Error> {
    let params = q::QueueItemsParams { since: unix_now() - DONE_KEEP_SECS, all, scope_user: user, limit };
    Ok(q::queue_items()
        .params(&mut Pg(c), &params)
        .all()?
        .into_iter()
        .map(|j| Item {
            id: j.id,
            kind: j.kind,
            status: j.status,
            attempts: j.attempts,
            run_after: j.run_after,
            created_at: j.created_at,
            started_at: j.started_at,
            finished_at: j.finished_at,
            error: j.error,
            note: j.note,
            payload: j.payload,
        })
        .collect())
}

/// Failed jobs run again: this one, or all of them. Returns how many.
pub fn retry_failed(c: &mut impl GenericClient, id: Option<Uuid>) -> Result<u64, Error> {
    q::retry_failed_jobs().bind(&mut Pg(c), &id)
}

/// Forgets jobs that have the state `status` (`failed` or `queued`): this one, or all of them. Returns how many.
pub fn drop_where(c: &mut impl GenericClient, status: &str, id: Option<Uuid>) -> Result<u64, Error> {
    q::drop_jobs().bind(&mut Pg(c), &status, &id)
}

/// A photo or video to read again: (device folder, path, root hash).
pub fn media_to_read(
    c: &mut impl GenericClient,
    everything: bool,
    limit: i64,
) -> Result<Vec<(String, String, Vec<u8>)>, Error> {
    Ok(q::media_to_read()
        .bind(&mut Pg(c), &everything, &limit)
        .all()?
        .into_iter()
        .map(|r| (r.folder, r.path, r.root_hash))
        .collect())
}

/// The unfinished work of a file as (kind, status): queued, running or failed.
pub fn pending_of(c: &mut impl GenericClient, device: &str, path: &str) -> Result<Vec<(String, String)>, Error> {
    Ok(q::pending_jobs_of_file()
        .bind(&mut Pg(c), &device, &path)
        .all()?
        .into_iter()
        .map(|j| (j.kind, j.status))
        .collect())
}
