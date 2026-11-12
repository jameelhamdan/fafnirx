//! The totals of a folder (files, bytes, files in the trash) kept on its row. They are not live: a write marks them
//! stale, and they are added up again when its folder is done syncing, or by the minute job for a folder whose folder is not
//! syncing (a folder of a user, a source, a change made in the web app).

use crate::platform::db;
use crate::repo::folders;
use anyhow::Result;
use fafnirx_common::unix_now;
use fafnirx_db::queries as sql;
use postgres::Client;

/// A folder that reported progress this recently is still syncing.
const SYNCING_SECS: i64 = 60;

/// Adds up the files of one folder and stores the result on its row.
pub fn refresh(conn: &mut Client, folder_id: i64) -> Result<()> {
    let mut tx = conn.transaction()?;
    // The row is locked first: a writer that is still committing marks it stale when it finishes, and the sums are
    // taken after that, so a change is never lost between the two.
    let Some(key) = folders::lock(&mut tx, folder_id)? else {
        return Ok(());
    };
    sql::folders::update_totals().bind(&mut tx, &folder_id).map(drop)?;
    db::notify(&mut tx, db::Topic::Folders, &key)?;
    tx.commit()?;
    Ok(())
}

/// Adds up again every folder whose totals are stale and whose folder is not in the middle of a sync.
pub fn refresh_stale(conn: &mut Client) -> Result<usize> {
    let ids = sql::folders::stale_folder_ids().bind(conn, &(unix_now() - SYNCING_SECS)).all()?;
    for id in &ids {
        refresh(conn, *id)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
#[path = "../../tests/unit/folders/totals.rs"]
mod tests;
