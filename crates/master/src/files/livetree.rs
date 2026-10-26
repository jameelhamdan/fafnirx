//! The live tree (`files/users/<user>/<device>/<path>`, and the master files straight in `files/`) can also be used as a
//! network drive, so files appear, change and vanish there without a client. This notices that and brings the
//! database in line: new and changed files become versions, removed files are marked deleted. Whatever is in
//! `files/` and is not in `files/users/` is a master file. A new folder `files/users/<user>/<name>` with a valid name
//! becomes a device of that user (kind `local`).
//!
//! A file system watcher queues the paths that were touched; once a path has been quiet for a moment
//! it is compared with the database. What the master itself just wrote matches its record (same size
//! and modification time) and is skipped without being read. Watchers miss things (network file
//! systems, overflowing queues), so the whole tree is also compared now and then.

use crate::files::ingest::{self, Seen};
use crate::platform::db::{self, Db, Device};
use crate::platform::store::USERS_DIR;
use crate::platform::store::{hash_file, Store, MASTER_FOLDER};
use crate::repo::{devices, files, users};
use anyhow::Result;
use fafnirx_common::{clean_remote_path, mode_of, mtime_ns, unix_now, valid_device_name};
use notify::{RecursiveMode, Watcher};
use postgres::Client;
use postgres::GenericClient;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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

/// A path relative to the live tree: what was touched.
type Target = String;

/// Reports the path (relative to `root`) of everything touched under it. Watching stops when the watcher is
/// dropped.
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

/// Set while a comparison of the whole tree is running, so that the periodic one and one asked for by hand never overlap.
static SCANNING: AtomicBool = AtomicBool::new(false);

/// Runs [`scan`] unless one is already running. Returns whether it ran.
fn scan_once(db: &Db, store: &Store) -> bool {
    if SCANNING.swap(true, Ordering::AcqRel) {
        return false;
    }
    if let Err(e) = scan(db, store, SCAN_SETTLE) {
        tracing::error!("comparison of the file store failed: {e:#}");
    }
    SCANNING.store(false, Ordering::Release);
    true
}

/// Compares the whole tree with the database now, in the background, instead of at the next round. Returns false when a
/// comparison is already running.
pub fn rescan(db: Db, store: Arc<Store>) -> bool {
    if SCANNING.load(Ordering::Acquire) {
        return false;
    }
    std::thread::spawn(move || {
        scan_once(&db, &store);
    });
    true
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
                scan_once(&db, &store);
            }
        }
    });
}

