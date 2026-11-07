//! Synced folders: a local folder kept in step with a folder of the master, in both directions.
//!
//! A cycle receives what changed on the master, then sends what changed here with the engine that uploads the collected
//! folders, so a file changed on both sides is noticed before the upload replaces the other side's version. Nothing is
//! overwritten silently: what becomes of this computer's version depends on the folder's mode (the master's is
//! kept and this one put aside; this one is sent; both are kept; or a person is asked).
//!
//! `State::synced` records each path as it was when last in step. A local file that still matches it is unchanged here, so
//! the master's change is safe to apply; one that does not was changed here too.

mod conflicts;
mod files;
mod receive;
mod spellings;

use self::files::*;
use self::spellings::Spellings;
use crate::config::{Config, Folder};
use crate::daemon::{self, SyncedStatus};
use crate::net;
use crate::sync::guard;
use crate::sync::hash::stat;
use crate::sync::state::{State, Synced};
use crate::sync::throttle::Limiter;
use crate::sync::{Engine, Report, SyncError};
use anyhow::Result;
use fafnirx_common::{chunk_hash, clean_remote_path, fold_path, not_portable, pb, unix_now};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A file being received is written under this prefix next to its place and renamed when it is complete and checked.
pub const PART_PREFIX: &str = ".fafnirx-part-";

/// Where this computer's version of a file goes when the master's is kept (`Mode::Both`): beside the files, never synced.
pub const CONFLICT_DIR: &str = ".fafnirx-conflicts";

/// Where in the state the point up to which the master's changes were seen is kept.
const CURSOR: &str = "synced_cursor";

/// A synced folder attached to a folder of this computer.
pub struct Attached {
    engine: Engine,
    synced: Folder,
    host: String,
    spellings: Spellings,
    read_only: bool,
}

/// What the master listed: the point to continue from next time, and the paths.
/// A path the master changed, and where it goes on this computer.
#[derive(Clone, Copy)]
struct Incoming<'a> {
    path: &'a str,
    target: &'a Path,
    change: &'a pb::Change,
}

struct Listing {
    cursor: i64,
    /// The paths are every live one, not the changes: the master does not remember every deletion since the last time (it was
    /// asked from the start, or this computer was away longer than deleted files are kept).
    complete: bool,
    changes: Vec<pb::Change>,
}

/// What a listing asks for here, worked out before anything is changed.
#[derive(Default)]
struct Plan {
    /// Paths to receive: remote path, place here, what the master has.
    live: Vec<(String, PathBuf, pb::Change)>,
    /// Files unchanged here that were deleted on the master.
    deletions: Vec<(Synced, PathBuf)>,
    /// Which of those were deleted in the web app: deliberate and in the Trash, so applied at once, not held.
    from_web: HashSet<String>,
    /// The paths to receive, by folded spelling.
    folded: HashSet<String>,
    /// A change that could not be applied is looked at again next time: the cursor may not pass.
    hold_cursor: bool,
}

/// What the master did to a file that was changed here too.
enum Master<'a> {
    Deleted,
    Live(&'a pb::Change),
}

/// What a conflict came to on this computer.
enum Settled {
    /// This computer's version is out of the way: the master's may take its place.
    Receive,
    /// The file here stays as it is, and nothing is received for it now.
    Leave,
}

/// What became of the deletions the master listed.
enum Applied {
    Done,
    /// A large part of the folder: waiting for a person.
    Held,
    /// A person chose to keep the files.
    Kept,
}

/// How far before its own time the master lists changes (`list_changes` rewinds its cursor by this much).
const LISTING_LAG: i64 = 2;

/// The state database of a synced folder: one per folder, beside the client's own. `key` is the stable id of the folder (`Folder::key`),
/// so that a folder renamed on the master keeps its state.
pub fn state_file(state_dir: &Path, key: &str) -> PathBuf {
    let id = hex::encode(&chunk_hash(key.as_bytes())[..4]);
    state_dir.join(format!("synced-{id}.db"))
}

