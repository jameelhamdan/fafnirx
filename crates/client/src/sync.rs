//! One collection cycle: scan, detect renames and deletes, upload what changed.
//! Upload only: nothing here ever asks the orchestrator for data.

use crate::config::{Config, Folder};
use crate::daemon::Activity;
use crate::hash::{hash_file, stat, Hashed};
use crate::net::Client;
use crate::scan::{scan_folder, Entry};
use crate::state::{may_attempt, State, Synced, BACKOFF_SECS};
use crate::throttle::Limiter;
use fafnirx_common::{chunk_hash, pb, unix_now, CHUNK_SIZE};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

const UPLOAD_WORKERS: usize = 4;

#[derive(Debug)]
pub enum SyncError {
    /// The orchestrator cannot be used right now. Nothing is lost: changes stay pending.
    Offline(String),
    /// A problem with this one file.
    File(String),
    Paused,
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Offline(m) | SyncError::File(m) => f.write_str(m),
            SyncError::Paused => f.write_str("paused"),
        }
    }
}

impl From<tonic::Status> for SyncError {
    fn from(s: tonic::Status) -> Self {
        use tonic::Code::*;
        let msg = format!("{:?}: {}", s.code(), s.message());
        match s.code() {
            Unavailable | DeadlineExceeded | Cancelled | Unknown | Unauthenticated | ResourceExhausted | Internal => {
                SyncError::Offline(msg)
            }
            _ => SyncError::File(msg),
        }
    }
}

impl From<anyhow::Error> for SyncError {
    fn from(e: anyhow::Error) -> Self {
        SyncError::File(format!("{e:#}"))
    }
}

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        SyncError::File(e.to_string())
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub uploaded: usize,
    pub renamed: usize,
    pub deleted: usize,
    pub failed: usize,
    pub bytes: u64,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} uploaded, {} renamed, {} deleted, {} failed",
            self.uploaded, self.renamed, self.deleted, self.failed
        )
    }
}

pub struct Engine {
    pub client: Client,
    pub state: State,
    pub limiter: Arc<Limiter>,
    /// Errors waiting to be reported with the next heartbeat.
    pub errors: Arc<Mutex<Vec<String>>>,
    /// While this file exists nothing is uploaded.
    pub pause_marker: PathBuf,
    /// What the engine is doing now, read by the daemon's status writer.
    pub activity: Arc<Mutex<Activity>>,
    /// Bytes sent in the current cycle, updated per chunk.
    pub sent: Arc<AtomicU64>,
    logged_skips: HashSet<String>,
}

fn top(path: &str) -> &str {
    path.split('/').next().unwrap_or("")
}

/// A file to hash, and whether it is new (never uploaded before).
type Changed = (Entry, bool);

struct Candidate {
    entry: Entry,
    hashed: Hashed,
    renamed_from: Option<String>,
}

impl Engine {
    pub fn new(
        client: Client,
        state: State,
        limiter: Arc<Limiter>,
        errors: Arc<Mutex<Vec<String>>>,
        pause_marker: PathBuf,
    ) -> Self {
        Self {
            client,
            state,
            limiter,
            errors,
            pause_marker,
            activity: Arc::new(Mutex::new(Activity::default())),
            sent: Arc::new(AtomicU64::new(0)),
            logged_skips: HashSet::new(),
        }
    }

    pub fn set_activity(&self, phase: &str, current: &str, done: usize, total: usize, bytes: u64) {
        if let Ok(mut a) = self.activity.lock() {
            *a = Activity { phase: phase.into(), current: current.into(), done, total, bytes };
        }
    }

    fn report_error(&self, msg: String) {
        tracing::warn!("{msg}");
        if let Ok(mut e) = self.errors.lock() {
            if e.len() < 50 {
                e.push(msg);
            }
        }
    }

    fn check_paused(&self) -> Result<(), SyncError> {
        if self.pause_marker.exists() {
            Err(SyncError::Paused)
        } else {
            Ok(())
        }
    }

    pub async fn cycle(&mut self, cfg: &Config) -> Result<Report, SyncError> {
        let mut report = Report::default();
        self.limiter.set(cfg.bandwidth_limit_kbps);
        self.sent.store(0, Ordering::Relaxed);
        self.set_activity("scanning", "", 0, 0, 0);
        let (entries, scanned) = self.scan(cfg).await;
        let (changed, mut vanished) = self.detect_changes(&entries, &scanned)?;
        let candidates = self.hash_changed(changed, &mut vanished, &mut report).await?;
        self.upload_all(candidates, &mut report).await?;
        self.delete_vanished(vanished, &mut report).await?;
        Ok(report)
    }

