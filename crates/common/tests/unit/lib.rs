use super::*;

#[test]
fn a_file_is_written_whole_over_the_old_one_and_leaves_nothing_beside_it() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("status.json");
    write_atomic(&file, b"one").unwrap();
    write_atomic(&file, "two").unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "two");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary file is left");
    // A folder in the way cannot be replaced: the error is reported and the temporary file is removed.
    let blocked = dir.path().join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    assert!(write_atomic(&blocked, b"x").is_err());
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        2,
        "{:?}",
        std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect::<Vec<_>>()
    );
}

#[test]
fn a_lock_whose_holder_panicked_is_still_taken() {
    let lock = std::sync::Arc::new(std::sync::Mutex::new(1));
    let l = lock.clone();
    let _ = std::thread::spawn(move || {
        let _held = l.lock().unwrap();
        panic!("while holding the lock");
    })
    .join();
    assert!(lock.lock().is_err(), "the lock is poisoned");
    *lock.locked() += 1;
    assert_eq!(*lock.locked(), 2);
}

/// A device can send data and say what it deleted. What it can read is the client update and the shared folders of its
/// user; there is no call that reads or lists the files of a device or the master files. A new call in the contract has
/// to be added here on purpose.
#[test]
fn the_device_protocol_reads_nothing_but_updates_and_shared_folders() {
    let proto = include_str!("../../../../proto/fafnirx.proto");
    let rpcs: Vec<&str> = proto
        .lines()
        .filter_map(|l| l.trim().strip_prefix("rpc "))
        .map(|l| l.split('(').next().unwrap())
        .collect();
    assert_eq!(
        rpcs,
        [
            "Heartbeat",
            "Identify",
            "Negotiate",
            "UploadChunks",
            "Commit",
            "MarkDeleted",
            "ListFolders",
            "CreateFolder",
            "ListChanges",
            "DownloadFolder",
            "WatchFolder"
        ]
    );
    // The only streams from the master to a device carry the shared folders.
    let returning_streams: Vec<&str> = proto
        .lines()
        .filter(|l| l.contains("returns (stream"))
        .map(|l| l.trim().strip_prefix("rpc ").unwrap().split('(').next().unwrap())
        .collect();
    assert_eq!(returning_streams, ["ListChanges", "DownloadFolder", "WatchFolder"]);
}

#[test]
fn equal_content_is_interchangeable_and_the_earliest_vanished_path_is_paired_first() {
    let mut gone = Vanished::new([("a", "old/1"), ("b", "old/2"), ("a", "old/3")]);
    assert_eq!(gone.take(&"a"), Some("old/1"));
    assert_eq!(gone.take(&"a"), Some("old/3"));
    assert_eq!(gone.take(&"a"), None, "each is paired once");
    assert_eq!(gone.take(&"c"), None);
    assert_eq!(gone.into_remaining(), ["old/2"]);
}

#[test]
fn paths_are_one_name_when_they_differ_only_in_case_or_unicode_form() {
    assert_eq!(fold_path("Docs/README.md"), fold_path("docs/readme.MD"));
    assert_eq!(fold_path("caf\u{e9}"), fold_path("cafe\u{301}"), "an accent as one character or two");
    assert_ne!(fold_path("a/b"), fold_path("a/c"));
}

#[test]
fn names_windows_cannot_keep_are_not_portable() {
    for ok in ["a/b.txt", "Report 2026.pdf", "notes.tar.gz", "console.txt", "com.txt", "lpt10.txt", "caf\u{e9}"] {
        assert!(not_portable(ok).is_none(), "{ok}");
    }
    for bad in [
        "a:b.txt",
        "what?.txt",
        "a/b*c",
        "x<y",
        "pipe|d",
        "q\"uote",
        "trailing.",
        "trailing ",
        "dir./f",
        "CON",
        "con.txt",
        "a/AUX.tar.gz",
        "Com1",
        "lpt9.log",
        "nul",
    ] {
        assert!(not_portable(bad).is_some(), "{bad}");
    }
}

#[test]
fn root_hash_depends_on_size_and_chunks() {
    let a = chunk_hash(b"a");
    let b = chunk_hash(b"b");
    assert_ne!(root_hash(&[a, b], 2), root_hash(&[b, a], 2));
    assert_ne!(root_hash(&[a], 1), root_hash(&[a], 2));
}

#[test]
fn chunk_hasher_matches_chunk_by_chunk_hashing() {
    let data: Vec<u8> = (0..CHUNK_SIZE * 2 + 10).map(|i| (i % 251) as u8).collect();
    let chunks: Vec<_> = data.chunks(CHUNK_SIZE).map(chunk_hash).collect();
    let mut h = ChunkHasher::default();
    for piece in data.chunks(1_000_003) {
        h.update(piece);
    }
    let done = h.finish();
    assert_eq!(
        (done.root, done.size, done.chunks),
        (root_hash(&chunks, data.len() as u64), data.len() as u64, chunks)
    );
    let empty = ChunkHasher::default().finish();
    assert_eq!((empty.root, empty.size, empty.chunks.len()), (root_hash(&[], 0), 0, 0));
    assert!(hash_reader(&b"abc"[..]).unwrap().root == root_hash(&[chunk_hash(b"abc")], 3));
}

#[test]
fn a_name_longer_than_a_filesystem_allows_is_refused() {
    assert!(clean_remote_path(&format!("a/{}", "x".repeat(255))).is_ok());
    assert!(clean_remote_path(&format!("a/{}", "x".repeat(256))).is_err());
}

#[test]
fn device_names_are_slugs() {
    for ok in ["laptop", "pc-2", "a", "my.phone_1", "9lives", "console"] {
        assert!(valid_device_name(ok), "{ok}");
    }
    for bad in ["", "-a", ".a", "A", "a b", "a/b", "a:b", "con", "nul.txt", "com1", "lpt9", &"a".repeat(64), "é"] {
        assert!(!valid_device_name(bad), "{bad}");
    }
}

#[test]
fn chunk_counts() {
    assert_eq!(chunk_count(0), 0);
    assert_eq!(chunk_count(1), 1);
    assert_eq!(chunk_count(CHUNK_SIZE as u64), 1);
    assert_eq!(chunk_count(CHUNK_SIZE as u64 + 1), 2);
}

#[test]
fn remote_paths() {
    assert_eq!(clean_remote_path("docs/a.txt").unwrap(), "docs/a.txt");
    assert_eq!(clean_remote_path("e\u{301}.txt").unwrap(), "\u{e9}.txt");
    for bad in ["", "/etc/passwd", "a/../b", "a//b", "a\\b", "./a", "a/\u{0}b"] {
        assert!(clean_remote_path(bad).is_err(), "{bad:?} should be rejected");
    }
}