/// The path of an event relative to the live tree.
fn locate(root: &Path, changed: &Path) -> Option<Target> {
    let mut parts = changed.strip_prefix(root).ok()?.components().peekable();
    parts.peek()?;
    let rel: Vec<_> = parts
        .map(|c| match c {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    Some(rel.join("/"))
}

/// The device that holds the master files.
fn master() -> Device {
    Device { id: ingest::MASTER_DEVICE_ID, name: MASTER_FOLDER.to_string() }
}

/// The names of the users: their folders in `files/` are not master files.
fn user_names(conn: &mut impl GenericClient) -> Result<HashSet<String>> {
    Ok(users::names(conn)?.into_iter().collect())
}

/// The device and the path inside it that a path of the live tree is about: under `users/<user>/` the next part is a
/// device of that user, or else the path is a master file.
fn resolve(conn: &mut Client, store: &Store, rel: &str) -> Result<Option<(Device, String)>> {
    let parts: Vec<&str> = rel.split('/').collect();
    if parts[0].to_lowercase() == USERS_DIR {
        // users/<user>/<device>/...: shorter paths are the folder of all users or of one user, whose devices are looked at
        // one by one.
        if parts.len() < 3 || parts[0] != USERS_DIR || !user_names(conn)?.contains(parts[1]) {
            return Ok(None);
        }
        let folder = format!("{}/{}", parts[1], parts[2]);
        return Ok(device_for_folder(conn, store, &folder)?.map(|dev| (dev, parts[3..].join("/"))));
    }
    Ok(Some((master(), rel.to_string())))
}

fn work(db: &Db, store: &Store, targets: &[Target]) {
    let mut conn = match db.get() {
        Ok(c) => c,
        Err(e) => return tracing::error!("file store: no database connection: {e}"),
    };
    for target in targets {
        let result = resolve(&mut conn, store, target).and_then(|found| {
            found.map_or(Ok(None), |(dev, rel)| {
                sync_subtree(&mut conn, store, &dev, &rel, EVENT_SETTLE).map(|counts| Some((dev.name, counts)))
            })
        });
        match result {
            Ok(Some((name, (changed, deleted)))) if changed + deleted > 0 => report(&name, changed, deleted),
            Ok(_) => {}
            Err(e) => tracing::error!("file store: {target}: {e:#}"),
        }
    }
}

fn report(name: &str, changed: usize, deleted: usize) {
    tracing::info!("file store of {name}: {changed} files added or changed, {deleted} removed");
}

/// The device that owns the folder `files/<user>/<name>` (`folder` is `<user>/<name>`). A folder nobody owns yet, with a
/// valid name, in the folder of a user who can have devices, becomes a `local` device of the user; anything else
/// that is not a device is ignored.
fn device_for_folder(conn: &mut Client, store: &Store, folder: &str) -> Result<Option<Device>> {
    // Only a local folder is written to in place. What a device or a source sends is a copy of theirs and is not
    // changed from here: edits made inside such a folder are not recorded.
    if let Some((id, kind)) = devices::id_and_kind(conn, folder)? {
        return Ok((kind == "local").then(|| Device { id, name: folder.to_string() }));
    }
    let Some((user, name)) = folder.split_once('/') else { return Ok(None) };
    if !valid_device_name(name) || !store.live_dir(folder).is_dir() {
        return Ok(None);
    }
    let Some(owner) = users::who_can_have_devices(conn, user)? else { return Ok(None) };
    let id = devices::ensure_local(conn, owner, name, unix_now())?;
    tracing::info!("new folder files/{folder}: added as a device");
    db::notify(conn, "devices", folder)?;
    Ok(Some(Device { id, name: folder.to_string() }))
}

/// Compares the master files and every device folder of every user with the database. `settle` is how long a file
/// must have been left alone to be recorded.
pub fn scan(db: &Db, store: &Store, settle: Duration) -> Result<()> {
    let mut conn = db.get()?;
    let mut devices = vec![master()];
    for user in users::names(&mut *conn)? {
        let folders: Vec<String> = std::fs::read_dir(store.user_dir(&user))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        for name in folders {
            match device_for_folder(&mut conn, store, &format!("{user}/{name}")) {
                Ok(dev) => devices.extend(dev),
                Err(e) => tracing::error!("comparison of the files of {user}/{name} failed: {e:#}"),
            }
        }
    }
    for dev in devices {
        match sync_subtree(&mut conn, store, &dev, "", settle) {
            Ok((changed, deleted)) if changed + deleted > 0 => report(&dev.name, changed, deleted),
            Ok(_) => {}
            Err(e) => tracing::error!("comparison of the files of {} failed: {e:#}", dev.name),
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
    let known: HashMap<String, (i64, i64)> =
        files::live_below(conn, dev.id, rel)?.into_iter().map(|(path, size, mtime)| (path, (size, mtime))).collect();

    // The master files share `files/` with `users/` (the folders of the users), which is not theirs to record.
    let skip = if dev.id == ingest::MASTER_DEVICE_ID && rel.is_empty() {
        HashSet::from([USERS_DIR.to_string()])
    } else {
        HashSet::new()
    };
    let files = match std::fs::symlink_metadata(&base) {
        Ok(m) if m.is_dir() => walk(&base, rel, &skip),
        Ok(m) if m.is_file() => vec![(rel.to_string(), base)],
        _ => Vec::new(),
    };
    // A name the master would not accept from a client (or would change) cannot be tracked.
    let files: Vec<_> = files
        .into_iter()
        .filter(|(path, full)| {
            let ok = clean_remote_path(path).ok().as_deref() == Some(path.as_str());
            if !ok {
                tracing::warn!("ignoring {}: not a valid path", full.display());
            }
            ok
        })
        .collect();
    let found: HashSet<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    // A new file with the size and modification time of exactly one vanished path is that path renamed (a move keeps both).
    let mut vanished: Vec<(&str, (i64, i64))> =
        known.iter().filter(|(p, _)| !found.contains(p.as_str())).map(|(p, v)| (p.as_str(), *v)).collect();
    let mut changed = 0;
    for (path, full) in &files {
        let renamed_from = if known.contains_key(path) { None } else { take_rename(&mut vanished, full) };
        if check_file(conn, store, dev, path, full, Before { known: known.get(path).copied(), renamed_from }, settle)? {
            changed += 1;
        }
    }
    let mut deleted = 0;
    for (path, _) in &vanished {
        if ingest::forget_absent(conn, store, dev, path)? {
            deleted += 1;
        }
    }
    Ok((changed, deleted))
}

/// What the database knows before a file is looked at: its size and mtime, and the path it was moved from.
struct Before {
    known: Option<(i64, i64)>,
    renamed_from: Option<String>,
}

/// The vanished path a new file at `full` is a rename of: the only one with its size and modification time. Taken out of the list.
fn take_rename(vanished: &mut Vec<(&str, (i64, i64))>, full: &Path) -> Option<String> {
    let meta = std::fs::metadata(full).ok()?;
    let (size, mtime) = (meta.len() as i64, mtime_ns(&meta));
    if size == 0 {
        return None;
    }
    let mut hits =
        vanished.iter().enumerate().filter(|(_, (_, (s, m)))| *s == size && (m - mtime).abs() < MTIME_SLACK_NS);
    let (index, _) = hits.next()?;
    if hits.next().is_some() {
        return None;
    }
    Some(vanished.remove(index).0.to_string())
}

/// Records a file if it differs from `known` (size, mtime of its current version). Returns whether a
/// version was added.
fn check_file(
    conn: &mut Client,
    store: &Store,
    dev: &Device,
    path: &str,
    full: &Path,
    before: Before,
    settle: Duration,
) -> Result<bool> {
    let Ok(meta) = std::fs::metadata(full) else { return Ok(false) };
    let (size, mtime) = (meta.len(), mtime_ns(&meta));
    let age = meta.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok()).unwrap_or_default();
    if age < settle {
        return Ok(false);
    }
    if before.known.is_some_and(|(s, m)| s as u64 == size && (m - mtime).abs() < MTIME_SLACK_NS) {
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
    Ok(hashed == size && ingest::adopt_live(conn, store, dev, path, &seen, before.renamed_from.as_deref())?)
}

/// Every regular file under `dir` as (path with `/`, full path); `prefix` is the path of `dir` itself.
/// Links are skipped, and so are the folders in `skip` at the top.
fn walk(dir: &Path, prefix: &str, skip: &HashSet<String>) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut pending = vec![(prefix.to_string(), dir.to_path_buf())];
    while let Some((prefix, folder)) = pending.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let (Ok(kind), Some(name)) = (entry.file_type(), entry.file_name().to_str().map(String::from)) else {
                tracing::warn!("ignoring {}: unreadable or not a valid name", entry.path().display());
                continue;
            };
            if prefix.is_empty() && skip.contains(&name.to_lowercase()) {
                continue;
            }
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
        let p = store.live_path("admin/d", path).unwrap();
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
    fn a_file_moved_in_a_local_folder_is_a_rename_and_its_album_follows() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let (dev, none) = (testing::device(), Duration::ZERO);
        put(&store, "Show/a.txt", b"episode one");
        assert_eq!(sync_subtree(&mut c, &store, &dev, "", none).unwrap(), (1, 0));
        let album = crate::repo::albums::insert(&mut *c, 1, "Show", "", Some((1, "Show/")), 0).unwrap();

        let from = store.live_path("admin/d", "Show/a.txt").unwrap();
        let to = store.live_path("admin/d", "Show 2/a.txt").unwrap();
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
        let prefix: String = c.query_one("SELECT folder_prefix FROM albums WHERE id = $1", &[&album]).unwrap().get(0);
        assert_eq!(prefix, "Show 2/");
    }

    #[test]
    fn only_local_folders_are_written_to_in_place() {
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        put(&store, "a.txt", b"x");
        assert!(
            device_for_folder(&mut c, &store, "admin/d").unwrap().is_none(),
            "the folder of a device is not changed from here"
        );
        c.execute("UPDATE devices SET kind = 'local' WHERE name = 'd'", &[]).unwrap();
        assert!(device_for_folder(&mut c, &store, "admin/d").unwrap().is_some());
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
        assert!(store.live_path("admin/d", "café/menu.txt").unwrap().is_file());
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

        fs::remove_dir_all(store.live_path("admin/d", "a").unwrap()).unwrap();
        assert_eq!(
            sync_subtree(&mut c, &store, &dev, "a", none).unwrap(),
            (0, 2),
            "a removed folder removes its files"
        );
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
    fn a_new_folder_in_a_users_folder_becomes_their_local_device_and_bad_names_are_ignored() {
        let db = testing::db();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        fs::create_dir_all(store.live_dir("admin/camera")).unwrap();
        fs::create_dir_all(store.live_dir("admin/Bad Name")).unwrap_or_default();
        fs::create_dir_all(store.live_dir("nobody/camera")).unwrap();
        let dev = device_for_folder(&mut c, &store, "admin/camera").unwrap().unwrap();
        let row = c.query_one("SELECT kind, user_id FROM devices WHERE id = $1", &[&dev.id]).unwrap();
        assert_eq!((row.get::<_, String>(0), row.get::<_, i64>(1)), ("local".into(), 1));
        assert_eq!(
            device_for_folder(&mut c, &store, "admin/camera").unwrap().unwrap().id,
            dev.id,
            "found again, not duplicated"
        );
        assert!(device_for_folder(&mut c, &store, "admin/Bad Name").unwrap().is_none());
        assert!(device_for_folder(&mut c, &store, "admin/missing").unwrap().is_none());
        assert!(device_for_folder(&mut c, &store, "nobody/camera").unwrap().is_none(), "there is no such user");
        c.execute("UPDATE users SET role = 'readonly'", &[]).unwrap();
        fs::create_dir_all(store.live_dir("admin/second")).unwrap();
        assert!(
            device_for_folder(&mut c, &store, "admin/second").unwrap().is_none(),
            "a user who can only read has no devices"
        );
    }

    #[test]
    fn what_is_not_the_folder_of_a_user_is_a_master_file() {
        let db = testing::db();
        let mut c = db.get().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        for (path, data) in [("loose.txt", &b"1"[..]), ("scans/2024/a.txt", b"2"), ("users/admin/laptop/b.txt", b"3")] {
            let p = store.live_root().join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, data).unwrap();
        }
        // The folder of the user is not part of the master files: its subfolder is a device of the user.
        scan(&db, &store, Duration::ZERO).unwrap();
        let paths: Vec<String> = live(&mut c).into_iter().map(|(p, _)| p).collect();
        assert_eq!(paths, ["b.txt", "loose.txt", "scans/2024/a.txt"]);
        let master: i64 = c.query_one("SELECT COUNT(*) FROM files WHERE device_id = 0", &[]).unwrap().get(0);
        assert_eq!(master, 2, "b.txt belongs to the device admin/laptop");

        let (found, rel) = resolve(&mut c, &store, "scans/2024/a.txt").unwrap().unwrap();
        assert_eq!((found.id, rel.as_str()), (0, "scans/2024/a.txt"));
        assert!(resolve(&mut c, &store, "users/admin").unwrap().is_none(), "the folder of a user is not a master file");
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
        fs::remove_dir_all(store.live_dir("admin/d")).unwrap();
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
