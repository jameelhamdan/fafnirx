use super::*;

fn row(options: &str) -> MirrorRow {
    let link = Due {
        id: 1,
        dir: crate::repo::links::Dir::Out,
        kind: "replica".into(),
        name: "m".into(),
        config: String::new(),
        folder_id: None,
        folder: None,
        owner: None,
    };
    let config = mirrors::Config {
        remote_type: "s3".into(),
        options: options.into(),
        remote_path: "bucket/prefix/".into(),
        keep_runs: 3,
        ..Default::default()
    };
    MirrorRow { link, config }
}

fn file(device_id: i64, path: &str, root: u8, size: u64) -> LiveFile {
    LiveFile { device_id, ns: "d".into(), path: path.into(), root: [root; HASH_LEN], size, mtime: 0 }
}

#[test]
fn stamps_sort_chronologically() {
    assert_eq!(run_stamp(0), "19700101T000000Z");
    assert_eq!(run_stamp(1_700_000_000), "20231114T221320Z");
    assert!(run_stamp(1_700_000_000) < run_stamp(1_800_000_000));
}

#[test]
fn rclone_options_become_environment() {
    let env = rclone_env(&row(r#"{"access_key_id":"k","secret_access_key":"s"}"#), Path::new("/tmp/c"), Path::new("/none")).unwrap();
    assert!(env.contains(&("RCLONE_CONFIG_FXMIRROR_TYPE".into(), "s3".into())));
    assert!(env.contains(&("RCLONE_CONFIG_FXMIRROR_SECRET_ACCESS_KEY".into(), "s".into())));
    assert!(
        rclone_env(&row(r#"{"Bad-Key":"x"}"#), Path::new("/tmp/c"), Path::new("/none")).is_err(),
        "option names cannot smuggle other variables"
    );
    let mut bad = row("{}");
    bad.config.remote_type = "s3;rm".into();
    assert!(rclone_env(&bad, Path::new("/tmp/c"), Path::new("/none")).is_err());
}

#[test]
fn a_remote_of_the_rclone_config_is_used_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("rclone.conf");
    let mut m = row("{}");
    m.config.remote = "crypt:backups/fafnirx/".into();
    let missing = rclone_env(&m, Path::new("/tmp/c"), &conf).unwrap_err();
    assert!(format!("{missing}").contains("rclone config file"), "{missing}");
    std::fs::write(
        &conf,
        "[crypt]
type = local
",
    )
    .unwrap();
    let env = rclone_env(&m, Path::new("/tmp/c"), &conf).unwrap();
    assert_eq!(
        env,
        [("RCLONE_CONFIG".to_string(), conf.to_string_lossy().into_owned())],
        "nothing else reaches rclone"
    );
    let r = Remotes::new(&base_of(&m), "20240101T000000Z");
    assert_eq!(
        (r.root.as_str(), r.files.as_str(), r.run.as_str()),
        (
            "crypt:backups/fafnirx",
            "crypt:backups/fafnirx/files",
            "crypt:backups/fafnirx/versions/20240101T000000Z"
        )
    );
    m.config.remote = "crypt:".into();
    assert_eq!(Remotes::new(&base_of(&m), "s").files, "crypt:files", "a remote without a path");
}

#[test]
fn a_backup_stacks_encryption_over_the_storage() {
    let mut m = row("{}");
    assert_eq!(top_of(&m), "FXMIRROR:bucket/prefix", "a replica writes to the storage itself");
    m.link.kind = "backup".into();
    assert_eq!(top_of(&m), "FXMIRRORCRYPT:");
    let env = backup_env(&base_of(&m), "pw", "salt");
    assert!(env.contains(&("RCLONE_CONFIG_FXMIRRORCRYPT_REMOTE".into(), "FXMIRROR:bucket/prefix".into())));
    assert!(env.contains(&("RCLONE_CONFIG_FXMIRRORCRYPT_PASSWORD2".into(), "salt".into())));
}

#[test]
fn the_restore_config_names_the_storage_and_the_key() {
    let config = mirrors::Config {
        remote_type: "b2".into(),
        options: r#"{"account":"id","key":"k"}"#.into(),
        remote_path: "bucket/fx/".into(),
        ..Default::default()
    };
    let key = mirrors::Key { kind: MirrorKind::Backup, config };
    let conf = restore_config(&key, "OBS1", "OBS2");
    for line in [
        "type = b2",
        "account = id",
        "remote = fafnirx-storage:bucket/fx",
        "password = OBS1",
        "password2 = OBS2",
        "filename_encryption = standard",
        "directory_name_encryption = true",
    ] {
        assert!(conf.lines().any(|l| l == line), "{line} is a line of its own, not indented, in {conf}");
    }
    let own = mirrors::Key { config: mirrors::Config { remote: "mine:fx/".into(), ..key.config }, ..key };
    let conf = restore_config(&own, "a", "b");
    assert!(conf.contains("remote = mine:fx") && !conf.contains("fafnirx-storage"), "{conf}");
}

#[test]
fn remote_paths_join_cleanly() {
    assert_eq!(join("FXMIRROR:bucket/prefix/", "files"), "FXMIRROR:bucket/prefix/files");
    assert_eq!(join("FXMIRROR:/abs/dir", "files"), "FXMIRROR:/abs/dir/files");
    assert_eq!(join("FXMIRROR:", "files"), "FXMIRROR:files");
    assert_eq!(join("FXMIRROR:b", ""), "FXMIRROR:b");
    let mut m = row("{}");
    m.config.remote_path = "b/x".into();
    let r = Remotes::new(&base_of(&m), "20240101T000000Z");
    assert_eq!((r.files.as_str(), r.run.as_str()), ("FXMIRROR:b/x/files", "FXMIRROR:b/x/versions/20240101T000000Z"));
}

#[test]
fn only_the_newest_runs_are_kept() {
    let dirs = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(expired_versions(dirs(&["c", "a", "d", "b"]), 3), ["a"]);
    assert!(expired_versions(dirs(&["a", "b"]), 3).is_empty());
    assert_eq!(expired_versions(dirs(&["a", "b"]), 0), ["a"], "never keeps fewer than one");
}

#[test]
fn diff_finds_new_changed_and_removed_files() {
    let live = [file(1, "same", 1, 10), file(1, "edited", 2, 10), file(1, "new", 3, 10), file(2, "same", 4, 10)];
    let mut mirrored = Mirrored::new();
    mirrored.entry(1).or_default().insert("same".into(), vec![1; HASH_LEN]);
    mirrored.entry(1).or_default().insert("edited".into(), vec![9; HASH_LEN]);
    mirrored.entry(1).or_default().insert("deleted".into(), vec![5; HASH_LEN]);
    let d = diff(&live, &mirrored);
    let mut changed: Vec<_> = d.changed.iter().map(|f| (f.device_id, f.path.as_str())).collect();
    changed.sort();
    assert_eq!(changed, [(1, "edited"), (1, "new"), (2, "same")], "same path on another device is a different file");
    assert_eq!(d.removed, [(1, "deleted".to_string())]);
    assert!(d.renamed.is_empty());
}

#[test]
fn a_new_path_with_the_content_of_a_vanished_one_is_a_rename() {
    let live = [file(1, "kept", 1, 10), file(1, "new/name", 2, 10), file(2, "moved", 3, 10), file(1, "fresh", 7, 10)];
    let mut mirrored = Mirrored::new();
    mirrored.entry(1).or_default().insert("kept".into(), vec![1; HASH_LEN]);
    mirrored.entry(1).or_default().insert("old/name".into(), vec![2; HASH_LEN]);
    mirrored.entry(1).or_default().insert("gone".into(), vec![4; HASH_LEN]);
    mirrored.entry(3).or_default().insert("elsewhere".into(), vec![3; HASH_LEN]);
    let d = diff(&live, &mirrored);
    let mut moves: Vec<_> = d.renamed.iter().map(|r| (r.from.clone(), r.to.path.as_str())).collect();
    moves.sort();
    assert_eq!(
        moves,
        [((1, "old/name".to_string()), "new/name"), ((3, "elsewhere".to_string()), "moved")],
        "a move to another device counts"
    );
    assert_eq!(d.changed.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["fresh"]);
    assert_eq!(d.removed, [(1, "gone".to_string())], "only what was not renamed is removed");
}

#[test]
fn copies_of_one_content_are_paired_one_to_one() {
    let live = [file(1, "a2", 5, 10), file(1, "a3", 5, 10), file(1, "a4", 5, 10)];
    let mut mirrored = Mirrored::new();
    for old in ["a0", "a1"] {
        mirrored.entry(1).or_default().insert(old.into(), vec![5; HASH_LEN]);
    }
    let d = diff(&live, &mirrored);
    assert_eq!((d.renamed.len(), d.changed.len(), d.removed.len()), (2, 1, 0), "the third copy is an upload");
}

#[test]
fn an_edit_in_place_is_not_a_rename() {
    let live = [file(1, "same", 2, 10)];
    let mut mirrored = Mirrored::new();
    mirrored.entry(1).or_default().insert("same".into(), vec![1; HASH_LEN]);
    let d = diff(&live, &mirrored);
    assert_eq!((d.renamed.len(), d.changed.len(), d.removed.len()), (0, 1, 0));
}

#[test]
fn batches_respect_both_limits() {
    let files: Vec<LiveFile> = (0..5).map(|i| file(1, &format!("f{i}"), i, 100)).collect();
    let refs: Vec<&LiveFile> = files.iter().collect();
    let sizes = |b: Vec<Vec<&LiveFile>>| b.iter().map(Vec::len).collect::<Vec<_>>();
    assert_eq!(sizes(batches(&refs, 250, 100)), [3, 2], "by bytes: a batch closes once it reaches the limit");
    assert_eq!(sizes(batches(&refs, u64::MAX, 2)), [2, 2, 1], "by file count");
    assert!(batches(&[], 10, 10).is_empty());
}
