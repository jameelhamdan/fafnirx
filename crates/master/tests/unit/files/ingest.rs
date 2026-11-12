use super::*;
use crate::platform::db::testing;
use crate::repo::files::{deleted_in_web_app, live_path_with_root};
use fafnirx_common::{chunk_hash, root_hash};

/// Writes `data` to a temporary file and commits it as `path`.
/// What a device syncing into a folder does: the names are checked first.
fn synced(c: &mut Client, store: &Store, path: &str, data: &[u8], renamed_from: Option<&str>) -> Result<u64> {
    check_names(c, &testing::folder(), path, renamed_from, true)?;
    commit(c, store, path, data, renamed_from)
}

fn commit(c: &mut Client, store: &Store, path: &str, data: &[u8], renamed_from: Option<&str>) -> Result<u64> {
    let staged = store.temp_path();
    std::fs::write(&staged, data).unwrap();
    let folder = testing::folder();
    let v = NewVersion {
        folder: &folder,
        path,
        root: root_hash(&[chunk_hash(data)], data.len() as u64),
        size: data.len() as u64,
        mtime: 0,
        mode: 0,
        renamed_from,
    };
    commit_version(c, store, &staged, &v)
}

fn live(store: &Store, path: &str) -> Option<Vec<u8>> {
    std::fs::read(store.live_path("admin/f", path).unwrap()).ok()
}

#[test]
fn a_synced_folder_refuses_paths_that_not_every_kind_of_device_can_hold() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let refusal = |r: anyhow::Result<u64>| r.unwrap_err().downcast::<crate::web::ApiError>().unwrap();

    c.execute("INSERT INTO devices (name, token_hash, created_at, user_id) VALUES ('d', 'x', 0, 1)", &[])
        .unwrap();
    c.execute("UPDATE folders SET kind = 'backup', device_id = 1 WHERE id = 1", &[]).unwrap();
    // The folder of a device keeps whatever it is sent, two spellings included.
    commit(&mut c, &store, "Notes.txt", b"one", None).unwrap();
    commit(&mut c, &store, "notes.txt", b"two", None).unwrap();

    c.execute("UPDATE folders SET kind = 'user', device_id = NULL WHERE id = 1", &[]).unwrap();
    let clash = refusal(synced(&mut c, &store, "NOTES.TXT", b"x", None));
    assert_eq!(clash.0, axum::http::StatusCode::CONFLICT, "{}", clash.1);
    let windows = refusal(synced(&mut c, &store, "what?.txt", b"x", None));
    assert_eq!(windows.0, axum::http::StatusCode::BAD_REQUEST, "{}", windows.1);
    synced(&mut c, &store, "fine.txt", b"x", None).unwrap();
    // A path that is in the folder can be updated even though a twin of it is there too.
    synced(&mut c, &store, "Notes.txt", b"updated", None).unwrap();
    assert_eq!(live(&store, "Notes.txt").unwrap(), b"updated");

    // A file can be renamed to another case; what it is renamed from does not count as a clash.
    synced(&mut c, &store, "Readme.md", b"r", None).unwrap();
    synced(&mut c, &store, "readme.md", b"r", Some("Readme.md")).unwrap();
    assert_eq!(refusal(synced(&mut c, &store, "README.md", b"x", None)).0, axum::http::StatusCode::CONFLICT);

    // The same goes for the folders above a path: one spelling at a time, and a folder is renamed file by file.
    synced(&mut c, &store, "Docs/a.txt", b"a", None).unwrap();
    synced(&mut c, &store, "Docs/b.txt", b"b", None).unwrap();
    let folder = refusal(synced(&mut c, &store, "docs/new.txt", b"x", None));
    assert_eq!(folder.0, axum::http::StatusCode::CONFLICT, "{}", folder.1);
    assert!(folder.1.contains("folder Docs"), "{}", folder.1);
    synced(&mut c, &store, "docs/a.txt", b"a", Some("Docs/a.txt")).unwrap();
    synced(&mut c, &store, "docs/b.txt", b"b", Some("Docs/b.txt")).unwrap();
    synced(&mut c, &store, "docs/new.txt", b"x", None).expect("nothing is left under the old spelling");
    assert_eq!(refusal(synced(&mut c, &store, "DOCS/other.txt", b"x", None)).0, axum::http::StatusCode::CONFLICT);
    // Deeper folders too.
    synced(&mut c, &store, "docs/Sub/x.txt", b"x", None).unwrap();
    assert_eq!(refusal(synced(&mut c, &store, "docs/sub/y.txt", b"y", None)).0, axum::http::StatusCode::CONFLICT);
}