    /// Walks every folder. Returns the files found and the names of the folders that could be read.
    async fn scan(&mut self, cfg: &Config) -> (Vec<Entry>, HashSet<String>) {
        let mut entries = Vec::new();
        let mut scanned = HashSet::new();
        for folder in &cfg.folders {
            let f: Folder = folder.clone();
            match tokio::task::spawn_blocking(move || scan_folder(&f)).await {
                Ok(Ok(scan)) => {
                    for note in scan.skipped {
                        if self.logged_skips.insert(note.clone()) {
                            tracing::warn!("skipped {note}");
                        }
                    }
                    scanned.insert(folder.name.clone());
                    entries.extend(scan.entries);
                }
                Ok(Err(e)) => self.report_error(format!("{}: {e:#}", folder.name)),
                Err(e) => self.report_error(format!("{}: scan failed: {e}", folder.name)),
            }
        }
        (entries, scanned)
    }

    /// Returns the files to hash (with whether they are new) and the synced paths that vanished from
    /// folders we could read.
    fn detect_changes(
        &self,
        entries: &[Entry],
        scanned: &HashSet<String>,
    ) -> Result<(Vec<Changed>, Vec<Synced>), SyncError> {
        let t = unix_now();
        let mut changed = Vec::new();
        for e in entries {
            let synced = self.state.synced(&e.remote)?;
            if synced.as_ref().is_some_and(|s| s.size == e.size as i64 && s.mtime_ns == e.mtime_ns) {
                continue;
            }
            let retry = self.state.retry(&e.remote)?;
            if may_attempt(&retry, e.size as i64, e.mtime_ns, t) {
                changed.push((e.clone(), synced.is_none()));
            }
        }
        let seen: HashSet<&str> = entries.iter().map(|e| e.remote.as_str()).collect();
        let vanished = self
            .state
            .all_synced()?
            .into_iter()
            .filter(|s| scanned.contains(top(&s.path)) && !seen.contains(s.path.as_str()))
            .collect();
        Ok((changed, vanished))
    }

    /// Hashes the changed files and pairs new ones with vanished files of the same content (renames).
    /// A paired file is taken out of `vanished`.
    async fn hash_changed(
        &mut self,
        changed: Vec<Changed>,
        vanished: &mut Vec<Synced>,
        report: &mut Report,
    ) -> Result<Vec<Candidate>, SyncError> {
        let mut candidates = Vec::new();
        let total = changed.len();
        for (n, (entry, is_new)) in changed.into_iter().enumerate() {
            self.check_paused()?;
            self.set_activity("hashing", &entry.remote, n, total, 0);
            let p = entry.local.clone();
            let hashed = match tokio::task::spawn_blocking(move || hash_file(&p)).await.map_err(anyhow::Error::from) {
                Ok(Ok(h)) => h,
                Ok(Err(e)) | Err(e) => {
                    self.fail(&entry, &format!("{e:#}"));
                    report.failed += 1;
                    continue;
                }
            };
            let renamed_from = if is_new {
                vanished
                    .iter()
                    .position(|v| v.root_hash == hashed.root && v.size as u64 == hashed.size)
                    .map(|pos| vanished.remove(pos).path)
            } else {
                None
            };
            candidates.push(Candidate { entry, hashed, renamed_from });
        }
        Ok(candidates)
    }

    async fn upload_all(&mut self, candidates: Vec<Candidate>, report: &mut Report) -> Result<(), SyncError> {
        let total = candidates.len();
        for (n, c) in candidates.into_iter().enumerate() {
            self.check_paused()?;
            self.set_activity("uploading", &c.entry.remote, n, total, 0);
            match self.upload(&c).await {
                Ok(bytes) => {
                    report.bytes += bytes;
                    if c.renamed_from.is_some() {
                        report.renamed += 1;
                    } else {
                        report.uploaded += 1;
                    }
                }
                Err(SyncError::File(msg)) => {
                    self.fail(&c.entry, &msg);
                    report.failed += 1;
                }
                Err(other) => return Err(other),
            }
        }
        Ok(())
    }

    /// The orchestrator keeps the data and only marks the path deleted.
    async fn delete_vanished(&mut self, vanished: Vec<Synced>, report: &mut Report) -> Result<(), SyncError> {
        for v in vanished {
            self.check_paused()?;
            self.client.mark_deleted(pb::DeleteRequest { path: v.path.clone() }).await?;
            self.state.remove_synced(&v.path)?;
            report.deleted += 1;
        }
        Ok(())
    }

