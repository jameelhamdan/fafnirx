//! The live tree (`files/users/<user>/<device>/<path>`, and the master folders `files/<name>/<path>`) can also be used as a
//! network drive, so files appear, change and vanish there without a client. This notices that and brings the
//! database in line: new and changed files become versions, removed files are marked deleted. A new folder
//! `files/users/<user>/<name>` with a valid name becomes a device of that user (kind `local`), and a new folder `files/<name>` a
//! master folder, unless it would be inside, or around, another.
//!
//! A file system watcher queues the paths that were touched; once a path has been quiet for a moment
//! it is compared with the database. What the master itself just wrote matches its record (same size
//! and modification time) and is skipped without being read. Watchers miss things (network file
//! systems, overflowing queues), so the whole tree is also compared now and then.

use crate::files::ingest::{self, Seen};
use crate::platform::db::{self, Db, Device};
use crate::platform::store::USERS_DIR;
use crate::platform::store::{hash_file, Store, MASTER_PREFIX};
use crate::repo::devices::{self, DeviceKind};
use crate::repo::{alerts, files, users};
use anyhow::Result;
use fafnirx_common::{clean_remote_path, mode_of, mtime_ns, unix_now, valid_device_name};
use fafnirx_db::client::GenericClient;
use fafnirx_db::queries as sql;
use notify::{RecursiveMode, Watcher};
use postgres::Client;
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

/// Held while the folders of the live tree are looked at and while one is renamed, so that the watcher never sees a folder
/// under its new name before the database knows it (it would adopt it as a new device) or under its old one after.
pub static LAYOUT: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn layout() -> std::sync::MutexGuard<'static, ()> {
    LAYOUT.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs [`scan`] unless one is already running. Returns whether it ran.
fn scan_once(db: &Db, store: &Store) -> bool {
    if SCANNING.swap(true, Ordering::AcqRel) {
        return false;
    }
    let _layout = layout();
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

/// The names of the users: their folders in `files/` are not master files.
fn user_names(conn: &mut impl GenericClient) -> Result<HashSet<String>> {
    Ok(users::names(conn)?.into_iter().collect())
}

/// The device and the path inside it that a path of the live tree is about: under `users/<user>/` the next part is a
/// device of that user, or else the path is in a master folder.
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
    master_for(conn, store, rel)
}

/// The master folder a path of `files/` is in, and the path inside it. A top-level folder that is not a master folder yet
/// becomes one.
fn master_for(conn: &mut Client, store: &Store, rel: &str) -> Result<Option<(Device, String)>> {
    for (id, folder) in devices::master_devices(conn)? {
        let name = folder.strip_prefix(MASTER_PREFIX).unwrap_or(&folder);
        let inside = if rel == name { Some("") } else { rel.strip_prefix(name).and_then(|r| r.strip_prefix('/')) };
        if let Some(inside) = inside {
            return Ok(Some((Device { id, name: folder }, inside.to_string())));
        }
    }
    let first = rel.split('/').next().unwrap_or("");
    Ok(adopt_master_dir(conn, store, first)?.map(|dev| (dev, rel.strip_prefix(first).and_then(|r| r.strip_prefix('/')).unwrap_or("").to_string())))
}

/// Makes the top-level folder `files/<name>` a master folder when it is a folder with a name that can be one, and is not inside
/// another master folder or around one.
fn adopt_master_dir(conn: &mut Client, store: &Store, name: &str) -> Result<Option<Device>> {
    let storable = !name.starts_with('.') && name.to_lowercase() != USERS_DIR && clean_remote_path(name).ok().as_deref() == Some(name);
    if !storable || fafnirx_common::not_portable(name).is_some() || !store.live_dir(&format!("{MASTER_PREFIX}{name}")).is_dir() {
        return Ok(None);
    }
    if !devices::master_names_overlapping(conn, name)?.is_empty() {
        return Ok(None);
    }
    let Some(folder) = devices::insert_master_folder(conn, name, unix_now())? else { return Ok(None) };
    let Some((id, _)) = devices::by_folder(conn, &folder)? else { return Ok(None) };
    tracing::info!("new folder files/{name}: added as a master folder");
    db::notify(conn, db::Topic::Devices, &folder)?;
    Ok(Some(Device { id, name: folder }))
}

fn work(db: &Db, store: &Store, targets: &[Target]) {
    let _layout = layout();
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
    // Only a local or shared folder is written to in place. What a device or a source sends is a copy of theirs and is not
    // changed from here: edits made inside such a folder are not recorded.
    if let Some((id, kind)) = devices::id_and_kind(conn, folder)? {
        return Ok((kind == DeviceKind::Local).then(|| Device { id, name: folder.to_string() }));
    }
    let Some((user, name)) = folder.split_once('/') else { return Ok(None) };
    if !valid_device_name(name) || !store.live_dir(folder).is_dir() {
        return Ok(None);
    }
    let Some(owner) = users::who_can_have_devices(conn, user)? else { return Ok(None) };
    let id = sql::devices::ensure_local_device().bind(conn, &name, &unix_now(), &owner).one()?;
    tracing::info!("new folder files/{folder}: added as a device");
    db::notify(conn, db::Topic::Devices, folder)?;
    Ok(Some(Device { id, name: folder.to_string() }))
}

