//! Removing what has outlived the retention period: deleted files, pruned versions, old events.

use fafnirx_db::client::GenericClient;
use fafnirx_db::queries::retention as q;
use postgres::Error;
use uuid::Uuid;

/// Locks up to `limit` of the files deleted before `cutoff` and returns (file ids, their device ids). A file that came back in
/// the meantime is no longer expired once its lock is released, so it keeps its history.
pub fn lock_expired_files(c: &mut impl GenericClient, cutoff: i64, limit: i64) -> Result<(Vec<Uuid>, Vec<i64>), Error> {
    let rows = q::lock_expired_files().bind(c, &cutoff, &limit).all()?;
    Ok(rows.into_iter().map(|r| (r.id, r.device_id)).unzip())
}

/// Removes the versions of these files and returns their ids, whose stored files the caller deletes.
pub fn delete_versions_of(c: &mut impl GenericClient, file_ids: &[Uuid]) -> Result<Vec<i64>, Error> {
    q::delete_versions_of_files().bind(c, &file_ids).all()
}

/// Removes these files. Returns how many were removed.
pub fn delete_files(c: &mut impl GenericClient, file_ids: &[Uuid]) -> Result<u64, Error> {
    q::delete_files().bind(c, &file_ids)
}
