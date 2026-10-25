//! Offsite mirrors, driven by rclone.
//!
//! A mirror holds three plain directories that need nothing from fafnirx to read or restore:
//!   files/<device>/<path>                   the current file tree (the same layout as the master's own)
//!   versions/<UTC timestamp>/...            what a run replaced or removed (the last N runs are kept)
//!   meta/*.json                             statistics and a file list per namespace
//!   meta/library.json                       albums, tags, favorites and corrected dates and places
//!
//! Each run uploads only files whose content changed since the last successful run. Changed files are
//! linked (or copied) from the file store into a staging directory in batches, so extra disk use is bounded.
//! Backend options reach rclone through environment variables, never through the command line. A mirror can instead
//! name a remote of an rclone config file ("name:path"), which is then the only thing rclone is told about.

use crate::integrations::{JobReport, MirrorRow};
use crate::platform::db::{self, Db};
use crate::platform::store::Store;
use crate::repo::mirrors;
use anyhow::{bail, Context, Result};
use fafnirx_common::{unix_now, HASH_LEN};
use postgres::Client;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const REMOTE: &str = "FXMIRROR";
/// Upper bounds for one staging batch.
const BATCH_BYTES: u64 = 512 << 20;
const BATCH_FILES: usize = 2000;

#[derive(Default, Debug)]
pub struct MirrorOutcome {
    pub uploaded: usize,
    pub bytes: u64,
    pub removed: usize,
    pub total_files: usize,
    pub warnings: Vec<String>,
    pub failed: Vec<String>,
}

impl MirrorOutcome {
    pub fn report(&self) -> JobReport {
        JobReport {
            summary: format!(
                "{} uploaded ({} bytes), {} removed, {} files on the mirror",
                self.uploaded, self.bytes, self.removed, self.total_files
            ),
            problems: self.failed.iter().chain(&self.warnings).cloned().collect(),
        }
    }
}

struct LiveFile {
    device_id: i64,
    /// The device's folder (`<user>/<device>`, or `_master`): where its files are on the master and on the mirror.
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

/// Path relative to the `files/` directory, which is the same on the master and on the mirror: the master files are
/// at its root, the files of a device below the folder of the device.
fn relative(folder: &str, path: &str) -> String {
    if folder == crate::platform::store::MASTER_FOLDER {
        path.to_string()
    } else {
        format!("{folder}/{path}")
    }
}

/// Sortable UTC timestamp used for version directories.
pub fn run_stamp(unix: i64) -> String {
    let t = time::OffsetDateTime::from_unix_timestamp(unix).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    format!("{:04}{:02}{:02}T{:02}{:02}{:02}Z", t.year(), t.month() as u8, t.day(), t.hour(), t.minute(), t.second())
}

/// Where a mirror's uploads go, as rclone writes it: the remote of the config file, or the one made from the options.
fn base_of(m: &MirrorRow) -> String {
    if m.remote.is_empty() {
        format!("{REMOTE}:{}", m.remote_path.trim_end_matches('/'))
    } else {
        m.remote.trim_end_matches('/').to_string()
    }
}

/// rclone configuration for the mirror, as environment variables. `config_file` is the rclone config file that a
/// mirror with a `remote` reads; the others get `empty_config`.
pub fn rclone_env(m: &MirrorRow, empty_config: &Path, config_file: &Path) -> Result<Vec<(String, String)>> {
    if !m.remote.is_empty() {
        if !config_file.is_file() {
            bail!(
                "mirror remote {:?} needs the rclone config file {}, which does not exist (set FAFNIRX_RCLONE_CONFIG)",
                m.remote,
                config_file.display()
            );
        }
        return Ok(vec![("RCLONE_CONFIG".to_string(), config_file.to_string_lossy().into_owned())]);
    }
    if m.remote_type.is_empty() || !m.remote_type.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("invalid remote type {:?}", m.remote_type);
    }
    let opts: BTreeMap<String, String> =
        serde_json::from_str(&m.options).context("mirror options are not valid JSON")?;
    let mut env = vec![
        ("RCLONE_CONFIG".to_string(), empty_config.to_string_lossy().into_owned()),
        (format!("RCLONE_CONFIG_{REMOTE}_TYPE"), m.remote_type.clone()),
    ];
    for (k, v) in opts {
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            bail!("invalid option name {k:?}");
        }
        if v.contains('\0') {
            bail!("invalid value for option {k}");
        }
        env.push((format!("RCLONE_CONFIG_{REMOTE}_{}", k.to_ascii_uppercase()), v));
    }
    Ok(env)
}