/// What a synced folder is doing, from its state database alone. It is all kept there (what is in step, what is held back,
/// how the last cycle ended), so `status` and `release-deletes` work with the client stopped.
pub fn status_of(synced: &Folder, state: &State) -> SyncedStatus {
    let (files, bytes) = state.totals().unwrap_or((0, 0));
    let (last_cycle, last_error) = state.outcome().unwrap_or_default();
    SyncedStatus {
        name: synced.name.clone(),
        path: synced.path.display().to_string(),
        files,
        bytes,
        last_cycle,
        last_error,
        mode: synced.mode.name().to_string(),
        held: state.held_files(),
        failing: state.pending_failures().unwrap_or(0) as usize,
        // Only the running client knows (the master says); `Attached::status` sets it.
        read_only: false,
        conflicts: state
            .pendings()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.choice.is_none())
            .take(50)
            .map(|p| daemon::ConflictInfo {
                path: p.path,
                deleted: p.deleted,
                master_size: p.size,
                master_mtime: p.mtime,
                local_size: p.local_size,
                local_mtime: p.local_mtime_ns.div_euclid(1_000_000_000),
            })
            .collect(),
    }
}

impl Attached {
    pub fn open(cfg: &Config, token: &str, state_dir: &Path, synced: &Folder, errors: Arc<Mutex<Vec<String>>>) -> Result<Self> {
        let client = net::synced_client(cfg, token, &crate::service::instance::client_id(state_dir), synced.key())?;
        let mut state = State::open(&state_file(state_dir, synced.key()))?;
        state.adopt_folder(&synced.path.display().to_string())?;
        let mut engine = Engine::new(
            client,
            state,
            Arc::new(Limiter::new(cfg.bandwidth_limit_kbps)),
            errors,
            daemon::paused_marker(state_dir),
        );
        engine.exclude = Some(state_dir.to_path_buf());
        engine.group = guard::at_the_top;
        let host = gethostname::gethostname().to_string_lossy().into_owned();
        Ok(Self { engine, synced: synced.clone(), host, spellings: Spellings::new(&synced.path), read_only: false })
    }

    pub fn name(&self) -> &str {
        &self.synced.name
    }

    /// Whether this is still the folder attached in the config.
    pub fn is(&self, synced: &Folder) -> bool {
        self.synced.name == synced.name && self.synced.path == synced.path
    }

    /// Receives, then sends, and notes how it went for `status`.
    pub async fn cycle(&mut self, cfg: &Config) -> Result<Report, SyncError> {
        self.engine.take_last_issue();
        let result = self.run_cycle(cfg).await;
        let outcome = match &result {
            Ok(_) => (unix_now(), self.engine.take_last_issue()),
            Err(e) => (self.engine.state.outcome().map_or(0, |(last, _)| last), e.to_string()),
        };
        if let Err(e) = self.engine.state.set_outcome(outcome.0, &outcome.1) {
            tracing::warn!("{}: cannot note how the cycle went: {e:#}", self.synced.name);
        }
        result
    }

    /// What this folder is doing, for the status file, the tray and the master.
    pub fn status(&self) -> SyncedStatus {
        SyncedStatus { read_only: self.read_only, ..status_of(&self.synced, &self.engine.state) }
    }

    /// The user may read this folder but not change it: what the master has is received, and nothing is sent (the master would
    /// refuse every file). Files changed here stay as they are until the master's version replaces them.
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    async fn run_cycle(&mut self, cfg: &Config) -> Result<Report, SyncError> {
        self.engine.check_master().await?;
        let mut report = Report::default();
        // Sending only: nothing of the master's is asked for, so nothing is received or deleted here.
        if self.synced.mode.pulls() {
            self.pull(&mut report).await?;
        }
        if self.read_only {
            return Ok(report);
        }
        // A file waiting for a person is neither sent nor taken for deleted.
        self.engine.skip = self.engine.state.pending_paths()?;
        // The engine collects one folder with no name of its own: the files of a synced folder are at the top.
        let mut one = cfg.clone();
        one.folders = vec![Folder::computer(self.synced.path.clone(), String::new())];
        let pushed = self.engine.cycle(&one).await?;
        report.add(&pushed);
        Ok(report)
    }

