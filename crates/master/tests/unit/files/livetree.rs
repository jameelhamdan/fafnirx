use super::*;
use crate::platform::db::testing;
use fafnirx_common::{chunk_hash, root_hash};
use std::fs;

fn put(store: &Store, path: &str, data: &[u8]) -> PathBuf {
    let p = store.live_path("admin/f", path).unwrap();
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, data).unwrap();
    p
}

fn live(c: &mut Client) -> Vec<(String, Vec<u8>)> {
    c.query(
        "SELECT f.path, v.root_hash FROM files f JOIN versions v ON v.id = f.current_version_id
             WHERE f.deleted_at IS NULL ORDER BY f.path",
        &[],
    )
    .unwrap()
    .iter()
    .map(|r| (r.get(0), r.get(1)))
    .collect()
}

#[test]
fn files_added_changed_and_removed_directly_are_tracked() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (dev, none) = (testing::folder(), Duration::ZERO);
    let root = |d: &[u8]| root_hash(&[chunk_hash(d)], d.len() as u64).to_vec();

    let p = put(&store, "docs/a.txt", b"one");
    assert_eq!(
        sync_subtree(&mut c, &store, &dev, "", Duration::from_secs(3600)).unwrap(),
        (0, 0),
        "still being written"
    );
    assert_eq!(sync_subtree(&mut c, &store, &dev, "", none).unwrap(), (1, 0));
    assert_eq!(live(&mut c), [("docs/a.txt".to_string(), root(b"one"))]);
    assert_eq!(sync_subtree(&mut c, &store, &dev, "", none).unwrap(), (0, 0), "nothing changed");

    fs::write(&p, b"two!").unwrap();
    assert_eq!(sync_subtree(&mut c, &store, &dev, "", none).unwrap(), (1, 0));
    assert_eq!(live(&mut c), [("docs/a.txt".to_string(), root(b"two!"))]);
    let versions: i64 = c.query_one("SELECT COUNT(*) FROM versions", &[]).unwrap().get(0);
    assert_eq!(versions, 2);

    fs::remove_file(&p).unwrap();
    assert_eq!(sync_subtree(&mut c, &store, &dev, "", none).unwrap(), (0, 1));
    assert!(live(&mut c).is_empty());
    let actions: Vec<String> = c.query("SELECT action FROM events ORDER BY id", &[]).unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(actions, ["add", "update", "delete"]);
}

#[test]
fn a_file_moved_in_a_local_folder_is_a_rename() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (dev, none) = (testing::folder(), Duration::ZERO);
    put(&store, "Show/a.txt", b"episode one");
    assert_eq!(sync_subtree(&mut c, &store, &dev, "", none).unwrap(), (1, 0));

    let from = store.live_path("admin/f", "Show/a.txt").unwrap();
    let to = store.live_path("admin/f", "Show 2/a.txt").unwrap();
    fs::create_dir_all(to.parent().unwrap()).unwrap();
    fs::rename(from, to).unwrap();
    assert_eq!(sync_subtree(&mut c, &store, &dev, "", none).unwrap(), (1, 0), "moved, not deleted");

    let events: Vec<(String, Option<String>)> = c
        .query("SELECT action, detail FROM events ORDER BY id", &[])
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    assert_eq!(events[1], ("rename".to_string(), Some("Show/a.txt".to_string())));
    assert_eq!(events.len(), 2, "no delete");
}

#[test]
fn only_folders_of_users_are_written_to_in_place() {
    let db = testing::db_with_device();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    fs::create_dir_all(store.live_dir("admin/d")).unwrap();
    assert!(
        user_folder(&mut c, &store, "admin/d").unwrap().is_none(),
        "the directory of a device is not a folder, and is not changed from here"
    );
    fs::create_dir_all(store.live_dir("admin/focs")).unwrap();
    assert!(
        user_folder(&mut c, &store, "admin/focs").unwrap().is_some(),
        "a directory nobody owns becomes a folder of the user"
    );
}

