//! Offsite mirrors, driven by rclone.
//!
//! A mirror holds three plain directories that need nothing from fafnirx to read or restore:
//!   files/<key>/<path>                      the current file tree (the same layout as the master's own)
//!   versions/<UTC timestamp>/...            what a run replaced or removed (the last N runs are kept)
//!   meta/*.json                             statistics and a file list per folder
//!   meta/library.json                       albums, tags, favorites and corrected dates and places
//!
//! A mirror holds the whole tree or, with a folder, that folder at the same place below `files/`.
//! A `replica` writes these to the storage as they are. A `backup` writes them through an rclone crypt layer, encrypting the
//! names and the contents with the mirror's own key, so only rclone with that key reads them.
//!
//! Each run uploads only files whose content changed since the last successful run. A file that is new under the
//! content of a vanished path is moved on the remote instead. Changed files are
//! linked (or copied) from the file store into a staging directory in batches, so extra disk use is bounded.
//! Backend options reach rclone through environment variables, never through the command line. A mirror can instead
//! name a remote of an rclone config file ("name:path"), which is then the only thing rclone is told about.

mod rclone;

pub use self::rclone::*;
use crate::integrations::MirrorRun;
use crate::platform::db::{self, Db};
use crate::platform::store::{RemoveOnDrop, Store};
use crate::repo::links::{Due, MirrorKind};
use crate::repo::mirrors;
use anyhow::{bail, Context, Result};
use fafnirx_common::{unix_now, HASH_LEN};
use postgres::Client;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) const REMOTE: &str = "FXMIRROR";
/// Upper bounds for one staging batch.
const BATCH_BYTES: u64 = 512 << 20;
const BATCH_FILES: usize = 2000;
/// Renames moved on the remote at the same time.
const RENAME_PARALLEL: usize = 8;

/// A mirror that is due: its link, and the settings stored in the link.
pub struct MirrorRow {
    pub link: Due,
    pub config: mirrors::Config,
}

impl MirrorRow {
    pub fn new(link: Due) -> Self {
        let config = serde_json::from_str(&link.config).unwrap_or_default();
        MirrorRow { link, config }
    }

    /// A backup is encrypted with the two passwords, a replica holds the files as they are.
    fn kind(&self) -> MirrorKind {
        self.link.mirror_kind()
    }

    /// The key of the folder or device it holds; empty for everything.
    fn scope(&self) -> &str {
        &self.link.scope
    }
}

struct LiveFile {
    folder_id: i64,
    /// The folder's folder (`<user>/<folder>`, or `master:<name>`): where its files are on the master and on the mirror.
    ns: String,
    path: String,
    root: [u8; HASH_LEN],
    size: u64,
    mtime: i64,
}

impl LiveFile {
    /// Path relative to the `files/` directory on the mirror.
    fn rel(&self) -> String {
        relative(&self.ns, &self.path)
    }
}

/// Path relative to the `files/` directory, which is the same on the master and on the mirror: the files of a master folder
/// are at its root, the files of a folder below `users/<user>/<folder>`.
fn relative(folder: &str, path: &str) -> String {
    crate::platform::store::tree_path(folder, path)
}

/// Sortable UTC timestamp used for version directories.
pub fn run_stamp(unix: i64) -> String {
    let t = time::OffsetDateTime::from_unix_timestamp(unix).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    let format = time::format_description::parse_borrowed::<2>("[year][month][day]T[hour][minute][second]Z").expect("a valid format");
    t.format(&format).unwrap_or_default()
}

/// Puts the live file where rclone uploads from, under the name it has on the mirror. A hard link
/// costs no disk space; a copy is the fallback.
fn stage_file(store: &Store, dest: &Path, f: &LiveFile) -> Result<()> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let from = store.live_path(&f.ns, &f.path)?;
    let (root, size) = crate::platform::store::hash_file(&from)?;
    if size != f.size || root != f.root {
        bail!("stored file is corrupt on disk (it does not match its recorded hash)");
    }
    if !crate::platform::store::link_or_copy(&from, dest)? {
        let copy = std::fs::OpenOptions::new().write(true).open(dest)?;
        copy.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_nanos(f.mtime.max(0) as u64))?;
    }
    Ok(())
}