/// The places on the remote that a run writes to.
struct Remotes {
    base: String,
    root: String,
    files: String,
    versions: String,
    /// This run's directory under `versions/`: where replaced and removed files go.
    run: String,
    meta: String,
}

impl Remotes {
    fn new(base: &str, stamp: &str) -> Self {
        Self {
            base: base.to_string(),
            root: join(base, ""),
            files: join(base, "files"),
            versions: join(base, "versions"),
            run: join(base, &format!("versions/{stamp}")),
            meta: join(base, "meta"),
        }
    }
}

/// `sub` inside `base`, where `base` is an rclone path such as `FXMIRROR:bucket/prefix` or `name:`.
fn join(base: &str, sub: &str) -> String {
    let base = base.trim_end_matches('/');
    match (base.ends_with(':'), sub.is_empty()) {
        (_, true) => base.to_string(),
        (true, false) => format!("{base}{sub}"),
        (false, false) => format!("{base}/{sub}"),
    }
}

/// Names of the version directories to delete so that only the newest `keep` remain.
pub fn expired_versions(mut dirs: Vec<String>, keep: usize) -> Vec<String> {
    dirs.retain(|d| !d.is_empty());
    dirs.sort();
    let excess = dirs.len().saturating_sub(keep.max(1));
    dirs.into_iter().take(excess).collect()
}

struct Rclone {
    bin: String,
    env: Vec<(String, String)>,
}

