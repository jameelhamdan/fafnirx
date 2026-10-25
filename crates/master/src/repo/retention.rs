//! Removing what has outlived the retention period: deleted files, pruned versions, old events.

use crate::platform::pg::Pg;
use fafnirx_db::queries::retention as q;
use postgres::{Error, GenericClient};

/// Locks the files deleted before `cutoff` and returns (file ids, their device ids). A file that came back in the
/// meantime is no longer expired once its lock is released, so it keeps its history.
pub fn lock_expired_files(c: &mut impl GenericClient, cutoff: i64) -> Result<(Vec<i64>, Vec<i64>), Error> {
    let rows = q::lock_expired_files().bind(&mut Pg(c), &cutoff).all()?;
    Ok(rows.into_iter().map(|r| (r.id, r.device_id)).unzip())
}

/// Removes the versions of these files and returns their ids, whose stored files the caller deletes.
pub fn delete_versions_of(c: &mut impl GenericClient, file_ids: &[i64]) -> Result<Vec<i64>, Error> {
    q::delete_versions_of_files().bind(&mut Pg(c), &file_ids).all()
}

/// Removes these files. Returns how many were removed.
pub fn delete_files(c: &mut impl GenericClient, file_ids: &[i64]) -> Result<u64, Error> {
    q::delete_files().bind(&mut Pg(c), &file_ids)
}

/// Removes the versions pruned before `cutoff` and returns their ids.
pub fn delete_pruned_versions(c: &mut impl GenericClient, cutoff: i64) -> Result<Vec<i64>, Error> {
    q::delete_pruned_versions().bind(&mut Pg(c), &cutoff).all()
}

/// The ids of every version the database knows, to find stored files that no record points to.
pub fn all_version_ids(c: &mut impl GenericClient) -> Result<Vec<i64>, Error> {
    q::all_version_ids().bind(&mut Pg(c)).all()
}
