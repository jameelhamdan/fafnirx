//! What the service is making right now (segments, subtitle tracks) and what failed lately, so that whoever runs it can see
//! why a video is slow to start. Kept in memory: a restart starts the list again.

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
    /// `segment` or `subtitles`
    kind: &'static str,
    /// The video, by its place in the file tree.
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
    /// How many can run at once.
    slots: usize,
    active: Vec<Active>,
    failed: Vec<Failed>,
    /// Made since the service started.
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
                // The last line is what ffmpeg complained about; the rest is its banner.
                let error =
                    error.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").chars().take(300).collect();
                failed.push_back(Failed { kind: self.kind, path: self.path.clone(), error, at: now() });
            }
        }
    }
}

impl Drop for Task<'_> {
    /// A request that went away while the work waited or ran leaves no entry behind.
    fn drop(&mut self) {
        if !self.done {
            self.tasks.active.lock().unwrap_or_else(|e| e.into_inner()).retain(|a| a.id != self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_is_listed_while_it_waits_and_runs_and_gone_after() {
        let tasks = Tasks::default();
        let a = tasks.begin("segment", "u/d/film.mkv");
        assert!(!tasks.report(2).active[0].running, "waiting for a slot");
        a.started();
        assert!(tasks.report(2).active[0].running);
        a.finish(None);
        let r = tasks.report(2);
        assert!(r.active.is_empty());
        assert_eq!((r.made, r.failed.len()), (1, 0));
    }

    #[test]
    fn a_failure_is_remembered_with_its_last_line_and_a_dropped_request_is_not() {
        let tasks = Tasks::default();
        tasks.begin("subtitles", "u/d/a.mkv").finish(Some("banner\nreal reason\n".into()));
        drop(tasks.begin("segment", "u/d/b.mkv"));
        let r = tasks.report(1);
        assert!(r.active.is_empty());
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].error, "real reason");
        assert_eq!(r.made, 0);
    }
}