impl Rclone {
    /// Runs rclone and returns stdout. Exit codes in `ok_codes` count as success.
    async fn run(&self, args: &[&str], ok_codes: &[i32]) -> Result<String> {
        let out = tokio::process::Command::new(&self.bin)
            .args(args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .with_context(|| format!("run {} (is rclone installed?)", self.bin))?;
        let code = out.status.code().unwrap_or(-1);
        if code != 0 && !ok_codes.contains(&code) {
            let err = String::from_utf8_lossy(&out.stderr);
            let tail: String = err.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
            bail!("rclone {} failed (exit {code}): {}", args.first().unwrap_or(&""), tail.trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
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

fn load_live(conn: &mut Client) -> Result<Vec<LiveFile>> {
    Ok(mirrors::live_files(conn)?
        .into_iter()
        .map(|f| LiveFile {
            device_id: f.device_id,
            ns: f.folder,
            path: f.path,
            root: f.root_hash.try_into().unwrap_or([0; HASH_LEN]),
            size: f.size as u64,
            mtime: f.mtime,
        })
        .collect())
}

/// device id -> path -> root hash of what is on the mirror.
type Mirrored = HashMap<i64, HashMap<String, Vec<u8>>>;

fn load_mirrored(conn: &mut Client, mirror_id: i64) -> Result<Mirrored> {
    let mut mirrored = Mirrored::new();
    for (device_id, path, root) in mirrors::items(conn, mirror_id)? {
        mirrored.entry(device_id).or_default().insert(path, root);
    }
    Ok(mirrored)
}

/// What differs between the master and the mirror.
struct Diff<'a> {
    /// Live files whose content is not on the mirror yet.
    changed: Vec<&'a LiveFile>,
    /// (device id, path) of files on the mirror that are no longer live.
    removed: Vec<(i64, String)>,
}

fn diff<'a>(live: &'a [LiveFile], mirrored: &Mirrored) -> Diff<'a> {
    let on_mirror = |f: &LiveFile| mirrored.get(&f.device_id).and_then(|files| files.get(&f.path));
    let changed = live.iter().filter(|f| on_mirror(f).is_none_or(|root| root[..] != f.root[..])).collect();
    let live_keys: HashSet<(i64, &str)> = live.iter().map(|f| (f.device_id, f.path.as_str())).collect();
    let removed = mirrored
        .iter()
        .flat_map(|(device, files)| files.keys().map(move |path| (*device, path)))
        .filter(|(device, path)| !live_keys.contains(&(*device, path.as_str())))
        .map(|(device, path)| (device, path.clone()))
        .collect();
    Diff { changed, removed }
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

fn metadata(m: &MirrorRow, live: &[LiveFile], stamp: &str, run: &MirrorOutcome) -> BTreeMap<String, serde_json::Value> {
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
        files
            .insert(format!("{}.json", ns.replace('/', "_")), serde_json::json!({ "namespace": ns, "files": entries }));
    }
    files.insert(
        "stats.json".into(),
        serde_json::json!({
            "mirror": m.name,
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

pub async fn run(db: Db, store: Arc<Store>, data_dir: PathBuf, m: MirrorRow) -> Result<MirrorOutcome> {
    let stage_root = data_dir.join("mirror-stage").join(m.id.to_string());
    let _ = std::fs::remove_dir_all(&stage_root);
    std::fs::create_dir_all(&stage_root)?;
    // Removed however the run ends, a panic included.
    let _stage = RemoveOnDrop(stage_root.clone());
    let empty_config = stage_root.join("rclone.conf");
    std::fs::write(&empty_config, b"")?;
    let rc = Rclone {
        bin: std::env::var("FAFNIRX_RCLONE").unwrap_or_else(|_| "rclone".into()),
        env: rclone_env(&m, &empty_config, &crate::integrations::spec::rclone_config())?,
    };
    let stamp = run_stamp(unix_now());
    let remotes = Remotes::new(&base_of(&m), &stamp);
    let job = Job { db, store, stage_root, rc, m, remotes, stamp };
    let result = job.run().await;
    if let Ok(o) = &result {
        let (id, files, bytes) = (job.m.id, o.total_files as i64, o.bytes as i64);
        let _ = db::with(&job.db, move |c| Ok(mirrors::set_totals(c, id, files, bytes)?)).await;
    }
    result
}

/// Removes a directory when dropped.
struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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
    async fn run(&self) -> Result<MirrorOutcome> {
        // Fail early, before touching any data, if the remote is unusable.
        self.rc.run(&["mkdir", &self.remotes.root], &[]).await.context("cannot reach the mirror")?;

        let mirror_id = self.m.id;
        let (live, mirrored) = db::with(&self.db, move |c| Ok((load_live(c)?, load_mirrored(c, mirror_id)?))).await?;
        let diff = diff(&live, &mirrored);
        let mut out = MirrorOutcome { total_files: live.len(), ..Default::default() };

        self.upload_changed(&diff.changed, &mut out).await?;
        self.move_removed(&diff.removed, &mut out).await?;
        self.upload_metadata(&live, &out).await?;
        self.prune_versions(&mut out).await;
        Ok(out)
    }

    /// Uploads files in bounded batches and records what is now on the mirror.
    async fn upload_changed(&self, changed: &[&LiveFile], out: &mut MirrorOutcome) -> Result<()> {
        let batches = batches(changed, BATCH_BYTES, BATCH_FILES);
        for (n, files) in batches.iter().enumerate() {
            let stage = self.stage_root.join(format!("batch{n}"));
            // Rebuilding reads and hashes a lot of data: tell the runtime this thread is about to block.
            let staged = tokio::task::block_in_place(|| {
                files.iter().map(|f| (*f, stage_file(&self.store, &stage.join(f.rel()), f))).collect::<Vec<_>>()
            });
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
                let rows: Vec<(i64, String, Vec<u8>)> =
                    good.iter().map(|f| (f.device_id, f.path.clone(), f.root.to_vec())).collect();
                let mirror_id = self.m.id;
                db::with(&self.db, move |c| {
                    let mut tx = c.transaction()?;
                    for (device_id, path, root) in rows {
                        mirrors::remember_item(&mut tx, mirror_id, device_id, &path, &root)?;
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

    /// Files deleted on the master move into this run's version directory.
    async fn move_removed(&self, removed: &[(i64, String)], out: &mut MirrorOutcome) -> Result<()> {
        if removed.is_empty() {
            return Ok(());
        }
        let namespaces: HashMap<i64, String> =
            db::with(&self.db, |c| Ok(mirrors::device_folders(c)?.into_iter().collect())).await?;
        let list: Vec<String> =
            removed.iter().filter_map(|(d, p)| namespaces.get(d).map(|ns| relative(ns, p))).collect();
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
        let (mirror_id, gone) = (self.m.id, removed.to_vec());
        db::with(&self.db, move |c| {
            let mut tx = c.transaction()?;
            for (device_id, path) in gone {
                mirrors::forget_item(&mut tx, mirror_id, device_id, &path)?;
            }
            Ok(tx.commit()?)
        })
        .await?;
        out.removed = removed.len();
        Ok(())
    }

    /// Statistics and file lists as small JSON files.
    async fn upload_metadata(&self, live: &[LiveFile], out: &MirrorOutcome) -> Result<()> {
        let dir = self.stage_root.join("meta");
        std::fs::create_dir_all(&dir)?;
        for (name, value) in metadata(&self.m, live, &self.stamp, out) {
            std::fs::write(dir.join(name), serde_json::to_vec_pretty(&value)?)?;
        }
        // Albums, tags, favorites and corrected dates live in the database only: keep a readable copy.
        let library = db::with(&self.db, crate::library::photos::snapshot).await?;
        std::fs::write(dir.join("library.json"), serde_json::to_vec_pretty(&library)?)?;
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
    async fn prune_versions(&self, out: &mut MirrorOutcome) {
        let listing = match self.rc.run(&["lsf", "--dirs-only", &self.remotes.versions], &[3]).await {
            Ok(l) => l,
            Err(e) => return out.warnings.push(format!("could not list versions: {e:#}")),
        };
        let dirs = listing.lines().map(|l| l.trim().trim_end_matches('/').to_string()).collect();
        for old in expired_versions(dirs, self.m.keep_runs.max(1) as usize) {
            let dir = join(&self.remotes.base, &format!("versions/{old}"));
            if let Err(e) = self.rc.run(&["purge", &dir], &[3]).await {
                out.warnings.push(format!("could not remove old versions {old}: {e:#}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(options: &str) -> MirrorRow {
        MirrorRow {
            id: 1,
            name: "m".into(),
            remote_type: "s3".into(),
            options: options.into(),
            remote_path: "bucket/prefix/".into(),
            remote: String::new(),
            keep_runs: 3,
        }
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
        let env = rclone_env(
            &row(r#"{"access_key_id":"k","secret_access_key":"s"}"#),
            Path::new("/tmp/c"),
            Path::new("/none"),
        )
        .unwrap();
        assert!(env.contains(&("RCLONE_CONFIG_FXMIRROR_TYPE".into(), "s3".into())));
        assert!(env.contains(&("RCLONE_CONFIG_FXMIRROR_SECRET_ACCESS_KEY".into(), "s".into())));
        assert!(
            rclone_env(&row(r#"{"Bad-Key":"x"}"#), Path::new("/tmp/c"), Path::new("/none")).is_err(),
            "option names cannot smuggle other variables"
        );
        let mut bad = row("{}");
        bad.remote_type = "s3;rm".into();
        assert!(rclone_env(&bad, Path::new("/tmp/c"), Path::new("/none")).is_err());
    }

    #[test]
    fn a_remote_of_the_rclone_config_is_used_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let conf = dir.path().join("rclone.conf");
        let mut m = row("{}");
        m.remote = "crypt:backups/fafnirx/".into();
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
            ("crypt:backups/fafnirx", "crypt:backups/fafnirx/files", "crypt:backups/fafnirx/versions/20240101T000000Z")
        );
        m.remote = "crypt:".into();
        assert_eq!(Remotes::new(&base_of(&m), "s").files, "crypt:files", "a remote without a path");
    }

    #[test]
    fn remote_paths_join_cleanly() {
        assert_eq!(join("FXMIRROR:bucket/prefix/", "files"), "FXMIRROR:bucket/prefix/files");
        assert_eq!(join("FXMIRROR:/abs/dir", "files"), "FXMIRROR:/abs/dir/files");
        assert_eq!(join("FXMIRROR:", "files"), "FXMIRROR:files");
        assert_eq!(join("FXMIRROR:b", ""), "FXMIRROR:b");
        let mut m = row("{}");
        m.remote_path = "b/x".into();
        let r = Remotes::new(&base_of(&m), "20240101T000000Z");
        assert_eq!(
            (r.files.as_str(), r.run.as_str()),
            ("FXMIRROR:b/x/files", "FXMIRROR:b/x/versions/20240101T000000Z")
        );
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
        assert_eq!(
            changed,
            [(1, "edited"), (1, "new"), (2, "same")],
            "same path on another device is a different file"
        );
        assert_eq!(d.removed, [(1, "deleted".to_string())]);
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
}