    /// Receives what changed on the master since the last time: asks, works out what to do, receives, then deletes.
    async fn pull(&mut self, report: &mut Report) -> Result<(), SyncError> {
        // What was read of the folders is only good for one pull: the person changes them in between.
        self.spellings.clear();
        self.apply_answers(report).await?;
        let listing = self.list_changes().await?;
        let cursor = listing.cursor;
        let mut plan = self.plan(listing, report)?;
        let live = std::mem::take(&mut plan.live);
        let total = live.len();
        for (n, (path, target, change)) in live.into_iter().enumerate() {
            self.engine.set_activity("downloading", &path, n, total);
            if let Err(e) = self
                .changed_on_master(&Incoming { path: &path, target: &target, change: &change }, &mut plan.deletions, report)
                .await
            {
                plan.hold_cursor |= self.file_failed(&path, e, report)?;
            }
        }
        let mut keep_cursor_back = plan.hold_cursor;
        let mut next = cursor;
        match self.apply_deletions(plan.deletions, &plan.from_web, &plan.folded, report)? {
            Applied::Held => keep_cursor_back = true,
            // The master lists a little before the time it names, so that a write racing with it is not missed, and from the
            // second it names on. The deletions that were kept are not to be listed (and held) again: the point goes past that
            // second, which a write by another computer in that very second would miss until it changes again.
            Applied::Kept => next = cursor + LISTING_LAG + 1,
            Applied::Done => {}
        }
        if !keep_cursor_back {
            self.engine.state.set_meta(CURSOR, &next.to_string())?;
        }
        Ok(())
    }

    async fn list_changes(&mut self) -> Result<Listing, SyncError> {
        let since: i64 = self.engine.state.meta(CURSOR)?.and_then(|v| v.parse().ok()).unwrap_or(0);
        let mut stream = self
            .engine
            .client
            .list_changes(pb::ListChangesRequest { folder: self.synced.key().to_string(), since })
            .await?
            .into_inner();
        let (mut cursor, mut complete, mut changes) = (None, false, Vec::new());
        while let Some(event) = stream.message().await? {
            match event.item {
                Some(pb::change_event::Item::Cursor(c)) => cursor = Some(c),
                Some(pb::change_event::Item::Change(c)) => changes.push(c),
                Some(pb::change_event::Item::Complete(c)) => complete = c,
                None => {}
            }
        }
        let cursor = cursor.ok_or_else(|| SyncError::Offline("the master listed no changes and no point to continue from".into()))?;
        Ok(Listing { cursor, complete, changes })
    }

    /// Sorts a listing into what to receive and what to delete, before anything here is changed.
    fn plan(&self, listing: Listing, report: &mut Report) -> Result<Plan, SyncError> {
        let mut plan = Plan::default();
        // The paths to receive by their folded spelling: of two that are one name, the first (they come sorted) is kept.
        let mut names: HashMap<String, String> = HashMap::new();
        let mut listed = HashSet::new();
        for change in listing.changes {
            let Ok(path) = clean_remote_path(&change.path) else { continue };
            listed.insert(path.clone());
            if change.deleted {
                self.queue_deletion(&path, change.from_web, &mut plan, report)?;
                continue;
            }
            let folded = fold_path(&path);
            if path.split('/').next() == Some(CONFLICT_DIR) {
                self.skipped(&path, "that folder is where this computer puts its own versions aside");
            } else if let Some(why) = not_portable(&path) {
                self.skipped(&path, &why);
            } else if let Some(first) = names.get(&folded).filter(|first| **first != path) {
                self.skipped(&path, &format!("differs only by case from {first:?}"));
            } else {
                names.insert(folded, path.clone());
                let target = local_path(&self.synced.path, &path);
                plan.live.push((path, target, change));
            }
        }
        if listing.complete {
            // Whatever was in step and is not listed was deleted while this computer was away. It goes the way a deletion
            // that was listed goes: unchanged here, it is deleted (unless that is a large part of the folder); changed here,
            // it is kept under another name.
            for base in self.engine.state.all_synced()? {
                if !listed.contains(&base.path) {
                    self.queue_deletion(&base.path, false, &mut plan, report)?;
                }
            }
        }
        plan.folded = names.into_keys().collect();
        Ok(plan)
    }

    /// Adds a path deleted on the master to the plan when the file here is unchanged.
    fn queue_deletion(&self, path: &str, from_web: bool, plan: &mut Plan, report: &mut Report) -> Result<(), SyncError> {
        let target = local_path(&self.synced.path, path);
        match self.deleted_on_master(path, &target, report) {
            Ok(Some(base)) => {
                if from_web {
                    plan.from_web.insert(path.to_string());
                }
                plan.deletions.push((base, target));
            }
            Ok(None) => {}
            Err(e) => plan.hold_cursor |= self.file_failed(path, e, report)?,
        }
        Ok(())
    }

    /// A file that is not received because this computer cannot hold it the way every other computer does. It is said once
    /// per listing and not retried: the same rule applies on every kind of computer.
    fn skipped(&self, path: &str, why: &str) {
        self.engine.report_error(format!("{}/{path}: not received: {why}", self.synced.name));
    }

