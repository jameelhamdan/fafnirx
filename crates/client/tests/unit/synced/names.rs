use super::*;

#[tokio::test]
async fn a_rename_on_the_master_moves_the_file_instead_of_receiving_it_again() {
    let mut rig = Rig::new().await;
    rig.remote_has("old.txt", b"same content");
    rig.cycle().await;
    {
        let mut h = rig.held.lock().unwrap();
        h.files.remove("old.txt");
        h.files.insert("new.txt".into(), b"same content".to_vec());
        h.changes = vec![master::gone("old.txt"), master::live("new.txt", b"same content")];
    }
    let report = rig.cycle().await;
    assert_eq!(rig.local("new.txt").as_deref(), Some("same content"));
    assert!(rig.local("old.txt").is_none());
    assert_eq!((report.downloaded, report.conflicts, report.failed), (1, 0, 0));
    assert!(rig.held.lock().unwrap().deleted.is_empty(), "nothing is deleted on the master by moving");
}

#[tokio::test]
async fn a_rename_that_only_changes_the_case_is_a_move_on_every_kind_of_computer() {
    let mut rig = Rig::new().await;
    rig.remote_has("Readme.md", b"same content");
    rig.cycle().await;
    {
        let mut h = rig.held.lock().unwrap();
        h.files.remove("Readme.md");
        h.files.insert("readme.md".into(), b"same content".to_vec());
        h.changes = vec![master::gone("Readme.md"), master::live("readme.md", b"same content")];
    }
    let report = rig.cycle().await;
    assert_eq!(rig.names(), ["readme.md"], "spelled as the master has it, not left under the old spelling");
    assert_eq!((report.downloaded, report.conflicts, report.failed), (1, 0, 0));
    assert!(rig.held.lock().unwrap().deleted.is_empty());
}

#[tokio::test]
async fn a_folder_renamed_to_another_case_is_renamed_here_too_with_everything_in_it() {
    let mut rig = Rig::new().await;
    rig.remote_has("Trip/a.txt", b"first");
    rig.cycle().await;
    rig.remote_has("Trip/b.txt", b"second");
    rig.cycle().await;
    {
        let mut h = rig.held.lock().unwrap();
        h.files.clear();
        h.files.insert("trip/a.txt".into(), b"first".to_vec());
        h.files.insert("trip/b.txt".into(), b"second".to_vec());
        h.changes = vec![
            master::gone("Trip/a.txt"),
            master::gone("Trip/b.txt"),
            master::live("trip/a.txt", b"first"),
            master::live("trip/b.txt", b"second"),
        ];
    }
    let report = rig.cycle().await;
    assert_eq!(rig.names(), ["trip"], "one folder, spelled as the master has it");
    assert_eq!(rig.local("trip/a.txt").as_deref(), Some("first"));
    assert_eq!(
        rig.local("trip/b.txt").as_deref(),
        Some("second"),
        "the second file is not deleted as the old spelling of itself"
    );
    assert_eq!((report.conflicts, report.failed, report.deleted), (0, 0, 0));
    let held = rig.held.lock().unwrap();
    assert!(held.commits.is_empty() && held.deleted.is_empty(), "and nothing goes back under the old spelling");
}

#[tokio::test]
async fn a_file_deleted_in_a_folder_renamed_to_another_case_is_deleted_there() {
    let mut rig = Rig::new().await;
    rig.remote_has("Trip/a.txt", b"first");
    rig.cycle().await;
    rig.remote_has("Trip/b.txt", b"second");
    rig.cycle().await;
    {
        let mut h = rig.held.lock().unwrap();
        h.files.clear();
        h.files.insert("trip/a.txt".into(), b"first".to_vec());
        h.changes = vec![master::gone("Trip/a.txt"), master::gone("Trip/b.txt"), master::live("trip/a.txt", b"first")];
    }
    let report = rig.cycle().await;
    assert_eq!(rig.names(), ["trip"]);
    assert_eq!(rig.local("trip/a.txt").as_deref(), Some("first"));
    assert_eq!(rig.local("trip/b.txt"), None, "deleted on the master, where the folder is now spelled otherwise");
    assert_eq!(report.deleted, 1);
    assert!(rig.held.lock().unwrap().commits.is_empty(), "and not sent back");
}

#[tokio::test]
async fn a_name_that_differs_only_by_case_from_one_here_is_not_received() {
    let mut rig = Rig::new().await;
    std::fs::write(rig.folder.path().join("Notes.txt"), "mine").unwrap();
    rig.remote_has("notes.txt", b"theirs");
    let report = rig.cycle().await;
    assert_eq!((report.downloaded, report.conflicts), (0, 0));
    assert_eq!(rig.names(), ["Notes.txt"], "one name, not two, whatever the file system");
    assert_eq!(rig.local("Notes.txt").as_deref(), Some("mine"));
}

#[tokio::test]
async fn names_windows_cannot_keep_are_not_received_anywhere() {
    let mut rig = Rig::new().await;
    {
        let mut h = rig.held.lock().unwrap();
        h.files.insert("a:b.txt".into(), b"x".to_vec());
        h.files.insert("ok.txt".into(), b"y".to_vec());
        h.changes = vec![master::live("a:b.txt", b"x"), master::live("ok.txt", b"y")];
    }
    let report = rig.cycle().await;
    assert_eq!((report.downloaded, report.failed), (1, 0), "skipped, not failed: nothing to retry");
    assert_eq!(rig.names(), ["ok.txt"]);
}

#[test]
fn another_spelling_is_found_by_reading_the_folders() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("Docs")).unwrap();
    std::fs::write(dir.path().join("Docs").join("A.txt"), "x").unwrap();
    let root = dir.path();
    let mut seen = Spellings::new(root);
    assert_eq!(seen.clash_with("docs/a.txt"), Some(root.join("Docs").join("A.txt")));
    assert_eq!(seen.clash_with("Docs/a.txt"), Some(root.join("Docs").join("A.txt")), "the file alone");
    assert_eq!(seen.clash_with("docs/new.txt"), Some(root.join("Docs").join("new.txt")), "a folder above it");
    assert_eq!(seen.clash_with("Docs/A.txt"), None, "the same spelling is the same file");
    assert_eq!(seen.clash_with("Docs/new.txt"), None);
    assert_eq!(seen.clash_with("other/a.txt"), None);
}

#[test]
fn what_was_read_follows_the_files_that_are_received_and_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut seen = Spellings::new(root);
    assert_eq!(seen.clash_with("a/b.txt"), None, "nothing is there; the folders are now read");
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::write(root.join("a").join("b.txt"), "x").unwrap();
    seen.added(&root.join("a").join("b.txt"));
    assert_eq!(seen.clash_with("A/b.txt"), Some(root.join("a").join("b.txt")), "a folder made since is known");
    assert!(seen.exists_spelled("a/b.txt"));
    seen.removed(&root.join("a").join("b.txt"));
    assert!(!seen.exists_spelled("a/b.txt"), "and a file deleted since is gone");
}
