//! The live tree (`files/<device name>/<path>`) can also be used as a network drive, so files appear,
//! change and vanish there without a client. This notices that and brings the database in line:
//! new and changed files become versions, removed files are marked deleted. A new folder in `files/`
//! with a valid name becomes a device of its own (kind `local`).
//!
//! A file system watcher queues the paths that were touched; once a path has been quiet for a moment
//! it is compared with the database. What the master itself just wrote matches its record (same size
//! and modification time) and is skipped without being read. Watchers miss things (network file
//! systems, overflowing queues), so the whole tree is also compared now and then.

use crate::files::ingest::{self, Seen};
use crate::platform::db::{self, Db, Device};
use crate::platform::store::{hash_file, Store};
use anyhow::Result;
use fafnirx_common::{clean_remote_path, mode_of, mtime_ns, unix_now, valid_device_name};
use notify::{RecursiveMode, Watcher};
use postgres::Client;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime};

/// File systems differ in how precisely they keep modification times.
const MTIME_SLACK_NS: i64 = 2_000_000_000;
/// A touched path is looked at once no event for it has come in this long.
const QUIET: Duration = Duration::from_secs(5);
/// Even then, a file modified more recently than this is considered still being written.
const EVENT_SETTLE: Duration = Duration::from_secs(2);
/// Without events (the full comparison), a file must have been left alone this long.
const SCAN_SETTLE: Duration = Duration::from_secs(30);
/// How often the whole tree is compared, in case events were missed.
const FULL_SCAN: Duration = Duration::from_secs(5 * 60);
const FIRST_FULL_SCAN: Duration = Duration::from_secs(30);

/// A device folder and a path inside it (`""` for the folder itself).
type Target = (String, String);

/// Reports the device folder and path of everything touched under `root`. Watching stops when the
/// watcher is dropped.
fn watch(root: &Path) -> (Option<notify::RecommendedWatcher>, mpsc::Receiver<Target>) {
    let (tx, rx) = mpsc::channel();
    let base = root.to_path_buf();
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            for path in event.paths {
                if let Some(target) = locate(&base, &path) {
                    let _ = tx.send(target);
                }
            }
        }
    })
    .and_then(|mut w| w.watch(root, RecursiveMode::Recursive).map(|_| w));
    match watcher {
        Ok(w) => (Some(w), rx),
        Err(e) => {
            tracing::warn!("cannot watch {}: {e}; the file store is only compared every {FULL_SCAN:?}", root.display());
            (None, rx)
        }
    }
}

/// Starts the watcher and the thread that works through what it reports.
pub fn spawn(db: Db, store: Arc<Store>) {
    let (watcher, rx) = watch(store.live_root());
    std::thread::spawn(move || {
        let _watcher = watcher; // watching lasts as long as this thread
        let mut pending: HashMap<Target, Instant> = HashMap::new();
        let mut next_full = Instant::now() + FIRST_FULL_SCAN;
        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(target) => {
                    pending.insert(target, Instant::now());
                    while let Ok(target) = rx.try_recv() {
                        pending.insert(target, Instant::now());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(Duration::from_secs(1)),
            }
            let ripe: Vec<_> = pending.iter().filter(|(_, at)| at.elapsed() >= QUIET).map(|(k, _)| k.clone()).collect();
            if !ripe.is_empty() {
                for key in &ripe {
                    pending.remove(key);
                }
                work(&db, &store, &ripe);
            }
            if Instant::now() >= next_full {
                next_full = Instant::now() + FULL_SCAN;
                if let Err(e) = scan(&db, &store, SCAN_SETTLE) {
                    tracing::error!("comparison of the file store failed: {e:#}");
                }
            }
        }
    });
}

/// Which device folder and path (relative to it) an event is about.
fn locate(root: &Path, changed: &Path) -> Option<Target> {
    let mut parts = changed.strip_prefix(root).ok()?.components();
    let Component::Normal(device) = parts.next()? else { return None };
    let rel: Vec<_> = parts.map(|c| c.as_os_str().to_str()).collect::<Option<_>>()?;
    Some((device.to_str()?.to_string(), rel.join("/")))
}

