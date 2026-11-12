use super::*;
use fafnirx_common::{chunk_hash, root_hash};

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

#[test]
fn upload_is_assembled_from_chunks_and_installed_under_its_name() {
    let (_dir, store) = store();
    let data = vec![7u8; CHUNK_SIZE + 5];
    let chunks: Vec<_> = data.chunks(CHUNK_SIZE).map(chunk_hash).collect();
    let root = root_hash(&chunks, data.len() as u64);

    store.write_chunk(1, &root, 1, &data[CHUNK_SIZE..]).unwrap();
    assert!(store.finish_upload(1, &root, data.len() as u64).is_err(), "a missing chunk is detected");
    store.write_chunk(1, &root, 0, &data[..CHUNK_SIZE]).unwrap();
    let staged = store.finish_upload(1, &root, data.len() as u64).unwrap();

    store.install("u/d", "docs/a.bin", &staged, Attrs { mtime_ns: 1_000_000_000, mode: 0 }).unwrap();
    assert_eq!(fs::read(store.live_path("u/d", "docs/a.bin").unwrap()).unwrap(), data);
}

#[test]
fn an_interrupted_upload_resumes_with_the_missing_chunks() {
    let (_dir, store) = store();
    let data: Vec<u8> = (0..CHUNK_SIZE * 2 + 5).map(|i| (i % 249) as u8).collect();
    let hashes: Vec<_> = data.chunks(CHUNK_SIZE).map(chunk_hash).collect();
    let root = root_hash(&hashes, data.len() as u64);
    assert_eq!(store.missing_chunks(1, &root, &hashes), [0, 1, 2], "nothing uploaded yet");

    store.write_chunk(1, &root, 1, &data[CHUNK_SIZE..2 * CHUNK_SIZE]).unwrap();
    store.write_chunk(1, &root, 2, b"junk").unwrap();
    assert_eq!(store.missing_chunks(1, &root, &hashes), [0, 2], "a chunk that does not match is still missing");
    assert_eq!(store.missing_chunks(2, &root, &hashes), [0, 1, 2], "uploads are per device");
}

#[test]
fn corrupt_or_wrong_size_uploads_are_rejected() {
    let (_dir, store) = store();
    let root = root_hash(&[chunk_hash(b"hello")], 5);
    store.write_chunk(1, &root, 0, b"hellx").unwrap();
    assert!(store.finish_upload(1, &root, 5).is_err(), "content differs from the hash");
    assert!(store.finish_upload(1, &root, 6).is_err(), "size differs");
}

#[test]
fn empty_files_need_no_chunks() {
    let (_dir, store) = store();
    let root = root_hash(&[], 0);
    let staged = store.finish_upload(1, &root, 0).unwrap();
    store.install("u/d", "empty", &staged, Attrs { mtime_ns: 0, mode: 0 }).unwrap();
    assert_eq!(fs::metadata(store.live_path("u/d", "empty").unwrap()).unwrap().len(), 0);
}

#[test]
fn archive_moves_the_live_file_into_its_version_and_back() {
    let (_dir, store) = store();
    let tmp = store.temp_path();
    fs::write(&tmp, b"one").unwrap();
    store.install("u/d", "a/b.txt", &tmp, Attrs { mtime_ns: 0, mode: 0 }).unwrap();

    assert!(store.archive("u/d", "a/b.txt", 9).unwrap());
    assert!(!store.live_path("u/d", "a/b.txt").unwrap().exists());
    assert_eq!(fs::read(store.version_path(9, "u/d", "a/b.txt").unwrap()).unwrap(), b"one");
    assert!(!store.archive("u/d", "a/b.txt", 10).unwrap(), "nothing live to archive");

    store.unarchive("u/d", "a/b.txt", 9).unwrap();
    assert!(store.live_path("u/d", "a/b.txt").unwrap().is_file());
    store.archive("u/d", "a/b.txt", 9).unwrap();
    store.remove_version(9);
    assert!(!store.version_path(9, "u/d", "a/b.txt").unwrap().exists());
}

#[test]
fn copying_a_live_file_leaves_the_original() {
    let (_dir, store) = store();
    let tmp = store.temp_path();
    fs::write(&tmp, b"same").unwrap();
    let (root, _) = hash_file(&tmp).unwrap();
    store.install("u/d", "x", &tmp, Attrs { mtime_ns: 0, mode: 0 }).unwrap();
    let copy = store.copy_live_to_temp("u/d", "x", &root).unwrap();
    store.install("u/d", "y", &copy, Attrs { mtime_ns: 0, mode: 0 }).unwrap();
    assert_eq!(fs::read(store.live_path("u/d", "x").unwrap()).unwrap(), b"same");
    assert_eq!(fs::read(store.live_path("u/d", "y").unwrap()).unwrap(), b"same");

    fs::write(store.live_path("u/d", "x").unwrap(), b"edited").unwrap();
    assert!(
        store.copy_live_to_temp("u/d", "x", &root).is_err(),
        "a file edited since it was recorded is refused"
    );
}

#[test]
fn only_versions_without_a_record_are_swept() {
    let (_dir, store) = store();
    for id in [1, 2] {
        fs::create_dir_all(store.versions.join(id.to_string())).unwrap();
    }
    let known = |ids: &[i64]| Ok::<_, ()>(ids.iter().copied().filter(|id| *id == 1).collect());
    assert_eq!(store.sweep_versions(Duration::from_secs(3600), known), Ok(0), "too new: a commit may be running");
    assert_eq!(store.sweep_versions(Duration::ZERO, known), Ok(1));
    assert!(store.versions.join("1").is_dir() && !store.versions.join("2").exists());
}

#[test]
fn paths_cannot_escape_the_store() {
    let (_dir, store) = store();
    for bad in ["../x", "a/../../x", "a//b", "a\\b", ""] {
        assert!(store.live_path("u/d", bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_file_has_one_name_whether_it_is_found_by_folder_and_path_or_by_where_it_is_on_disk() {
    let (dir, store) = store();
    for (folder, path) in [("ana/laptop", "docs/a.txt"), ("master:Music", "rock/b.mp3")] {
        let on_disk = store.live_path(folder, path).unwrap();
        assert_eq!(tree_path_of(&dir.path().join("files"), &on_disk).unwrap(), tree_path(folder, path));
    }
    let files = Path::new("/data/files");
    assert_eq!(
        tree_path_of(files, Path::new("/data/files/users/admin/pc/a b/clip.mp4")).unwrap(),
        "users/admin/pc/a b/clip.mp4"
    );
    assert_eq!(tree_path_of(files, Path::new("/data/files/clip.mp4")).unwrap(), "clip.mp4");
    assert!(tree_path_of(files, Path::new("/elsewhere/clip.mp4")).is_err());
}

#[test]
fn a_directory_of_a_device_is_a_folder_with_a_third_part() {
    let (_dir, store) = store();
    assert!(store.live_path("u/laptop/docs", "a.txt").unwrap().ends_with("users/u/laptop/docs/a.txt"));
    assert!(
        store.live_path("u/laptop/.config", "a.txt").is_ok(),
        "a directory may be named as a device calls it"
    );
    for bad in ["u/laptop/..", "u/laptop/", r"u/laptop/a\b", "u/.hidden/docs", "u/laptop/docs/deeper", "u"] {
        assert!(store.live_path(bad, "a.txt").is_err(), "{bad}");
    }
}
