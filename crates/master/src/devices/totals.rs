//! The totals of a device (files, bytes, files in the trash) kept on its row. They are not live: a write marks them
//! stale, and they are added up again when the device is done syncing, or by the minute job for a device that is not
//! syncing (a local folder, a source, a change made in the web app).

use crate::platform::db;
use crate::repo::devices;
use anyhow::Result;
use fafnirx_common::unix_now;
use fafnirx_db::queries as sql;
use postgres::Client;

/// A device that reported progress this recently is still syncing.
const SYNCING_SECS: i64 = 60;

/// Adds up the files of one device and stores the result on its row.
pub fn refresh(conn: &mut Client, device_id: i64) -> Result<()> {
    let mut tx = conn.transaction()?;
    // The row is locked first: a writer that is still committing marks it stale when it finishes, and the sums are
    // taken after that, so a change is never lost between the two.
    let Some(folder) = devices::lock(&mut tx, device_id)? else {
        return Ok(());
    };
    sql::devices::update_totals().bind(&mut tx, &device_id).map(drop)?;
    db::notify(&mut tx, db::Topic::Devices, &folder)?;
    tx.commit()?;
    Ok(())
}

/// Adds up again every device whose totals are stale and that is not in the middle of a sync.
pub fn refresh_stale(conn: &mut Client) -> Result<usize> {
    let ids = sql::devices::stale_device_ids().bind(conn, &(unix_now() - SYNCING_SECS)).all()?;
    for id in &ids {
        refresh(conn, *id)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
#[path = "../../tests/unit/devices/totals.rs"]
mod tests;
