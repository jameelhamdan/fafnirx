//! What the client says about itself (the status file), and the markers other programs leave for it, in the control folder.

use crate::config::control_dir;
use crate::sync::Report;
use crate::update;
use fafnirx_common::unix_now;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// What the engine is doing right now, shared with the status writer.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Activity {
    /// idle | scanning | hashing | uploading
    pub phase: String,
    /// Remote path of the file being worked on.
    pub current: String,
    pub done: usize,
    pub total: usize,
    /// Bytes sent so far in this cycle.
    pub bytes: u64,
    /// Sizes of the files to upload this cycle, and of those already finished before the current one.
    pub total_bytes: u64,
    pub base_bytes: u64,
    /// Size of the current file, and the cycle's sent counter when it started.
    pub file_size: u64,
    pub file_start: u64,
    /// Filled in by the status writer: progress of the current file, of the whole cycle, and upload speed.
    pub file_bytes: u64,
    pub done_bytes: u64,
    pub speed_bps: u64,
}

/// What one shared folder attached to this computer is doing, for `status`, the tray and the master.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SharedStatus {
    /// `<owner>/<name>`.
    pub name: String,
    pub path: String,
    /// Files and bytes in step with the master.
    pub files: usize,
    pub bytes: u64,
    /// When the last cycle for this folder finished (unix seconds), 0 before the first.
    pub last_cycle: i64,
    /// The last problem of that cycle, empty when it went well.
    pub last_error: String,
    /// Deletions held back (not applied here, or not sent), see the delete guard.
    pub held: usize,
    /// Files waiting for a retry.
    pub failing: usize,
}

impl SharedStatus {
    /// One line for `status` and the tray: whether it is in step, how much, how long ago, and what needs attention.
    pub fn summary(&self, now: i64) -> String {
        let mut parts = Vec::new();
        if self.last_cycle == 0 {
            parts.push("waiting for the first sync".to_string());
        } else {
            let ok = self.last_error.is_empty() && self.failing == 0;
            parts.push(if ok { "in step" } else { "needs attention" }.to_string());
            parts.push(format!("{} files", self.files));
            parts.push(format!("synced {} ago", age(now - self.last_cycle)));
        }
        if self.held > 0 {
            parts.push(format!("{} deletions held back", self.held));
        }
        if self.failing > 0 {
            parts.push(format!("{} files waiting for a retry", self.failing));
        }
        if !self.last_error.is_empty() {
            parts.push(format!("last problem: {}", self.last_error.chars().take(120).collect::<String>()));
        }
        parts.join(", ")
    }
}

/// `45s`, `12m`, `3h`, `2d`.
pub fn age(seconds: i64) -> String {
    match seconds.max(0) {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Status {
    pub version: String,
    pub pid: u32,
    /// When the daemon started (unix seconds).
    pub started: i64,
    pub server: String,
    /// Device name on the master, as confirmed by the last heartbeat.
    pub device: String,
    pub online: bool,
    /// Why the master is unreachable, when it is.
    pub message: String,
    pub last_cycle: i64,
    /// True while a collection cycle (scan and upload) is running.
    pub syncing: bool,
    pub activity: Activity,
    /// The last finished cycle.
    pub last: Report,
    pub folders: usize,
    /// Everything the master has from this device, as of the last cycle.
    pub synced_files: usize,
    pub synced_bytes: u64,
    /// Files that failed and are waiting to be retried.
    pub failing: usize,
    /// Totals since the daemon started.
    pub session_files: usize,
    pub session_bytes: u64,
    /// When the next periodic scan is due (unix seconds).
    pub next_scan: i64,
    pub bandwidth_limit_kbps: u64,
    pub last_error: String,
    /// Why the client could not start or had to stop. Written as it exits, since nobody sees the output of a
    /// hidden client; the tray shows it until the next start clears it.
    pub fatal: String,
    /// True until every file of the collected folders has been sent once: the first sync of a new install.
    pub initial_sync: bool,
    /// The shared folders attached here.
    pub shared: Vec<SharedStatus>,
    /// The shared folders the user can use on the master, with whether each is their own: what the tray offers to attach.
    pub shared_on_master: Vec<(String, bool)>,
    pub updated: i64,
}

pub struct StatusFile {
    path: PathBuf,
    inner: Mutex<Status>,
}

impl StatusFile {
    pub fn new(path: PathBuf) -> Self {
        let status = Status {
            version: update::VERSION.into(),
            pid: std::process::id(),
            started: unix_now(),
            ..Default::default()
        };
        Self { path, inner: Mutex::new(status) }
    }

    pub fn snapshot(&self) -> Status {
        self.inner.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn edit(&self, f: impl FnOnce(&mut Status)) {
        let Ok(mut s) = self.inner.lock() else { return };
        f(&mut s);
        s.updated = unix_now();
        if let Ok(json) = serde_json::to_vec_pretty(&*s) {
            let tmp = self.path.with_extension("tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &self.path);
            }
        }
    }
}

pub fn read_status(state_dir: &Path) -> Option<Status> {
    serde_json::from_slice(&std::fs::read(status_file(state_dir)).ok()?).ok()
}

/// What the client says about itself, for the tray and status.
pub fn status_file(state_dir: &Path) -> PathBuf {
    control_dir(state_dir).join("status.json")
}

pub fn log_file(state_dir: &Path) -> PathBuf {
    control_dir(state_dir).join("fafnirx.log")
}

/// Created to pause the uploads; removed to resume.
pub fn paused_marker(state_dir: &Path) -> PathBuf {
    control_dir(state_dir).join("paused")
}

/// Created by the tray (or by hand) to start a cycle now. The daemon removes it.
pub fn sync_now_marker(state_dir: &Path) -> PathBuf {
    control_dir(state_dir).join("sync-now")
}

/// Created to ask the daemon to exit. The daemon removes it.
pub fn stop_marker(state_dir: &Path) -> PathBuf {
    control_dir(state_dir).join("stop")
}

/// Created to ask the daemon to update and start again on the new version. The daemon removes it.
pub fn update_marker(state_dir: &Path) -> PathBuf {
    control_dir(state_dir).join("update")
}

/// Touched by `fafnirx restart`: every tray that started before it was touched starts over (it is a new file time, not a
/// request that is taken away, because one folder serves the trays of all users).
pub fn restart_tray_marker(state_dir: &Path) -> PathBuf {
    control_dir(state_dir).join("restart-tray")
}

/// What the daemon answers to an update marker: the text of the marker (so an old answer is not taken for a new one), then `current <version>`, `updated <version>` or `failed: <why>`.
pub fn update_result_file(state_dir: &Path) -> PathBuf {
    control_dir(state_dir).join("update-result")
}