fn load_live(conn: &mut Client, folder_id: Option<i64>, device_id: Option<i64>) -> Result<Vec<LiveFile>> {
    Ok(mirrors::live_files(conn, folder_id, device_id)?
        .into_iter()
        .map(|f| LiveFile {
            folder_id: f.folder_id,
            ns: f.folder,
            path: f.path,
            root: f.root_hash.try_into().unwrap_or([0; HASH_LEN]),
            size: f.size as u64,
            mtime: f.mtime,
        })
        .collect())
}

/// folder id -> path -> root hash of what is on the mirror.
type Mirrored = HashMap<i64, HashMap<String, Vec<u8>>>;

fn load_mirrored(conn: &mut Client, mirror_id: i64) -> Result<Mirrored> {
    let mut mirrored = Mirrored::new();
    for (folder_id, path, root) in mirrors::items(conn, mirror_id)? {
        mirrored.entry(folder_id).or_default().insert(path, root);
    }
    Ok(mirrored)
}

/// A file that is on the mirror under another path: moving it there costs no upload.
#[derive(Clone)]
struct Rename<'a> {
    from: (i64, String),
    to: &'a LiveFile,
}

/// What differs between the master and the mirror.
struct Diff<'a> {
    /// Live files whose content is not on the mirror yet.
    changed: Vec<&'a LiveFile>,
    /// (folder id, path) of files on the mirror that are no longer live.
    removed: Vec<(i64, String)>,
    /// Files in `changed` and `removed` that are one file with a new path.
    renamed: Vec<Rename<'a>>,
}

fn diff<'a>(live: &'a [LiveFile], mirrored: &Mirrored) -> Diff<'a> {
    let on_mirror = |f: &LiveFile| mirrored.get(&f.folder_id).and_then(|files| files.get(&f.path));
    let live_keys: HashSet<(i64, &str)> = live.iter().map(|f| (f.folder_id, f.path.as_str())).collect();
    let mut removed: Vec<(i64, String, &Vec<u8>)> = mirrored
        .iter()
        .flat_map(|(folder, files)| files.iter().map(move |(path, root)| (*folder, path, root)))
        .filter(|(folder, path, _)| !live_keys.contains(&(*folder, path.as_str())))
        .map(|(folder, path, root)| (folder, path.clone(), root))
        .collect();
    removed.sort();
    // A path that is new and has the content of a vanished one is a rename or move; equal content is interchangeable.
    let mut vanished = fafnirx_common::Vanished::new(removed.into_iter().map(|(folder, path, root)| (root.clone(), (folder, path))));
    let (mut changed, mut renamed) = (Vec::new(), Vec::new());
    for f in live {
        match on_mirror(f) {
            Some(root) if root[..] == f.root[..] => {}
            Some(_) => changed.push(f),
            None => match vanished.take(&f.root.to_vec()) {
                Some(from) => renamed.push(Rename { from, to: f }),
                None => changed.push(f),
            },
        }
    }
    let mut removed = vanished.into_remaining();
    removed.sort();
    Diff { changed, removed, renamed }
}