fn work(db: &Db, store: &Store, targets: &[Target]) {
    let mut conn = match db.get() {
        Ok(c) => c,
        Err(e) => return tracing::error!("file store: no database connection: {e}"),
    };
    for (name, rel) in targets {
        let result = device_for_folder(&mut conn, store, name)
            .and_then(|dev| dev.map_or(Ok((0, 0)), |dev| sync_subtree(&mut conn, store, &dev, rel, EVENT_SETTLE)));
        match result {
            Ok((changed, deleted)) if changed + deleted > 0 => report(name, changed, deleted),
            Ok(_) => {}
            Err(e) => tracing::error!("file store: {rel} of {name}: {e:#}"),
        }
    }
}

fn report(name: &str, changed: usize, deleted: usize) {
    tracing::info!("file store of {name}: {changed} files added or changed, {deleted} removed");
}

/// The device that owns the folder `files/<name>`. A folder nobody owns yet, with a valid name, becomes
/// a `local` device; anything else that is not a device is ignored.
fn device_for_folder(conn: &mut Client, store: &Store, name: &str) -> Result<Option<Device>> {
    // Only a local folder is written to in place. What a device or a source sends is a copy of theirs and is not
    // changed from here: edits made inside such a folder are not recorded.
    if let Some(row) = conn.query_opt("SELECT id, kind FROM devices WHERE name = $1", &[&name])? {
        return Ok((row.get::<_, String>(1) == "local").then(|| Device { id: row.get(0), name: name.to_string() }));
    }
    if !valid_device_name(name) || !store.live_dir(name).is_dir() {
        return Ok(None);
    }
    let id: i64 = conn
        .query_one(
            "INSERT INTO devices (name, kind, token_hash, created_at) VALUES ($1, 'local', '!', $2)
             ON CONFLICT (name) DO UPDATE SET name = devices.name RETURNING id",
            &[&name, &unix_now()],
        )?
        .get(0);
    tracing::info!("new folder files/{name}: added as a device");
    db::notify(conn, "devices", name)?;
    Ok(Some(Device { id, name: name.to_string() }))
}

/// Compares every device folder with the database. `settle` is how long a file must have been left
/// alone to be recorded.
pub fn scan(db: &Db, store: &Store, settle: Duration) -> Result<()> {
    let mut conn = db.get()?;
    let folders: Vec<String> = std::fs::read_dir(store.live_root())?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    for name in folders {
        let result = device_for_folder(&mut conn, store, &name)
            .and_then(|dev| dev.map_or(Ok((0, 0)), |dev| sync_subtree(&mut conn, store, &dev, "", settle)));
        match result {
            Ok((changed, deleted)) if changed + deleted > 0 => report(&name, changed, deleted),
            Ok(_) => {}
            Err(e) => tracing::error!("comparison of the files of {name} failed: {e:#}"),
        }
    }
    Ok(())
}

/// Brings the database in line with `rel` (a file or a folder; `""` is the whole device) on disk.
/// Returns (files recorded, files marked deleted).
fn sync_subtree(conn: &mut Client, store: &Store, dev: &Device, rel: &str, settle: Duration) -> Result<(usize, usize)> {
    // No folder: nothing was stored yet, or the volume is not mounted. Either way, delete nothing.
    if !store.live_dir(&dev.name).is_dir() {
        return Ok((0, 0));
    }
    let Ok(base) = (if rel.is_empty() { Ok(store.live_dir(&dev.name)) } else { store.live_path(&dev.name, rel) })
    else {
        return Ok((0, 0)); // a name that cannot be stored cannot be tracked
    };
    let known: HashMap<String, (i64, i64)> = conn
        .query(
            "SELECT f.path, v.size, v.mtime FROM files f JOIN versions v ON v.id = f.current_version_id
             WHERE f.device_id = $1 AND f.deleted_at IS NULL AND ($2 = '' OR f.path = $2 OR f.path LIKE $3)",
            &[&dev.id, &rel, &db::like_prefix(&format!("{rel}/"))],
        )?
        .iter()
        .map(|r| (r.get(0), (r.get(1), r.get(2))))
        .collect();

    let files = match std::fs::symlink_metadata(&base) {
        Ok(m) if m.is_dir() => walk(&base, rel),
        Ok(m) if m.is_file() => vec![(rel.to_string(), base)],
        _ => Vec::new(),
    };
    let mut found = HashSet::new();
    let mut changed = 0;
    for (path, full) in files {
        // A name the master would not accept from a client (or would change) cannot be tracked.
        if clean_remote_path(&path).ok().as_deref() != Some(path.as_str()) {
            tracing::warn!("ignoring {}: not a valid path", full.display());
            continue;
        }
        if check_file(conn, store, dev, &path, &full, known.get(&path).copied(), settle)? {
            changed += 1;
        }
        found.insert(path);
    }
    let mut deleted = 0;
    for path in known.keys().filter(|p| !found.contains(*p)) {
        if ingest::forget_absent(conn, store, dev, path)? {
            deleted += 1;
        }
    }
    Ok((changed, deleted))
}