#[test]
fn decomposed_names_are_renamed_to_their_composed_form() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    put(&store, "café/menu.txt", b"x");
    assert_eq!(sync_subtree(&mut c, &store, &testing::folder(), "", Duration::ZERO).unwrap(), (1, 0));
    let paths: Vec<String> = live(&mut c).into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, ["café/menu.txt"]);
    assert!(store.live_path("admin/f", "café/menu.txt").unwrap().is_file());
}

#[test]
fn an_event_for_one_path_only_looks_at_that_path() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (dev, none) = (testing::folder(), Duration::ZERO);
    put(&store, "a/x", b"1");
    put(&store, "a/y", b"2");
    put(&store, "b/z", b"3");
    assert_eq!(sync_subtree(&mut c, &store, &dev, "a/x", none).unwrap(), (1, 0), "one file");
    assert_eq!(sync_subtree(&mut c, &store, &dev, "a", none).unwrap(), (1, 0), "x is known, y is new");
    assert_eq!(live(&mut c).len(), 2, "b/z was not looked at");

    fs::remove_dir_all(store.live_path("admin/f", "a").unwrap()).unwrap();
    assert_eq!(sync_subtree(&mut c, &store, &dev, "a", none).unwrap(), (0, 2), "a removed folder removes its files");
    assert_eq!(sync_subtree(&mut c, &store, &dev, "", none).unwrap(), (1, 0));
    assert_eq!(live(&mut c).len(), 1);
}

#[test]
fn events_are_mapped_to_a_path_of_the_live_tree() {
    let root = Path::new("data").join("files");
    assert_eq!(locate(&root, &root.join("bob").join("pc").join("a.txt")), Some("bob/pc/a.txt".into()));
    assert_eq!(locate(&root, &root.join("scans")), Some("scans".into()));
    assert_eq!(locate(&root, &root), None);
    assert_eq!(locate(&root, Path::new("elsewhere/pc/a")), None);
}

#[test]
fn a_new_folder_in_a_users_folder_becomes_their_folder_and_bad_names_are_ignored() {
    let db = testing::db();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    fs::create_dir_all(store.live_dir("admin/camera")).unwrap();
    fs::create_dir_all(store.live_dir("admin/Bad Name")).unwrap_or_default();
    fs::create_dir_all(store.live_dir("nobody/camera")).unwrap();
    let dev = user_folder(&mut c, &store, "admin/camera").unwrap().unwrap();
    let row = c.query_one("SELECT kind, user_id FROM folders WHERE id = $1", &[&dev.id]).unwrap();
    assert_eq!((row.get::<_, String>(0), row.get::<_, i64>(1)), ("user".into(), 1));
    assert_eq!(
        user_folder(&mut c, &store, "admin/camera").unwrap().unwrap().id,
        dev.id,
        "found again, not duplicated"
    );
    assert!(user_folder(&mut c, &store, "admin/Bad Name").unwrap().is_none());
    assert!(user_folder(&mut c, &store, "admin/missing").unwrap().is_none());
    assert!(user_folder(&mut c, &store, "nobody/camera").unwrap().is_none(), "there is no such user");
    c.execute("UPDATE users SET role = 'readonly'", &[]).unwrap();
    fs::create_dir_all(store.live_dir("admin/second")).unwrap();
    assert!(
        user_folder(&mut c, &store, "admin/second").unwrap().is_none(),
        "a user who can only read has no folders"
    );
}