/// Splits files into batches that fit the staging limits.
fn batches<'a>(files: &[&'a LiveFile], max_bytes: u64, max_files: usize) -> Vec<Vec<&'a LiveFile>> {
    let mut out = Vec::new();
    let (mut current, mut bytes) = (Vec::new(), 0u64);
    for &f in files {
        current.push(f);
        bytes += f.size;
        if bytes >= max_bytes || current.len() >= max_files {
            out.push(std::mem::take(&mut current));
            bytes = 0;
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn metadata(m: &MirrorRow, live: &[LiveFile], stamp: &str, run: &MirrorRun) -> BTreeMap<String, serde_json::Value> {
    let mut per_ns: BTreeMap<&str, Vec<&LiveFile>> = BTreeMap::new();
    for f in live {
        per_ns.entry(&f.ns).or_default().push(f);
    }
    let mut files = BTreeMap::new();
    let mut summary = Vec::new();
    for (ns, list) in &per_ns {
        let bytes: u64 = list.iter().map(|f| f.size).sum();
        summary.push(serde_json::json!({ "namespace": ns, "files": list.len(), "bytes": bytes }));
        let entries: Vec<_> = list
            .iter()
            .map(|f| serde_json::json!({ "path": f.path, "size": f.size, "mtime_ns": f.mtime, "root_hash": hex::encode(f.root) }))
            .collect();
        files.insert(format!("{}.json", ns.replace('/', "_")), serde_json::json!({ "namespace": ns, "files": entries }));
    }
    files.insert(
        "stats.json".into(),
        serde_json::json!({
            "mirror": m.link.name,
            "scope": m.scope(),
            "run": stamp,
            "generated_at_unix": unix_now(),
            "files": live.len(),
            "bytes": live.iter().map(|f| f.size).sum::<u64>(),
            "uploaded_this_run": run.uploaded,
            "removed_this_run": run.removed,
            "namespaces": summary,
        }),
    );
    files
}

pub async fn run(db: Db, store: Arc<Store>, data_dir: PathBuf, m: MirrorRow) -> Result<MirrorRun> {
    let stage_root = data_dir.join("mirror-stage").join(m.link.id.to_string());
    let _ = std::fs::remove_dir_all(&stage_root);
    std::fs::create_dir_all(&stage_root)?;
    // Removed however the run ends, a panic included.
    let _stage = RemoveOnDrop(stage_root.clone());
    let empty_config = stage_root.join("rclone.conf");
    std::fs::write(&empty_config, b"")?;
    let bin = rclone_bin();
    let mut env = rclone_env(&m, &empty_config, &crate::integrations::spec::rclone_config())?;
    if m.kind() == MirrorKind::Backup {
        let (password, salt) = (obscure(&bin, &m.config.crypt_password).await?, obscure(&bin, &m.config.crypt_salt).await?);
        env.extend(backup_env(&base_of(&m), &password, &salt));
    }
    let rc = Rclone { bin, env };
    let stamp = run_stamp(unix_now());
    let remotes = Remotes::new(&top_of(&m), &stamp);
    let job = Job { db, store, stage_root, rc, m, remotes, stamp };
    let result = job.run().await;
    if let Ok(o) = &result {
        let (id, files, bytes) = (job.m.link.id, o.total_files as i64, o.bytes as i64);
        let _ = db::with(&job.db, move |c| Ok(crate::repo::links::set_totals(c, id, files, bytes)?)).await;
    }
    result
}

/// Everything one mirror run needs.
struct Job {
    db: Db,
    store: Arc<Store>,
    stage_root: PathBuf,
    rc: Rclone,
    m: MirrorRow,
    remotes: Remotes,
    stamp: String,
}

impl Job {
    async fn run(&self) -> Result<MirrorRun> {
        // Fail early, before touching any data, if the remote is unusable.
        self.rc.run(&["mkdir", &self.remotes.root], &[]).await.context("cannot reach the mirror")?;

        let (mirror_id, folder_id, device_id) = (self.m.link.id, self.m.link.folder_id, self.m.link.device_id);
        let (live, mirrored) = db::with(&self.db, move |c| Ok((load_live(c, folder_id, device_id)?, load_mirrored(c, mirror_id)?))).await?;
        let Diff { mut changed, mut removed, renamed } = diff(&live, &mirrored);
        let mut out = MirrorRun { total_files: live.len(), ..Default::default() };

        // A rename that cannot be moved on the mirror is an upload and a removal, as it was before.
        for failed in self.move_renamed(renamed, &mut out).await? {
            removed.push(failed.from);
            changed.push(failed.to);
        }
        self.upload_changed(&changed, &mut out).await?;
        self.move_removed(&removed, &mut out).await?;
        self.upload_metadata(&live, &out).await?;
        self.prune_versions(&mut out).await;
        Ok(out)
    }

    /// Uploads files in bounded batches and records what is now on the mirror.
    async fn upload_changed(&self, changed: &[&LiveFile], out: &mut MirrorRun) -> Result<()> {
        let batches = batches(changed, BATCH_BYTES, BATCH_FILES);
        for (n, files) in batches.iter().enumerate() {
            let stage = self.stage_root.join(format!("batch{n}"));
            // Rebuilding reads and hashes a lot of data: tell the runtime this thread is about to block.
            let staged = tokio::task::block_in_place(|| files.iter().map(|f| (*f, stage_file(&self.store, &stage.join(f.rel()), f))).collect::<Vec<_>>());
            let mut good = Vec::new();
            for (f, result) in staged {
                match result {
                    Ok(()) => good.push(f),
                    Err(e) => out.failed.push(format!("{}: {e:#}", f.rel())),
                }
            }
            if !good.is_empty() {
                let args = [
                    "copy",
                    &stage.to_string_lossy(),
                    &self.remotes.files,
                    "--backup-dir",
                    &self.remotes.run,
                    "--ignore-times",
                    "--transfers",
                    "4",
                    "--retries",
                    "3",
                    "--stats",
                    "0",
                    "--log-level",
                    "ERROR",
                ];
                self.rc
                    .run(&args, &[])
                    .await
                    .with_context(|| format!("upload batch {} of {}", n + 1, batches.len()))?;
                let rows: Vec<(i64, String, Vec<u8>)> = good.iter().map(|f| (f.folder_id, f.path.clone(), f.root.to_vec())).collect();
                let mirror_id = self.m.link.id;
                db::with(&self.db, move |c| {
                    let mut tx = c.transaction()?;
                    for (folder_id, path, root) in rows {
                        mirrors::remember_item(&mut tx, mirror_id, folder_id, &path, &root)?;
                    }
                    Ok(tx.commit()?)
                })
                .await?;
                out.uploaded += good.len();
                out.bytes += good.iter().map(|f| f.size).sum::<u64>();
            }
            let _ = std::fs::remove_dir_all(&stage);
        }
        Ok(())
    }

    /// Moves renamed files to their new path on the mirror, a few at a time, and returns those that could not be moved.
    async fn move_renamed<'a>(&self, renamed: Vec<Rename<'a>>, out: &mut MirrorRun) -> Result<Vec<Rename<'a>>> {
        if renamed.is_empty() {
            return Ok(renamed);
        }
        let namespaces: HashMap<i64, String> = db::with(&self.db, |c| Ok(mirrors::folder_keys(c)?.into_iter().collect())).await?;
        let mut failed = Vec::new();
        for chunk in renamed.chunks(RENAME_PARALLEL) {
            let moves = chunk.iter().map(|r| async {
                let Some(ns) = namespaces.get(&r.from.0) else { return false };
                let from = join(&self.remotes.files, &relative(ns, &r.from.1));
                let to = join(&self.remotes.files, &r.to.rel());
                self.rc.run(&["moveto", &from, &to, "--stats", "0", "--log-level", "ERROR"], &[]).await.is_ok()
            });
            let done: Vec<bool> = futures_util::future::join_all(moves).await;
            let mut rows = Vec::new();
            for (r, ok) in chunk.iter().zip(&done) {
                if *ok {
                    rows.push((r.from.clone(), r.to.folder_id, r.to.path.clone(), r.to.root.to_vec()));
                } else {
                    failed.push(r.clone());
                }
            }
            let (mirror_id, moved) = (self.m.link.id, rows.len());
            db::with(&self.db, move |c| {
                let mut tx = c.transaction()?;
                for ((folder_id, path), to_device, to_path, root) in rows {
                    mirrors::forget_item(&mut tx, mirror_id, folder_id, &path)?;
                    mirrors::remember_item(&mut tx, mirror_id, to_device, &to_path, &root)?;
                }
                Ok(tx.commit()?)
            })
            .await?;
            out.moved += moved;
        }
        Ok(failed)
    }

    /// Files deleted on the master move into this run's version directory.
    async fn move_removed(&self, removed: &[(i64, String)], out: &mut MirrorRun) -> Result<()> {
        if removed.is_empty() {
            return Ok(());
        }
        let namespaces: HashMap<i64, String> = db::with(&self.db, |c| Ok(mirrors::folder_keys(c)?.into_iter().collect())).await?;
        let list: Vec<String> = removed.iter().filter_map(|(d, p)| namespaces.get(d).map(|ns| relative(ns, p))).collect();
        let list_file = self.stage_root.join("removed.txt");
        std::fs::write(&list_file, list.join("\n"))?;
        // Exit 3/4: the file or directory is already gone from the mirror, which is the goal.
        let args = [
            "move",
            &self.remotes.files,
            &self.remotes.run,
            "--files-from-raw",
            &list_file.to_string_lossy(),
            "--stats",
            "0",
            "--log-level",
            "ERROR",
        ];
        self.rc.run(&args, &[3, 4]).await.context("move deleted files into versions")?;
        let (mirror_id, gone) = (self.m.link.id, removed.to_vec());
        db::with(&self.db, move |c| {
            let mut tx = c.transaction()?;
            for (folder_id, path) in gone {
                mirrors::forget_item(&mut tx, mirror_id, folder_id, &path)?;
            }
            Ok(tx.commit()?)
        })
        .await?;
        out.removed = removed.len();
        Ok(())
    }

    /// Statistics and file lists as small JSON files.
    async fn upload_metadata(&self, live: &[LiveFile], out: &MirrorRun) -> Result<()> {
        let dir = self.stage_root.join("meta");
        std::fs::create_dir_all(&dir)?;
        for (name, value) in metadata(&self.m, live, &self.stamp, out) {
            std::fs::write(dir.join(name), serde_json::to_vec_pretty(&value)?)?;
        }
        // Albums, tags, favorites and corrected dates live in the database only: keep a readable copy. It names files all
        // over the tree, so a mirror of one folder or device leaves it out.
        if self.m.link.scope.is_empty() {
            let library = db::with(&self.db, crate::library::photos::snapshot).await?;
            std::fs::write(dir.join("library.json"), serde_json::to_vec_pretty(&library)?)?;
        }
        let args = [
            "copy",
            &dir.to_string_lossy(),
            &self.remotes.meta,
            "--ignore-times",
            "--stats",
            "0",
            "--log-level",
            "ERROR",
        ];
        self.rc.run(&args, &[]).await.context("upload metadata")?;
        Ok(())
    }

    /// Keeps only the newest version directories. Problems here are warnings: the data itself is safe.
    async fn prune_versions(&self, out: &mut MirrorRun) {
        let listing = match self.rc.run(&["lsf", "--dirs-only", &self.remotes.versions], &[3]).await {
            Ok(l) => l,
            Err(e) => return out.warnings.push(format!("could not list versions: {e:#}")),
        };
        let dirs = listing.lines().map(|l| l.trim().trim_end_matches('/').to_string()).collect();
        for old in expired_versions(dirs, self.m.config.keep_runs.max(1) as usize) {
            let dir = join(&self.remotes.base, &format!("versions/{old}"));
            if let Err(e) = self.rc.run(&["purge", &dir], &[3]).await {
                out.warnings.push(format!("could not remove old versions {old}: {e:#}"));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/integrations/mirror.rs"]
mod tests;
