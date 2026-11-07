use super::*;

#[test]
fn every_topic_is_named_once_and_found_by_its_name() {
    for topic in Topic::ALL {
        assert_eq!(Topic::parse(topic.name()), Some(topic));
    }
    assert_eq!(Topic::parse("nothing"), None);
    assert_eq!(Topic::Users.reach(), Reach::Admins);
}

#[test]
fn a_folder_pattern_takes_wildcards_literally() {
    assert_eq!(like_prefix("photos/2024/"), "photos/2024/%");
    assert_eq!(like_prefix("100%_sure\\"), r"100\%\_sure\\%");
    assert_eq!(like_prefix(""), "%");

    let db = testing::db_with_device();
    let mut c = db.get().unwrap();
    for path in ["a_b/x", "aXb/x", "a%/y", "abc/z", "a_b2/w"] {
        c.execute("INSERT INTO files (device_id, path) VALUES (1, $1)", &[&path]).unwrap();
    }
    let mut below = |folder: &str| -> Vec<String> {
        let rows = c.query("SELECT path FROM files WHERE path LIKE $1 ORDER BY path", &[&like_prefix(folder)]);
        rows.unwrap().iter().map(|r| r.get(0)).collect()
    };
    assert_eq!(below("a_b/"), ["a_b/x"], "_ is not a wildcard");
    assert_eq!(below("a%/"), ["a%/y"], "% is not a wildcard");
}

#[test]
fn the_indexes_behind_the_lookups_exist() {
    let db = testing::db();
    let mut c = db.get().unwrap();
    for index in [
        "files_path_search",
        "files_path_prefix",
        "files_current_version",
        "files_deleted",
        "files_library",
        "versions_root",
        "versions_pruned",
        "events_path",
        "events_ts",
        "devices_token",
        "jobs_queue",
    ] {
        let found: bool = c.query_one("SELECT to_regclass($1) IS NOT NULL", &[&index]).unwrap().get(0);
        assert!(found, "missing index {index}");
    }
}