#[test]
fn a_folder_of_a_user_and_the_master_files_check_names_only_for_what_people_add() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let folder = testing::folder();
    let refused = |r: anyhow::Result<()>| r.unwrap_err().downcast::<crate::web::ApiError>().unwrap().0;

    // What is on disk is taken as it is: the watcher cannot refuse a file, and a clash that was there stays.
    commit(&mut c, &store, "Notes.txt", b"one", None).unwrap();
    commit(&mut c, &store, "notes.txt", b"two", None).expect("not a web upload: the watcher's file is recorded");
    // What a person adds in the web app follows the rules of the devices the folder is shown to over SMB.
    assert_eq!(refused(check_names(&mut *c, &folder, "NOTES.txt", None, true)), axum::http::StatusCode::CONFLICT);
    assert_eq!(
        refused(check_names(&mut *c, &folder, "docs/a:b.txt", None, true)),
        axum::http::StatusCode::BAD_REQUEST
    );
    check_names(&mut *c, &folder, "fresh.txt", None, true).unwrap();
    check_names(&mut *c, &folder, "Notes.txt", None, true).expect("a file that is there can be replaced");
    assert_eq!(
        refused(move_path(&mut c, &store, &folder, "Notes.txt", "NOTES.txt")),
        axum::http::StatusCode::CONFLICT
    );

    // The backup of a device keeps what the device has.
    c.execute("INSERT INTO devices (name, token_hash, created_at, user_id) VALUES ('d', 'x', 0, 1)", &[])
        .unwrap();
    c.execute("UPDATE folders SET kind = 'backup', device_id = 1 WHERE id = 1", &[]).unwrap();
    check_names(&mut *c, &folder, "NOTES.TXT", None, true).unwrap();
}

