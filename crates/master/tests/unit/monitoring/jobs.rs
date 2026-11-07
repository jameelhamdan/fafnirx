use super::*;
use crate::platform::db::testing;

#[test]
fn retention_removes_only_expired() {
    let db = testing::db_with_device();
    let mut c = db.get().unwrap();
    let old = unix_now() - 40 * 86400;
    let fresh = unix_now() - 86400;
    c.execute("INSERT INTO files (device_id, path, deleted_at) VALUES (1, 'old', $1)", &[&old]).unwrap();
    c.execute("INSERT INTO files (device_id, path, deleted_at) VALUES (1, 'fresh', $1)", &[&fresh])
        .unwrap();
    c.execute("INSERT INTO files (device_id, path) VALUES (1, 'live')", &[]).unwrap();
    c.execute(
        "INSERT INTO versions (file_id, root_hash, size, mtime, created_at)
             SELECT id, decode('00', 'hex'), 0, 0, 0 FROM files ORDER BY deleted_at NULLS LAST",
        &[],
    )
    .unwrap();
    let removed = retention(&mut c).unwrap();
    assert_eq!(removed, [1], "the version of the expired file is reported so its files can be deleted");
    let paths: Vec<String> = c
        .query("SELECT path FROM files ORDER BY deleted_at NULLS LAST", &[])
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(paths, ["fresh", "live"]);
    let versions: i64 = c.query_one("SELECT COUNT(*) FROM versions", &[]).unwrap().get(0);
    assert_eq!(versions, 2);

    // The master notes how far back it knows every deletion, and that only moves forward.
    let known_from = settings::get_i64(&mut *c, settings::DELETIONS_KNOWN_FROM, 0);
    let thirty_days_ago = unix_now() - 30 * 86400;
    assert!((thirty_days_ago - 5..=thirty_days_ago + 5).contains(&known_from), "{known_from}");
    settings::set(&mut *c, "retention_days", "90").unwrap();
    retention(&mut c).unwrap();
    assert_eq!(
        settings::get_i64(&mut *c, settings::DELETIONS_KNOWN_FROM, 0),
        known_from,
        "a longer period does not go back"
    );
}

#[test]
fn upload_errors_expire_after_48_hours_without_a_new_report() {
    let db = testing::db_with_device();
    let mut c = db.get().unwrap();
    let c = &mut *c;
    alerts::upsert(c, "upload_error", "d", "disk full").unwrap();
    alerts::upsert(c, "disk_usage", "", "90% full").unwrap();
    let age = |c: &mut Client, kind: &str, hours: i64| {
        c.execute(
            "UPDATE alerts SET created_at = $1, updated_at = $1 WHERE kind = $2",
            &[&(unix_now() - hours * 3600), &kind],
        )
        .unwrap();
    };

    age(c, "upload_error", 47);
    expire_quiet_alerts(c).unwrap();
    assert_eq!(testing::open_alerts(c), 2, "47 hours: still open");

    age(c, "upload_error", 49);
    age(c, "disk_usage", 500);
    expire_quiet_alerts(c).unwrap();
    assert_eq!(testing::open_alerts(c), 1, "49 hours: the upload error is gone, other alerts are untouched");

    // The client reporting the same error again keeps (and reopens) it.
    alerts::upsert(c, "upload_error", "d", "disk full").unwrap();
    age(c, "upload_error", 47);
    alerts::upsert(c, "upload_error", "d", "disk full").unwrap();
    expire_quiet_alerts(c).unwrap();
    assert_eq!(testing::open_alerts(c), 2, "a repeated report refreshes the alert");

    // Held deletions are reported with every heartbeat, so they go quiet much sooner.
    alerts::upsert(c, "deletions_held", "alice/laptop|alice/family", "12 deletions held back").unwrap();
    age(c, "deletions_held", 0);
    expire_quiet_alerts(c).unwrap();
    assert_eq!(testing::open_alerts(c), 3, "just reported");
    c.execute("UPDATE alerts SET updated_at = $1 WHERE kind = 'deletions_held'", &[&(unix_now() - 20 * 60)])
        .unwrap();
    expire_quiet_alerts(c).unwrap();
    assert_eq!(testing::open_alerts(c), 2, "20 minutes without a report: the computer no longer holds them");
}

#[test]
fn local_folders_are_never_offline() {
    let db = testing::db_with_device();
    let mut c = db.get().unwrap();
    let c = &mut *c;
    c.execute("UPDATE devices SET kind = 'local', last_seen = $1 WHERE id = 1", &[&(unix_now() - 600)])
        .unwrap();
    offline_alerts(c).unwrap();
    assert_eq!(testing::open_alerts(c), 0, "only clients send heartbeats");
}

#[test]
fn offline_alert_opens_and_resolves() {
    let db = testing::db_with_device();
    let mut c = db.get().unwrap();
    let c = &mut *c;
    c.execute("UPDATE devices SET last_seen = $1 WHERE id = 1", &[&(unix_now() - 600)]).unwrap();
    offline_alerts(c).unwrap();
    assert_eq!(testing::open_alerts(c), 1);
    c.execute("UPDATE devices SET last_seen = $1 WHERE id = 1", &[&unix_now()]).unwrap();
    offline_alerts(c).unwrap();
    assert_eq!(testing::open_alerts(c), 0);
}