/// Records a file if it differs from `known` (size, mtime of its current version). Returns whether a
/// version was added.
fn check_file(
    conn: &mut Client,
    store: &Store,
    dev: &Device,
    path: &str,
    full: &Path,
    known: Option<(i64, i64)>,
    settle: Duration,
) -> Result<bool> {
    let Ok(meta) = std::fs::metadata(full) else { return Ok(false) };
    let (size, mtime) = (meta.len(), mtime_ns(&meta));
    let age = meta.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok()).unwrap_or_default();
    if age < settle {
        return Ok(false);
    }
    if known.is_some_and(|(s, m)| s as u64 == size && (m - mtime).abs() < MTIME_SLACK_NS) {
        return Ok(false);
    }
    let (root, hashed) = match hash_file(full) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("cannot read {}: {e:#}", full.display());
            return Ok(false);
        }
    };
    let seen = Seen { root, size: hashed, mtime, mode: mode_of(&meta) };
    Ok(hashed == size && ingest::adopt_live(conn, store, dev, path, &seen)?)
}

/// Every regular file under `dir` as (path with `/`, full path); `prefix` is the path of `dir` itself.
/// Links are skipped.
fn walk(dir: &Path, prefix: &str) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut pending = vec![(prefix.to_string(), dir.to_path_buf())];
    while let Some((prefix, folder)) = pending.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let (Ok(kind), Some(name)) = (entry.file_type(), entry.file_name().to_str().map(String::from)) else {
                tracing::warn!("ignoring {}: unreadable or not a valid name", entry.path().display());
                continue;
            };
            let (name, path) = composed(&entry.path(), name);
            let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            if kind.is_dir() {
                pending.push((rel, path));
            } else if kind.is_file() {
                out.push((rel, path));
            }
        }
    }
    out
}