#[test]
fn add_update_noop_delete_and_rename() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let folder = testing::folder();

    let v1 = commit(&mut c, &store, "a", b"one", None).unwrap();
    assert_eq!(live(&store, "a").unwrap(), b"one", "stored as a normal file under its own name");
    assert_eq!(commit(&mut c, &store, "a", b"one", None).unwrap(), v1, "same content is a no-op");
    let v2 = commit(&mut c, &store, "a", b"two", None).unwrap();
    assert!(v2 > v1);
    assert_eq!(live(&store, "a").unwrap(), b"two");
    assert_eq!(
        std::fs::read(dir.path().join(format!("versions/{v1}/admin/f/a"))).unwrap(),
        b"one",
        "the replaced file moves to its version"
    );

    let root = root_hash(&[chunk_hash(b"two")], 3);
    assert_eq!(live_path_with_root(&mut *c, 1, &root, None).unwrap().as_deref(), Some("a"));
    commit(&mut c, &store, "b", b"two", Some("a")).unwrap();
    assert!(live(&store, "a").is_none(), "the old path of a rename leaves the live tree");
    assert_eq!(live(&store, "b").unwrap(), b"two");
    assert_eq!(live_path_with_root(&mut *c, 1, &root, Some("a")).unwrap().as_deref(), Some("b"));

    assert!(mark_deleted(&mut c, &store, &folder, "b", false).unwrap());
    assert!(live(&store, "b").is_none());
    assert!(!mark_deleted(&mut c, &store, &folder, "b", false).unwrap(), "deleting twice changes nothing");
    assert!(!deleted_in_web_app(&mut *c, folder.id, "b").unwrap(), "a delete on the device is the device's own");
    let actions: Vec<String> = c.query("SELECT action FROM events ORDER BY id", &[]).unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(
        actions,
        ["add", "update", "rename", "delete"],
        "a rename is one event: the old path is vacated, not deleted"
    );
    let hidden: i64 = c
        .query_one("SELECT COUNT(*) FROM files WHERE path = 'a' AND deleted_by = 'rename'", &[])
        .unwrap()
        .get(0);
    assert_eq!(hidden, 1, "the old path is kept, marked as renamed, until retention removes it");
    let (in_trash, in_listing): (i64, i64) = (
        crate::repo::library::trash(&mut *c, crate::repo::Viewer { uid: 1, all: true, scope_user: -1 }, 10, 0)
            .unwrap()
            .iter()
            .filter(|t| t.path == "a")
            .count() as i64,
        crate::repo::files::expand(&mut *c, 1, "a", true).unwrap().len() as i64,
    );
    assert_eq!((in_trash, in_listing), (0, 0), "a renamed file is not in the trash, nor listed as deleted");

    commit(&mut c, &store, "b", b"back", None).unwrap();
    assert_eq!(live(&store, "b").unwrap(), b"back", "a deleted path can be added again");

    assert!(mark_deleted(&mut c, &store, &folder, "b", true).unwrap());
    assert!(deleted_in_web_app(&mut *c, folder.id, "b").unwrap(), "a delete in the web app is remembered");
    commit(&mut c, &store, "b", b"restored", None).unwrap();
    assert!(!deleted_in_web_app(&mut *c, folder.id, "b").unwrap(), "and ends when the file is brought back");

    // A file that a source collected is remembered as removed in the web app in the source's own record.
    c.execute(
        "INSERT INTO links (direction, name, kind, config, folder_id, user_id, interval_secs, created_at) VALUES ('in', 's', 'rclone', '{}', 1, 1, 3600, 0)",
        &[],
    )
    .unwrap();
    c.execute(
        "INSERT INTO link_items (link_id, folder_id, path, remote_id, version) SELECT id, 1, 'b', 'r', 'm' FROM links",
        &[],
    )
    .unwrap();
    let flag = |c: &mut postgres::Client| -> bool { c.query_one("SELECT removed_in_web_app FROM link_items", &[]).unwrap().get(0) };
    assert!(mark_deleted(&mut c, &store, &folder, "b", false).unwrap());
    assert!(!flag(&mut c), "a delete by the source itself is not one made in the web app");
    commit(&mut c, &store, "b", b"again", None).unwrap();
    assert!(mark_deleted(&mut c, &store, &folder, "b", true).unwrap());
    assert!(flag(&mut c));
}

#[test]
fn a_failed_install_leaves_the_current_file_alone() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    commit(&mut c, &store, "a", b"one", None).unwrap();

    let missing = store.temp_path();
    let folder = testing::folder();
    let v = NewVersion {
        folder: &folder,
        path: "a",
        root: root_hash(&[chunk_hash(b"two")], 3),
        size: 3,
        mtime: 0,
        mode: 0,
        renamed_from: None,
    };
    assert!(commit_version(&mut c, &store, &missing, &v).is_err());
    assert_eq!(live(&store, "a").unwrap(), b"one");
    let versions: i64 = c.query_one("SELECT COUNT(*) FROM versions", &[]).unwrap().get(0);
    assert_eq!(versions, 1, "no version row for the failed commit");
}

#[test]
fn only_the_newest_versions_stay_unpruned() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    for n in 0..7 {
        commit(&mut c, &store, "a", format!("v{n}").as_bytes(), None).unwrap();
    }
    let live: i64 = c.query_one("SELECT COUNT(*) FROM versions WHERE pruned_at IS NULL", &[]).unwrap().get(0);
    assert_eq!(live, 5, "version_count defaults to 5");
}

#[test]
fn images_queue_a_metadata_job_once_per_content() {
    let db = testing::db_with_folder();
    let mut c = db.get().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    commit(&mut c, &store, "photos/a.jpg", b"jpeg", None).unwrap();
    commit(&mut c, &store, "notes.txt", b"text", None).unwrap();
    let jobs: i64 = c.query_one("SELECT COUNT(*) FROM jobs WHERE kind = 'meta'", &[]).unwrap().get(0);
    assert_eq!(jobs, 1, "only the image");
}
