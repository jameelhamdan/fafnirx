//! The queue of background work (`library::meta`): reading metadata, making thumbnails. Look at it and
//! steer it with `monitoring::queue`.

use fafnirx_common::unix_now;
use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::jobs as q;
use postgres::Error;
use uuid::Uuid;

/// How long a job that worked stays in the list.
pub const DONE_KEEP_SECS: i64 = 24 * 3600;

text_enum! {
    /// What a job does. The column is text and the web app is told the name (`as_str`).
    pub enum JobKind {
        /// Reads the metadata of a file.
        Meta = "meta",
        /// Makes the picture of a file that somebody waits for.
        Thumb = "thumb",
        /// A run of a source (`repo::links` queues and claims these).
        Source = "source",
        /// A run of a mirror.
        Mirror = "mirror",
    }
}

text_enum! {
    /// Where a job is: waiting, running, failed for good or done.
    #[derive(serde::Serialize)]
    #[serde(rename_all = "lowercase")]
    pub enum JobStatus {
        Queued = "queued",
        Running = "running",
        Failed = "failed",
        Done = "done",
    }
}

/// Queues background work of a kind, with its payload as JSON; the same work that is already waiting is not queued again.
pub fn enqueue(c: &mut impl GenericClient, kind: JobKind, payload: &serde_json::Value) -> Result<(), Error> {
    q::enqueue_job().bind(c, &kind.as_str(), &payload.to_string(), &unix_now()).map(drop)
}

/// A job taken from the queue.
pub struct Claimed {
    pub id: Uuid,
    /// `Err` with the text of a kind this version does not know.
    pub kind: Result<JobKind, String>,
    pub payload: String,
    /// How many times it has been started, this time included.
    pub attempts: i64,
}

/// How the last job of this kind with this payload went (queued, running, failed or done), if there was one. It is failed
/// only when it failed since `since` (unix seconds): after that it is worth another try.
pub fn status_of(c: &mut impl GenericClient, kind: JobKind, payload: &serde_json::Value, since: i64) -> Result<Option<JobStatus>, Error> {
    let status: Option<String> = q::job_status_of().bind(c, &since, &kind.as_str(), &payload.to_string()).opt()?;
    Ok(status.as_deref().and_then(JobStatus::parse))
}

/// Takes one due job from the queue and marks it running. One statement, so a job is only taken once.
/// A picture somebody is waiting for goes first.
pub fn claim(c: &mut impl GenericClient, now: i64) -> Result<Option<Claimed>, Error> {
    if let Some(j) = q::claim_thumb_job().bind(c, &now).opt()? {
        return Ok(Some(Claimed {
            id: j.id,
            kind: JobKind::parse(&j.kind).ok_or(j.kind),
            payload: j.payload,
            attempts: j.attempts,
        }));
    }
    Ok(q::claim_job()
        .bind(c, &now)
        .opt()?
        .map(|j| Claimed { id: j.id, kind: JobKind::parse(&j.kind).ok_or(j.kind), payload: j.payload, attempts: j.attempts }))
}

/// A job that worked is done; it stays in the list for a day.
pub fn complete(c: &mut impl GenericClient, id: Uuid, note: Option<&str>) -> Result<(), Error> {
    q::complete_job().bind(c, &unix_now(), &note, &id).map(drop)
}

/// A job that failed: queued again after `run_after`, or failed for good, with what went wrong.
pub fn fail(c: &mut impl GenericClient, id: Uuid, give_up: bool, run_after: i64, error: &str) -> Result<(), Error> {
    let status = if give_up { JobStatus::Failed } else { JobStatus::Queued }.as_str();
    q::retry_job()
        .params(c, &q::RetryJobParams { id, status, run_after, error, now: unix_now() })
        .map(drop)
}

/// Forgets the jobs that worked more than a day ago.
pub fn purge_done(c: &mut impl GenericClient) -> Result<u64, Error> {
    q::purge_done_jobs().bind(c, &(unix_now() - DONE_KEEP_SECS))
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
    /// For a run of a source or a mirror: which one.
    pub link: Option<LinkOf>,
}

/// The source or mirror a run is of.
pub struct LinkOf {
    pub name: String,
    /// The user a source belongs to; none for a mirror.
    pub owner: Option<String>,
    /// The folder a source writes into, or the one a mirror holds.
    pub folder: Option<String>,
    pub kind: String,
}

/// How many jobs there are of each kind in each state, for the jobs `(all, user)` can see (see `Access::scope_args`).
pub fn counts(c: &mut impl GenericClient, all: bool, user: i64) -> Result<Vec<Count>, Error> {
    let params = q::QueueCountsParams { since: unix_now() - DONE_KEEP_SECS, all, scope_user: user };
    Ok(q::queue_counts()
        .params(c, &params)
        .all()?
        .into_iter()
        .map(|r| Count { kind: r.kind, status: r.status, n: r.n })
        .collect())
}

/// The jobs worth looking at: running, failed and waiting first, then those done lately.
pub fn items(c: &mut impl GenericClient, all: bool, user: i64, limit: i64) -> Result<Vec<Item>, Error> {
    let params = q::QueueItemsParams { since: unix_now() - DONE_KEEP_SECS, all, scope_user: user, limit };
    Ok(q::queue_items()
        .params(c, &params)
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
            link: j
                .link_name
                .map(|name| LinkOf { name, owner: j.link_owner, folder: j.link_folder, kind: j.link_kind.unwrap_or_default() }),
        })
        .collect())
}

/// Failed jobs run again: this one, or all of them. Returns how many.
pub fn retry_failed(c: &mut impl GenericClient, id: Option<Uuid>) -> Result<u64, Error> {
    q::retry_failed_jobs().bind(c, &id)
}

/// Forgets jobs that have the state `status` (`failed` or `queued`): this one, or all of them. Returns how many.
pub fn drop_where(c: &mut impl GenericClient, status: JobStatus, id: Option<Uuid>) -> Result<u64, Error> {
    q::drop_jobs().bind(c, &status.as_str(), &id)
}

/// A photo or video to read: (file id, folder, path, content hash).
pub type ToRead = (Uuid, String, String, Vec<u8>);

/// The photos and videos to read again, newest first, from the file after `after` on: see `ToRead`.
pub fn media_to_read(c: &mut impl GenericClient, everything: bool, after: Option<Uuid>, limit: i64) -> Result<Vec<ToRead>, Error> {
    let params = q::MediaToReadParams { everything, after, limit };
    Ok(q::media_to_read()
        .params(c, &params)
        .all()?
        .into_iter()
        .map(|r| (r.id, r.folder, r.path, r.root_hash))
        .collect())
}

/// The unfinished work of a file as (kind, status): queued, running or failed.
pub fn pending_of(c: &mut impl GenericClient, folder: &str, path: &str) -> Result<Vec<(String, JobStatus)>, Error> {
    Ok(q::pending_jobs_of_file()
        .bind(c, &folder, &path)
        .all()?
        .into_iter()
        .filter_map(|j| JobStatus::parse(&j.status).map(|s| (j.kind, s)))
        .collect())
}
