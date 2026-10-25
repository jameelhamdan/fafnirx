//! The queue of background work (`library::meta`): reading metadata, making thumbnails and playable copies.

use crate::platform::pg::Pg;
use fafnirx_common::unix_now;
use fafnirx_db::client::Params;
use fafnirx_db::queries::jobs as q;
use postgres::{Error, GenericClient};

/// Queues background work of a kind, with its payload as JSON.
pub fn enqueue(c: &mut impl GenericClient, kind: &str, payload: &serde_json::Value) -> Result<(), Error> {
    q::enqueue_job().bind(&mut Pg(c), &kind, &payload.to_string(), &unix_now())?;
    Ok(())
}

/// A job taken from the queue.
pub struct Claimed {
    pub id: i64,
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

/// Takes one due job from the queue and marks it running; the other lane (`with_proxies` false) leaves the slow video
/// conversions alone. One statement, so a job is only taken once.
pub fn claim(c: &mut impl GenericClient, now: i64, with_proxies: bool) -> Result<Option<Claimed>, Error> {
    let params = q::ClaimJobParams { now, with_proxies };
    Ok(q::claim_job().params(&mut Pg(c), &params).opt()?.map(|j| Claimed {
        id: j.id,
        kind: j.kind,
        payload: j.payload,
        attempts: j.attempts,
    }))
}

/// A job that worked is gone.
pub fn complete(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::complete_job().bind(&mut Pg(c), &id)?;
    Ok(())
}

/// A job that failed: queued again after `run_after`, or failed for good, with what went wrong.
pub fn fail(c: &mut impl GenericClient, id: i64, give_up: bool, run_after: i64, error: &str) -> Result<(), Error> {
    let status = if give_up { "failed" } else { "queued" };
    q::retry_job().params(&mut Pg(c), &q::RetryJobParams { id, status, run_after, error })?;
    Ok(())
}