    /// A problem with one file is counted and reported and the next one goes on; a problem with the master ends the cycle.
    /// Returns true, to hold the cursor back.
    fn file_failed(&self, path: &str, e: SyncError, report: &mut Report) -> Result<bool, SyncError> {
        match e {
            SyncError::File(msg) => {
                report.failed += 1;
                self.engine.report_error(format!("{}/{path}: {msg} (will retry)", self.synced.name));
                Ok(true)
            }
            other => Err(other),
        }
    }

    /// A path was deleted on the master. Returns the record of the file to delete here when it is unchanged here.
    fn deleted_on_master(&self, path: &str, target: &Path, report: &mut Report) -> Result<Option<Synced>, SyncError> {
        let Some(base) = self.engine.state.synced(path)? else { return Ok(None) };
        match local_stat(target)? {
            None => {
                self.engine.state.remove_synced(path)?;
                self.engine.state.remove_pending(path)?;
                Ok(None)
            }
            Some(st) if unchanged(&base, st) => {
                self.engine.state.remove_pending(path)?;
                Ok(Some(base))
            }
            Some(_) => {
                // Changed here, deleted there: the folder's policy says which is kept.
                self.settle(path, target, Master::Deleted, report)?;
                Ok(None)
            }
        }
    }

    /// A path has other content on the master than it had when it was last in step.
    async fn changed_on_master(&mut self, incoming: &Incoming<'_>, deletions: &mut Vec<(Synced, PathBuf)>, report: &mut Report) -> Result<(), SyncError> {
        let Incoming { path, target, change } = *incoming;
        let base = self.engine.state.synced(path)?;
        if base.as_ref().is_some_and(|b| b.root_hash == change.root_hash) {
            return Ok(());
        }
        // Another spelling of this name is here (a different case, or of a folder above it). Every kind of computer treats that as
        // one name, so the file is not received, unless the other spelling is what the master just renamed.
        if let Some(clash) = self.spellings.clash_with(path) {
            let Some(i) = deletions.iter().position(|(_, old_target)| *old_target == clash) else {
                self.skipped(path, &format!("differs only by case from {}", clash.display()));
                return Ok(());
            };
            let (old, _) = deletions.remove(i);
            if self.follow_case_rename(incoming, &old, report)? {
                return Ok(());
            }
        }
        if let Some(st) = local_stat(target)? {
            if !base.as_ref().is_some_and(|b| unchanged(b, st)) {
                let local = hash_in_background(target).await?;
                if local.root[..] == change.root_hash[..] {
                    // The same content on both sides already (a copy put there by hand, an upload that was interrupted).
                    self.record(path, target, &change.root_hash)?;
                    self.engine.state.remove_pending(path)?;
                    return Ok(());
                }
                if let Settled::Leave = self.settle(path, target, Master::Live(change), report)? {
                    return Ok(());
                }
            }
        }
        self.engine.state.remove_pending(path)?;
        // The content of a file that is deleted on the master is what a rename moved: move it here too.
        if let Some(i) = deletions.iter().position(|(b, _)| b.root_hash == change.root_hash) {
            let (old, old_target) = deletions.remove(i);
            make_parent(&self.synced.path, target)?;
            std::fs::rename(&old_target, target)?;
            self.spellings.removed(&old_target);
            self.spellings.added(target);
            self.engine.state.remove_synced(&old.path)?;
            self.record(path, target, &change.root_hash)?;
            report.downloaded += 1;
            return Ok(());
        }
        self.receive_here(path, target, change, report).await
    }

    /// Receives the master's version of a file into its place and notes it as in step.
    async fn receive_here(&mut self, path: &str, target: &Path, change: &pb::Change, report: &mut Report) -> Result<(), SyncError> {
        if self.download(path, target, change, report).await? {
            self.spellings.added(target);
            self.record(path, target, &change.root_hash)?;
            report.downloaded += 1;
        }
        Ok(())
    }

    /// The master renamed `old` to `path`, which differs from it only in case. The folders above are renamed here the same,
    /// whole: a file system that ignores case would keep the old spelling of one and send that back. Returns true when that
    /// was all (same content, the file is moved); false when the old file is gone and the new content is still to be received.
    fn follow_case_rename(&mut self, incoming: &Incoming<'_>, old: &Synced, report: &mut Report) -> Result<bool, SyncError> {
        let Incoming { path, target, change } = *incoming;
        self.spellings.respell_folders(path)?;
        // Where the old file is now: under its own name if that is all that differs, otherwise already at the new one.
        let old_now = self.spellings.clash_with(path);
        if old.root_hash == change.root_hash {
            if let Some(old_now) = old_now {
                std::fs::rename(&old_now, target)?;
                self.spellings.removed(&old_now);
                self.spellings.added(target);
            }
            self.engine.state.remove_synced(&old.path)?;
            self.record(path, target, &change.root_hash)?;
            report.downloaded += 1;
            return Ok(true);
        }
        // Other content under the new spelling: the old name goes first, so that the new one can take its place.
        let old_file = old_now.as_deref().unwrap_or(target);
        std::fs::remove_file(old_file)?;
        self.spellings.removed(old_file);
        self.engine.state.remove_synced(&old.path)?;
        Ok(false)
    }