#[test]
fn a_folder_of_the_master_files_is_a_master_folder() {
    let db = testing::db();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    for (path, data) in [("loose.txt", &b"1"[..]), ("scans/2024/a.txt", b"2"), ("users/admin/laptop/b.txt", b"3")] {
        let p = store.live_root().join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    }
    // The folder of the user is not a master folder: its subfolder is a folder of the user. A file in no folder is not recorded.
    scan(&db, &store, Duration::ZERO).unwrap();
    let paths: Vec<String> = live(&mut c).into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, ["2024/a.txt", "b.txt"]);
    let folders: Vec<String> = crate::repo::folders::master_folders(&mut *c).unwrap().into_iter().map(|(_, f)| f).collect();
    assert_eq!(folders, ["master:scans"], "a top-level folder became one, `users` did not");

    let (found, rel) = resolve(&mut c, &store, "scans/2024/a.txt").unwrap().unwrap();
    assert_eq!((found.key.as_str(), rel.as_str()), ("master:scans", "2024/a.txt"));
    assert!(
        resolve(&mut c, &store, "users/admin").unwrap().is_none(),
        "the folder of a user is not a master folder"
    );
    assert!(resolve(&mut c, &store, "loose.txt").unwrap().is_none(), "a file in no folder is not recorded");

    // Not inside another master folder, nor around one.
    fs::create_dir_all(store.live_root().join("scans/inner")).unwrap();
    assert!(adopt_master_dir(&mut c, &store, "scans").unwrap().is_none());
}

#[test]
fn what_the_layout_leaves_out_is_reported_until_it_is_put_right() {
    let db = testing::db();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let live = store.live_root().to_path_buf();
    let write = |path: &str| {
        let p = live.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    };
    write("users/stray.txt");
    write("users/ghost/a.txt"); // a folder that is not a user
    write("users/admin/loose.txt"); // not in a device folder
    write("users/admin/.DS_Store"); // hidden: not mentioned
    write("users/admin/Bad Name/b.txt"); // a name no device can have
    write("users/admin/camera/ok.txt"); // a device: fine
    write("loose.txt"); // in no folder
    scan(&db, &store, Duration::ZERO).unwrap();

    let open = |c: &mut Client, kind: &str| -> Option<String> { alerts::open(c).unwrap().into_iter().find(|a| a.kind == kind).map(|a| a.message) };
    let misplaced = open(&mut c, "misplaced_files").expect("an alert for the files outside device folders");
    for named in ["loose.txt", "users/stray.txt", "users/ghost", "users/admin/loose.txt", "users/admin/Bad Name"] {
        assert!(misplaced.contains(named), "{named} in: {misplaced}");
    }
    assert!(!misplaced.contains("camera") && !misplaced.contains("DS_Store"), "{misplaced}");

    fs::remove_file(live.join("loose.txt")).unwrap();
    fs::remove_file(live.join("users/stray.txt")).unwrap();
    fs::remove_dir_all(live.join("users/ghost")).unwrap();
    fs::remove_file(live.join("users/admin/loose.txt")).unwrap();
    fs::remove_dir_all(live.join("users/admin/Bad Name")).unwrap();
    scan(&db, &store, Duration::ZERO).unwrap();
    assert_eq!(open(&mut c, "misplaced_files"), None);
}

#[test]
fn a_missing_device_folder_deletes_nothing() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let dev = testing::folder();
    put(&store, "a", b"x");
    sync_subtree(&mut c, &store, &dev, "", Duration::ZERO).unwrap();
    fs::remove_dir_all(store.live_dir("admin/f")).unwrap();
    assert_eq!(sync_subtree(&mut c, &store, &dev, "", Duration::ZERO).unwrap(), (0, 0));
    assert_eq!(live(&mut c).len(), 1);
}

#[test]
fn files_committed_by_a_client_are_left_alone() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let dev = testing::folder();
    let staged = store.temp_path();
    fs::write(&staged, b"from a client").unwrap();
    let v = ingest::NewVersion {
        folder: &dev,
        path: "c.txt",
        root: root_hash(&[chunk_hash(b"from a client")], 13),
        size: 13,
        mtime: 1_700_000_000_000_000_000,
        mode: 0,
        renamed_from: None,
    };
    ingest::commit_version(&mut c, &store, &staged, &v).unwrap();
    assert_eq!(sync_subtree(&mut c, &store, &dev, "", Duration::ZERO).unwrap(), (0, 0));
    let versions: i64 = c.query_one("SELECT COUNT(*) FROM versions", &[]).unwrap().get(0);
    assert_eq!(versions, 1);
}