/// Names are stored composed (NFC), but macOS writes them decomposed. A name that is not composed is renamed on
/// disk, unless that name is taken, so that the file is tracked like any other.
fn composed(path: &Path, name: String) -> (String, PathBuf) {
    let nfc = fafnirx_common::nfc(&name);
    if nfc == name {
        return (name, path.to_path_buf());
    }
    let target = path.with_file_name(&nfc);
    if !target.exists() && std::fs::rename(path, &target).is_ok() {
        tracing::info!("renamed {} to its composed form", path.display());
        return (nfc, target);
    }
    (name, path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::db::testing;
    use fafnirx_common::{chunk_hash, root_hash};
    use std::fs;

    fn put(store: &Store, path: &str, data: &[u8]) -> PathBuf {
        let p = store.live_path("d", path).unwrap();
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
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let (dev, none) = (testing::device(), Duration::ZERO);
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
        let actions: Vec<String> =
            c.query("SELECT action FROM events ORDER BY id", &[]).unwrap().iter().map(|r| r.get(0)).collect();
        assert_eq!(actions, ["add", "update", "delete"]);
    }

    #[test]
    fn only_local_folders_are_written_to_in_place() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        put(&store, "a.txt", b"x");
        assert!(
            device_for_folder(&mut c, &store, "d").unwrap().is_none(),
            "the folder of a device is not changed from here"
        );
        c.execute("UPDATE devices SET kind = 'local'", &[]).unwrap();
        assert!(device_for_folder(&mut c, &store, "d").unwrap().is_some());
    }

    #[test]
    fn decomposed_names_are_renamed_to_their_composed_form() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        put(&store, "café/menu.txt", b"x");
        assert_eq!(sync_subtree(&mut c, &store, &testing::device(), "", Duration::ZERO).unwrap(), (1, 0));
        let paths: Vec<String> = live(&mut c).into_iter().map(|(p, _)| p).collect();
        assert_eq!(paths, ["café/menu.txt"]);
        assert!(store.live_path("d", "café/menu.txt").unwrap().is_file());
    }

    #[test]
    fn an_event_for_one_path_only_looks_at_that_path() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let (dev, none) = (testing::device(), Duration::ZERO);
        put(&store, "a/x", b"1");
        put(&store, "a/y", b"2");
        put(&store, "b/z", b"3");
        assert_eq!(sync_subtree(&mut c, &store, &dev, "a/x", none).unwrap(), (1, 0), "one file");
        assert_eq!(sync_subtree(&mut c, &store, &dev, "a", none).unwrap(), (1, 0), "x is known, y is new");
        assert_eq!(live(&mut c).len(), 2, "b/z was not looked at");

        fs::remove_dir_all(store.live_path("d", "a").unwrap()).unwrap();
        assert_eq!(
            sync_subtree(&mut c, &store, &dev, "a", none).unwrap(),
            (0, 2),
            "a removed folder removes its files"
        );
        assert_eq!(sync_subtree(&mut c, &store, &dev, "", none).unwrap(), (1, 0));
        assert_eq!(live(&mut c).len(), 1);
    }

    /// Waits until the watcher has reported one of `wanted` (device, path).
    fn reports(rx: &mpsc::Receiver<Target>, wanted: &[(&str, &str)]) -> Result<(), String> {
        let mut seen = HashSet::new();
        while let Ok(target) = rx.recv_timeout(Duration::from_secs(5)) {
            if wanted.iter().any(|(d, p)| target == (d.to_string(), p.to_string())) {
                return Ok(());
            }
            seen.insert(target);
        }
        Err(format!("none of {wanted:?} was reported, got {seen:?}"))
    }

    #[test]
    fn the_watcher_reports_what_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("three/docs")).unwrap();
        let (watcher, rx) = watch(&root);
        assert!(watcher.is_some());

        fs::write(root.join("three/docs/a.txt"), b"x").unwrap();
        reports(&rx, &[("three", "docs/a.txt")]).unwrap();

        // A folder that is made now is watched a moment later, so a file written into it at once may only be
        // reported through the folder, whose whole contents are then compared with the database.
        fs::create_dir_all(root.join("three/new")).unwrap();
        fs::write(root.join("three/new/b.txt"), b"y").unwrap();
        reports(&rx, &[("three", "new/b.txt"), ("three", "new")]).unwrap();
    }

    #[test]
    fn events_are_mapped_to_a_device_and_path() {
        let root = Path::new("data").join("files");
        assert_eq!(
            locate(&root, &root.join("pc").join("docs").join("a.txt")),
            Some(("pc".into(), "docs/a.txt".into()))
        );
        assert_eq!(locate(&root, &root.join("pc")), Some(("pc".into(), String::new())));
        assert_eq!(locate(&root, Path::new("elsewhere/pc/a")), None);
    }

    #[test]
    fn a_new_folder_becomes_a_local_device_and_bad_names_are_ignored() {
        let db = testing::db();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        fs::create_dir_all(store.live_dir("camera")).unwrap();
        fs::create_dir_all(store.live_dir("Bad Name")).unwrap();
        let dev = device_for_folder(&mut c, &store, "camera").unwrap().unwrap();
        let kind: String = c.query_one("SELECT kind FROM devices WHERE id = $1", &[&dev.id]).unwrap().get(0);
        assert_eq!(kind, "local");
        assert_eq!(
            device_for_folder(&mut c, &store, "camera").unwrap().unwrap().id,
            dev.id,
            "found again, not duplicated"
        );
        assert!(device_for_folder(&mut c, &store, "Bad Name").unwrap().is_none());
        assert!(device_for_folder(&mut c, &store, "missing").unwrap().is_none());
    }

    #[test]
    fn a_missing_device_folder_deletes_nothing() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let dev = testing::device();
        put(&store, "a", b"x");
        sync_subtree(&mut c, &store, &dev, "", Duration::ZERO).unwrap();
        fs::remove_dir_all(store.live_dir("d")).unwrap();
        assert_eq!(sync_subtree(&mut c, &store, &dev, "", Duration::ZERO).unwrap(), (0, 0));
        assert_eq!(live(&mut c).len(), 1);
    }

    #[test]
    fn files_committed_by_a_client_are_left_alone() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let dev = testing::device();
        let staged = store.temp_path();
        fs::write(&staged, b"from a client").unwrap();
        let v = ingest::NewVersion {
            device: &dev,
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
}