    /// Deletes here what was deleted on the master. Deletions by other computers that are a large part of the folder wait
    /// until released or kept (see `Applied`).
    fn apply_deletions(&mut self, deletions: Vec<(Synced, PathBuf)>, from_web: &HashSet<String>, folded: &HashSet<String>, report: &mut Report) -> Result<Applied, SyncError> {
        let synced = self.engine.state.totals()?.0;
        // What was deleted in the web app is applied whatever its number; the guard is for what other computers delete.
        let (web, deletions): (Vec<_>, Vec<_>) = deletions.into_iter().partition(|(base, _)| from_web.contains(&base.path));
        self.delete_here(web, folded, report)?;
        let state = &self.engine.state;
        if !deletions.is_empty() && !state.release_requested()? && guard::tripped(deletions.len(), synced) {
            if state.take_keep()? {
                // The person said to keep the files: they stay, and are no longer in step with the master, so that they are sent
                // again.
                for (base, _) in &deletions {
                    state.remove_synced(&base.path)?;
                }
                state.clear_pull_hold()?;
                self.engine
                    .report_error(format!("{}: kept the {} files that were deleted on the server", self.synced.name, deletions.len()));
                return Ok(Applied::Kept);
            }
            // Held until a person decides: there is no time after which it is applied by itself.
            state.hold_pull(deletions.len(), unix_now())?;
            self.engine.report_error(format!(
                "{name}: {n} files were deleted on the server, a large part of the folder. They are not deleted here. \
                 `fafnirx release-deletes` deletes them here; `fafnirx folder keep {name}` keeps them",
                name = self.synced.name,
                n = deletions.len()
            ));
            return Ok(Applied::Held);
        }
        // Not held (any more): a request to keep that was not needed is not left to catch the next one.
        state.take_keep()?;
        state.clear_pull_hold()?;
        self.delete_here(deletions, folded, report)?;
        Ok(Applied::Done)
    }

    /// `folded`: the paths being received, by folded spelling.
    fn delete_here(&mut self, deletions: Vec<(Synced, PathBuf)>, folded: &HashSet<String>, report: &mut Report) -> Result<(), SyncError> {
        for (base, target) in deletions {
            // Where the file is here. Not under the spelling it was deleted with when a folder above it was renamed to another
            // case (whole, see `follow_case_rename`): it is then under the new one. When the path itself was renamed to another
            // case, what is here under the new spelling is the file just received: nothing to delete.
            let here = if self.spellings.exists_spelled(&base.path) {
                Some(target)
            } else if folded.contains(&fold_path(&base.path)) {
                None
            } else {
                self.spellings.clash_with(&base.path)
            };
            let Some(here) = here else {
                self.engine.state.remove_synced(&base.path)?;
                continue;
            };
            // Edited since it was listed (downloads can take minutes): settled, as for a file changed on both sides.
            if local_stat(&here)?.is_some_and(|st| !unchanged(&base, st)) {
                self.settle(&base.path, &here, Master::Deleted, report)?;
                continue;
            }
            match std::fs::remove_file(&here) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    self.engine.report_error(format!("{}/{}: {e}", self.synced.name, base.path));
                    continue;
                }
            }
            self.spellings.removed(&here);
            self.engine.state.remove_synced(&base.path)?;
            report.deleted += 1;
        }
        Ok(())
    }

    /// Notes that the local file of a path, which has the content with this hash, is the one in step with the master.
    fn record(&self, path: &str, target: &Path, root: &[u8]) -> Result<(), SyncError> {
        let (size, mtime_ns) = stat(target)?;
        let synced = Synced { path: path.to_string(), size: size as i64, mtime_ns, root_hash: root.to_vec() };
        self.engine.state.set_synced(&synced)?;
        Ok(())
    }
}
#[cfg(test)]
#[path = "../../tests/unit/synced/mod.rs"]
mod tests;
