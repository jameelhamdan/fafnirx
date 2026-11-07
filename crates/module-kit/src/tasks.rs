//! What the service is doing right now (listing a source, fetching a file) and what failed lately, so that whoever runs it can
//! see why a source is slow (the module's `GET /status`). Kept in memory: a restart starts the list again.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// How many failures are remembered.
const KEEP_FAILED: usize = 20;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[derive(Serialize, Clone)]
pub struct Active {
    id: u64,
    /// `list` or `fetch`
    kind: &'static str,
    /// What it is about: the file's name for a fetch, the link for a listing.
    path: String,
    /// False while it waits for a free slot.
    running: bool,
    since: u64,
}

#[derive(Serialize, Clone)]
pub struct Failed {
    kind: &'static str,
    path: String,
    error: String,
    at: u64,
}

#[derive(Serialize)]
pub struct Report {
    /// How many fetches can run at once.
    slots: usize,
    active: Vec<Active>,
    failed: Vec<Failed>,
    /// Finished since the service started.
    made: u64,
}

#[derive(Default)]
pub struct Tasks {
    next: AtomicU64,
    made: AtomicU64,
    active: Mutex<Vec<Active>>,
    failed: Mutex<VecDeque<Failed>>,
}

/// One piece of work in the list; finishing it (or dropping it) takes it out.
pub struct Task<'a> {
    tasks: &'a Tasks,
    id: u64,
    kind: &'static str,
    path: String,
    done: bool,
}

impl Tasks {
    /// Work that is waiting for a slot.
    pub fn begin(&self, kind: &'static str, path: &str) -> Task<'_> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let entry = Active { id, kind, path: path.to_string(), running: false, since: now() };
        self.active.lock().unwrap_or_else(|e| e.into_inner()).push(entry);
        Task { tasks: self, id, kind, path: path.to_string(), done: false }
    }

    pub fn report(&self, slots: usize) -> Report {
        Report {
            slots,
            active: self.active.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            failed: self.failed.lock().unwrap_or_else(|e| e.into_inner()).iter().rev().cloned().collect(),
            made: self.made.load(Ordering::Relaxed),
        }
    }
}

impl Task<'_> {
    /// It has its slot and runs.
    pub fn started(&self) {
        let mut active = self.tasks.active.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(a) = active.iter_mut().find(|a| a.id == self.id) {
            a.running = true;
            a.since = now();
        }
    }

    /// It ended: `error` says why not, when it failed.
    pub fn finish(mut self, error: Option<String>) {
        self.done = true;
        self.tasks.active.lock().unwrap_or_else(|e| e.into_inner()).retain(|a| a.id != self.id);
        match error {
            None => {
                self.tasks.made.fetch_add(1, Ordering::Relaxed);
            }
            Some(error) => {
                let mut failed = self.tasks.failed.lock().unwrap_or_else(|e| e.into_inner());
                if failed.len() == KEEP_FAILED {
                    failed.pop_front();
                }
                let error = error.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").chars().take(300).collect();
                failed.push_back(Failed { kind: self.kind, path: self.path.clone(), error, at: now() });
            }
        }
    }
}

impl Drop for Task<'_> {
    /// A request that went away while the work waited or ran (the master stopped waiting) leaves no entry behind.
    fn drop(&mut self) {
        if !self.done {
            self.tasks.active.lock().unwrap_or_else(|e| e.into_inner()).retain(|a| a.id != self.id);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/tasks.rs"]
mod tests;
