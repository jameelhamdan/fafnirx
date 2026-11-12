use super::*;
use crate::platform::db::testing;

#[test]
fn totals_follow_the_files_once_they_are_refreshed() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let totals = |c: &mut Client| -> (i64, i64, i64, bool) {
        let r = c
            .query_one("SELECT file_count, total_bytes, deleted_files, totals_stale FROM folders WHERE id = 1", &[])
            .unwrap();
        (r.get(0), r.get(1), r.get(2), r.get(3))
    };
    assert_eq!(totals(&mut c), (0, 0, 0, true), "a new folder has nothing to show yet");
    for (path, size, deleted) in [("a", 10_i64, false), ("b", 5, false), ("c", 7, true)] {
        let file: uuid::Uuid = c
            .query_one("INSERT INTO files (folder_id, path) VALUES (1, $1) RETURNING id", &[&path])
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

    // The folder is a directory of a device, which is syncing.
    c.execute("INSERT INTO devices (name, token_hash, created_at, user_id) VALUES ('d', 'x', 0, 1)", &[])
        .unwrap();
    c.execute("UPDATE folders SET kind = 'backup', device_id = 1, totals_stale = TRUE WHERE id = 1", &[])
        .unwrap();
    c.execute(
        "INSERT INTO device_activity (device_id, updated_at, phase) VALUES (1, $1, 'uploading')",
        &[&unix_now()],
    )
    .unwrap();
    assert_eq!(refresh_stale(&mut c).unwrap(), 0, "a folder whose device is syncing is left alone until it is done");
}
