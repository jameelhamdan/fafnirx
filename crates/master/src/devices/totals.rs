//! The totals of a device (files, bytes, files in the trash) kept on its row. They are not live: a write marks them
//! stale, and they are added up again when the device is done syncing, or by the minute job for a device that is not
//! syncing (a local folder, a source, a change made in the web app).

use crate::platform::db;
use crate::repo::devices;
use anyhow::Result;
use fafnirx_common::unix_now;
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
    devices::store_totals(&mut tx, device_id, unix_now())?;
    db::notify(&mut tx, "devices", &folder)?;
    tx.commit()?;
    Ok(())
}

/// Adds up again every device whose totals are stale and that is not in the middle of a sync.
pub fn refresh_stale(conn: &mut Client) -> Result<usize> {
    let ids = devices::stale_totals(conn, unix_now() - SYNCING_SECS)?;
    for id in &ids {
        refresh(conn, *id)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::db::testing;

    #[test]
    fn totals_follow_the_files_once_they_are_refreshed() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        // The master files are another device with totals of their own; this test is about the one device.
        c.execute("UPDATE devices SET totals_stale = FALSE WHERE id = 0", &[]).unwrap();
        let totals = |c: &mut Client| -> (i64, i64, i64, bool) {
            let r = c
                .query_one("SELECT file_count, total_bytes, deleted_files, totals_stale FROM devices WHERE id = 1", &[])
                .unwrap();
            (r.get(0), r.get(1), r.get(2), r.get(3))
        };
        assert_eq!(totals(&mut c), (0, 0, 0, true), "a new device has nothing to show yet");
        for (path, size, deleted) in [("a", 10_i64, false), ("b", 5, false), ("c", 7, true)] {
            let file: i64 = c
                .query_one("INSERT INTO files (device_id, path) VALUES (1, $1) RETURNING id", &[&path])
                .unwrap()
                .get(0);
            let version: i64 = c
                .query_one(
                    "INSERT INTO versions (file_id, root_hash, size, mtime, created_at)
                     VALUES ($1, decode('00', 'hex'), $2, 0, 0) RETURNING id",
                    &[&file, &size],
                )
                .unwrap()
                .get(0);
            c.execute(
                "UPDATE files SET current_version_id = $2, deleted_at = CASE WHEN $3 THEN 1 END WHERE id = $1",
                &[&file, &version, &deleted],
            )
            .unwrap();
        }
        assert_eq!(refresh_stale(&mut c).unwrap(), 1);
        assert_eq!(totals(&mut c), (2, 15, 1, false));
        assert_eq!(refresh_stale(&mut c).unwrap(), 0, "nothing is stale any more");

        c.execute("UPDATE devices SET totals_stale = TRUE WHERE id = 1", &[]).unwrap();
        c.execute(
            "INSERT INTO device_activity (device_id, updated_at, phase) VALUES (1, $1, 'uploading')",
            &[&unix_now()],
        )
        .unwrap();
        assert_eq!(refresh_stale(&mut c).unwrap(), 0, "a device that is syncing is left alone until it is done");
    }
}
