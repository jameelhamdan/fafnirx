//! The queue of background work (`library::meta`): reading metadata, making thumbnails and playable copies.

use crate::platform::pg::Pg;
use fafnirx_common::unix_now;
use fafnirx_db::queries::jobs as q;
use postgres::{Error, GenericClient};

/// Queues background work of a kind, with its payload as JSON.
pub fn enqueue(c: &mut impl GenericClient, kind: &str, payload: &serde_json::Value) -> Result<(), Error> {
    q::enqueue_job().bind(&mut Pg(c), &kind, &payload.to_string(), &unix_now())?;
    Ok(())
}