    fn fail(&self, e: &Entry, msg: &str) {
        match self.state.record_failure(&e.remote, e.size as i64, e.mtime_ns, unix_now(), msg) {
            Ok(count) if count as usize >= BACKOFF_SECS.len() => {
                self.report_error(format!("{}: {msg} (giving up until the file changes)", e.remote))
            }
            Ok(_) => self.report_error(format!("{}: {msg} (will retry)", e.remote)),
            Err(x) => self.report_error(format!("{}: {msg}; also failed to record it: {x}", e.remote)),
        }
    }

    async fn upload(&mut self, c: &Candidate) -> Result<u64, SyncError> {
        let h = &c.hashed;
        let manifest = pb::Manifest {
            path: c.entry.remote.clone(),
            size: h.size,
            root_hash: h.root.to_vec(),
            chunk_hashes: h.chunks.iter().map(|x| x.to_vec()).collect(),
        };
        let neg = self.client.negotiate(manifest.clone()).await?.into_inner();
        let mut bytes = 0;
        if !neg.already_current {
            if !neg.missing.is_empty() {
                bytes = self.send_chunks(&c.entry.local, h, &neg.missing).await?;
            }
            // The file must be unchanged since it was hashed.
            let (size, mtime) = stat(&c.entry.local)?;
            if size != h.size || mtime != h.mtime_ns {
                return Err(SyncError::File("file changed during upload".into()));
            }
            self.client
                .commit(pb::CommitRequest {
                    manifest: Some(manifest),
                    mtime: h.mtime_ns,
                    mode: c.entry.mode,
                    renamed_from: c.renamed_from.clone().unwrap_or_default(),
                })
                .await?;
        }
        if let Some(old) = &c.renamed_from {
            // The commit above already marked the old path deleted on the orchestrator.
            self.state.remove_synced(old)?;
        }
        self.state.set_synced(&Synced {
            path: c.entry.remote.clone(),
            size: h.size as i64,
            mtime_ns: h.mtime_ns,
            root_hash: h.root.to_vec(),
        })?;
        Ok(bytes)
    }

    async fn send_chunks(&self, local: &Path, h: &Hashed, missing: &[u32]) -> Result<u64, SyncError> {
        let workers = UPLOAD_WORKERS.min(missing.len());
        let mut groups: Vec<Vec<u32>> = vec![vec![]; workers];
        for (i, idx) in missing.iter().enumerate() {
            groups[i % workers].push(*idx);
        }
        let mut set = tokio::task::JoinSet::new();
        for g in groups {
            set.spawn(send_group(
                self.client.clone(),
                self.limiter.clone(),
                self.sent.clone(),
                local.to_path_buf(),
                h.chunks.clone(),
                h.size,
                g,
            ));
        }
        let mut total = 0;
        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(n)) => total += n,
                Ok(Err(e)) => {
                    set.abort_all();
                    return Err(e);
                }
                Err(e) => {
                    set.abort_all();
                    return Err(SyncError::File(format!("upload task failed: {e}")));
                }
            }
        }
        Ok(total)
    }
}

/// Streams the chunks at `indexes` to the orchestrator over one gRPC stream.
async fn send_group(
    mut client: Client,
    limiter: Arc<Limiter>,
    sent: Arc<AtomicU64>,
    path: PathBuf,
    hashes: Vec<[u8; 32]>,
    size: u64,
    indexes: Vec<u32>,
) -> Result<u64, SyncError> {
    let (tx, rx) = mpsc::channel::<pb::Chunk>(2);
    let producer = tokio::spawn(async move {
        let io = |e: std::io::Error| SyncError::File(format!("read failed: {e}"));
        let mut file = tokio::fs::File::open(&path).await.map_err(io)?;
        let mut total = 0u64;
        for idx in indexes {
            let offset = idx as u64 * CHUNK_SIZE as u64;
            let len = (size - offset).min(CHUNK_SIZE as u64) as usize;
            file.seek(SeekFrom::Start(offset)).await.map_err(io)?;
            let mut buf = vec![0u8; len];
            file.read_exact(&mut buf).await.map_err(|_| SyncError::File("file changed during upload".into()))?;
            if chunk_hash(&buf) != hashes[idx as usize] {
                return Err(SyncError::File("file changed during upload".into()));
            }
            limiter.acquire(len).await;
            total += len as u64;
            if tx.send(pb::Chunk { hash: hashes[idx as usize].to_vec(), data: buf }).await.is_err() {
                break;
            }
            sent.fetch_add(len as u64, Ordering::Relaxed);
        }
        Ok(total)
    });
    let response = client.upload_chunks(ReceiverStream::new(rx)).await;
    let total = producer.await.map_err(|e| SyncError::File(format!("reader failed: {e}")))??;
    response?;
    Ok(total)
}