/// Compares the master folders and every device folder of every user with the database. `settle` is how long a file
/// must have been left alone to be recorded.
pub fn scan(db: &Db, store: &Store, settle: Duration) -> Result<()> {
    let mut conn = db.get()?;
    let top: Vec<String> = std::fs::read_dir(store.live_root())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    for name in top {
        if let Err(e) = adopt_master_dir(&mut conn, store, &name) {
            tracing::error!("master folder {name}: {e:#}");
        }
    }
    let mut devices: Vec<Device> = devices::master_devices(&mut *conn)?.into_iter().map(|(id, name)| Device { id, name }).collect();
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
    if let Err(e) = check_layout(&mut conn, store) {
        tracing::error!("check of the layout of the file store failed: {e:#}");
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

/// How many names an alert lists.
const LISTED: usize = 10;

/// Files and folders that are not recorded, because they are not inside a device folder of a user or a master folder: a file in
/// `files/`, in `users/` or in the folder of a user, a folder that is not a user, a device folder with a name that cannot be used.
/// Hidden names (`.DS_Store`, `.Trash`) are not mentioned.
fn misplaced(store: &Store, names: &HashSet<String>) -> Vec<String> {
    let entries = |dir: &Path| -> Vec<(String, bool)> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| Some((e.file_name().into_string().ok()?, e.file_type().ok()?.is_dir())))
            .filter(|(name, _)| !name.starts_with('.'))
            .collect()
    };
    let mut out = Vec::new();
    // A file straight in `files/` is in no folder.
    for (name, is_dir) in entries(store.live_root()) {
        if !is_dir {
            out.push(name);
        }
    }
    for (name, is_dir) in entries(&store.live_root().join(USERS_DIR)) {
        if !is_dir || !names.contains(&name) {
            out.push(format!("{USERS_DIR}/{name}"));
            continue;
        }
        for (inside, is_dir) in entries(&store.user_dir(&name)) {
            if !is_dir || !valid_device_name(&inside) {
                out.push(format!("{USERS_DIR}/{name}/{inside}"));
            }
        }
    }
    out.sort();
    out
}

/// Tells the administrators about what the layout of `files/` leaves out, with an alert that closes itself when it is put
/// right: files that are not in a device folder of a user or in a master folder.
fn check_layout(conn: &mut Client, store: &Store) -> Result<()> {
    let names = user_names(conn)?;
    let list = |items: &[String]| {
        let shown: Vec<&str> = items.iter().take(LISTED).map(String::as_str).collect();
        let more = items.len().saturating_sub(LISTED);
        format!("{}{}", shown.join(", "), if more > 0 { format!(" and {more} more") } else { String::new() })
    };
    let stray = misplaced(store, &names);
    let message = (!stray.is_empty()).then(|| {
        format!(
            "Not recorded, because they are not inside a folder (files/<name>/ for the master, files/users/<user>/<device>/ for a user): {}",
            list(&stray)
        )
    });
    Ok(alerts::set(conn, "misplaced_files", USERS_DIR, message.as_deref())?)
}

/// Brings the database in line with `rel` (a file or a folder; `""` is the whole device) on disk.
/// Returns (files recorded, files marked deleted).
fn sync_subtree(conn: &mut Client, store: &Store, dev: &Device, rel: &str, settle: Duration) -> Result<(usize, usize)> {
    // No folder: nothing was stored yet, or the volume is not mounted. Either way, delete nothing.
    if !store.live_dir(&dev.name).is_dir() {
        return Ok((0, 0));
    }
    let Ok(base) = (if rel.is_empty() { Ok(store.live_dir(&dev.name)) } else { store.live_path(&dev.name, rel) }) else {
        return Ok((0, 0)); // a name that cannot be stored cannot be tracked
    };
    let known: HashMap<String, (i64, i64)> = files::live_below(conn, dev.id, rel)?
        .into_iter()
        .map(|(path, size, mtime)| (path, (size, mtime)))
        .collect();

    let files = match std::fs::symlink_metadata(&base) {
        Ok(m) if m.is_dir() => walk(&base, rel),
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
    let mut vanished: Vec<(&str, (i64, i64))> = known.iter().filter(|(p, _)| !found.contains(p.as_str())).map(|(p, v)| (p.as_str(), *v)).collect();
    let mut changed = 0;
    for (path, full) in &files {
        let renamed_from = if known.contains_key(path) { None } else { take_rename(&mut vanished, full) };
        if check_file(
            conn,
            store,
            dev,
            &Candidate { path, full, before: Before { known: known.get(path).copied(), renamed_from } },
            settle,
        )? {
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
    let mut hits = vanished
        .iter()
        .enumerate()
        .filter(|(_, (_, (s, m)))| *s == size && (m - mtime).abs() < MTIME_SLACK_NS);
    let (index, _) = hits.next()?;
    if hits.next().is_some() {
        return None;
    }
    Some(vanished.remove(index).0.to_string())
}

/// Records a file if it differs from `known` (size, mtime of its current version). Returns whether a
/// version was added.
/// A file of the live tree that is looked at: its path, where it is on disk and what was known of it.
struct Candidate<'a> {
    path: &'a str,
    full: &'a Path,
    before: Before,
}

fn check_file(conn: &mut Client, store: &Store, dev: &Device, file: &Candidate, settle: Duration) -> Result<bool> {
    let Candidate { path, full, before } = file;
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
    let found = ingest::Found { path, seen: &seen, renamed_from: before.renamed_from.as_deref() };
    Ok(hashed == size && ingest::adopt_live(conn, store, dev, &found)?)
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
#[path = "../../tests/unit/files/livetree.rs"]
mod tests;
