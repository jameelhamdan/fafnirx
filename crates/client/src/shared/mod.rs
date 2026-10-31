//! Shared folders: a local folder kept in step with a shared folder of the master, in both directions.
//!
//! A cycle receives what changed on the master, then sends what changed here with the engine that uploads the collected
//! folders, so a file changed on both sides is noticed before the upload replaces the other side's version. Nothing is
//! overwritten silently: what becomes of this computer's version depends on the folder's `Conflict` policy (the master's is
//! kept and this one put aside; this one is sent; both are kept; or a person is asked).
//!
//! `State::synced` records each path as it was when last in step. A local file that still matches it is unchanged here, so
//! the master's change is safe to apply; one that does not was changed here too.

mod spellings;

use self::spellings::Spellings;
use crate::config::{Config, Conflict, Folder, Shared};
use crate::daemon::{self, SharedStatus};
use crate::net;
use crate::sync::guard;
use crate::sync::hash::{hash_file, stat};
use crate::sync::state::{Pending, State, Synced};
use crate::sync::throttle::Limiter;
use crate::sync::{Engine, Report, SyncError};
use anyhow::Result;
use fafnirx_common::{chunk_hash, clean_remote_path, fold_path, not_portable, pb, unix_now};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;

/// A file being received is written under this prefix next to its place and renamed when it is complete and checked.
pub const PART_PREFIX: &str = ".fafnirx-part-";

/// Where this computer's version of a file goes when the master's is kept (`Conflict::Theirs`): beside the files, never synced.
pub const CONFLICT_DIR: &str = ".fafnirx-conflicts";

/// Where in the state the point up to which the master's changes were seen is kept.
const CURSOR: &str = "shared_cursor";

/// A shared folder attached to a folder of this computer.
pub struct Attached {
    engine: Engine,
    shared: Shared,
    host: String,
    spellings: Spellings,
    /// Files answered at the start of this pull, with the master's content the answer was about: if the master lists that
    /// same change again (the cursor was held back), it is not a new conflict.
    answered: HashMap<String, Vec<u8>>,
}

/// What the master listed: the point to continue from next time, and the paths.
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

/// The state database of a shared folder: one per folder, beside the client's own.
pub fn state_file(state_dir: &Path, name: &str) -> PathBuf {
    let id = hex::encode(&chunk_hash(name.as_bytes())[..4]);
    state_dir.join(format!("shared-{id}.db"))
}

/// What a shared folder is doing, from its state database alone. It is all kept there (what is in step, what is held back,
/// how the last cycle ended), so `status` and `release-deletes` work with the client stopped.
pub fn status_of(shared: &Shared, state: &State) -> SharedStatus {
    let (files, bytes) = state.totals().unwrap_or((0, 0));
    let (last_cycle, last_error) = state.outcome().unwrap_or_default();
    SharedStatus {
        name: shared.name.clone(),
        path: shared.path.display().to_string(),
        files,
        bytes,
        last_cycle,
        last_error,
        held: state.held_files(),
        failing: state.pending_failures().unwrap_or(0) as usize,
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
    pub fn open(cfg: &Config, token: &str, state_dir: &Path, shared: &Shared, errors: Arc<Mutex<Vec<String>>>) -> Result<Self> {
        let client = net::shared_client(cfg, token, &crate::service::instance::client_id(state_dir), &shared.name)?;
        let mut state = State::open(&state_file(state_dir, &shared.name))?;
        state.adopt_folder(&shared.path.display().to_string())?;
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
        Ok(Self {
            engine,
            shared: shared.clone(),
            host,
            spellings: Spellings::new(&shared.path),
            answered: HashMap::new(),
        })
    }

    pub fn name(&self) -> &str {
        &self.shared.name
    }

    /// Whether this is still the folder attached in the config.
    pub fn is(&self, shared: &Shared) -> bool {
        self.shared.name == shared.name && self.shared.path == shared.path
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
            tracing::warn!("{}: cannot note how the cycle went: {e:#}", self.shared.name);
        }
        result
    }

    /// What this folder is doing, for the status file, the tray and the master.
    pub fn status(&self) -> SharedStatus {
        status_of(&self.shared, &self.engine.state)
    }

    async fn run_cycle(&mut self, cfg: &Config) -> Result<Report, SyncError> {
        self.engine.check_master().await?;
        let mut report = Report::default();
        self.pull(&mut report).await?;
        // A file waiting for a person is neither sent nor taken for deleted.
        self.engine.skip = self.engine.state.pending_paths()?;
        // The engine collects one folder with no name of its own: the files of a shared folder are at the top.
        let mut one = cfg.clone();
        one.folders = vec![Folder { path: self.shared.path.clone(), name: String::new() }];
        one.shared.clear();
        let pushed = self.engine.cycle(&one).await?;
        report.add(&pushed);
        Ok(report)
    }

    /// Receives what changed on the master since the last time: asks, works out what to do, receives, then deletes.
    async fn pull(&mut self, report: &mut Report) -> Result<(), SyncError> {
        // What was read of the folders is only good for one pull: the person changes them in between.
        self.spellings.clear();
        self.answered.clear();
        self.apply_answers(report).await?;
        let listing = self.list_changes().await?;
        let cursor = listing.cursor;
        let mut plan = self.plan(listing, report)?;
        let live = std::mem::take(&mut plan.live);
        let total = live.len();
        for (n, (path, target, change)) in live.into_iter().enumerate() {
            self.engine.set_activity("downloading", &path, n, total, 0);
            if let Err(e) = self.changed_on_master(&path, &target, &change, &mut plan.deletions, report).await {
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
            .list_changes(pb::ListChangesRequest { shared: self.shared.name.clone(), since })
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
                let target = local_path(&self.shared.path, &path);
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
        let target = local_path(&self.shared.path, path);
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
        self.engine.report_error(format!("{}/{path}: not received: {why}", self.shared.name));
    }

    /// A problem with one file is counted and reported and the next one goes on; a problem with the master ends the cycle.
    /// Returns true, to hold the cursor back.
    fn file_failed(&self, path: &str, e: SyncError, report: &mut Report) -> Result<bool, SyncError> {
        match e {
            SyncError::File(msg) => {
                report.failed += 1;
                self.engine.report_error(format!("{}/{path}: {msg} (will retry)", self.shared.name));
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
    async fn changed_on_master(&mut self, path: &str, target: &Path, change: &pb::Change, deletions: &mut Vec<(Synced, PathBuf)>, report: &mut Report) -> Result<(), SyncError> {
        let base = self.engine.state.synced(path)?;
        if base.as_ref().is_some_and(|b| b.root_hash == change.root_hash) || self.answered.get(path).is_some_and(|answered| *answered == change.root_hash) {
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
            if self.follow_case_rename(path, target, &old, change, report)? {
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
            make_parent(&self.shared.path, target)?;
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
    fn follow_case_rename(&mut self, path: &str, target: &Path, old: &Synced, change: &pb::Change, report: &mut Report) -> Result<bool, SyncError> {
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
                    .report_error(format!("{}: kept the {} files that were deleted on the server", self.shared.name, deletions.len()));
                return Ok(Applied::Kept);
            }
            // Held until a person decides: there is no time after which it is applied by itself.
            state.hold_pull(deletions.len(), unix_now())?;
            self.engine.report_error(format!(
                "{name}: {n} files were deleted on the server, a large part of the folder. They are not deleted here. \
                 `fafnirx release-deletes` deletes them here; `fafnirx shared keep {name}` keeps them",
                name = self.shared.name,
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
                    self.engine.report_error(format!("{}/{}: {e}", self.shared.name, base.path));
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

    /// The policy of this folder. `ask` needs the tray to show the question; a build without it cannot, and keeps the master's.
    fn policy(&self) -> Conflict {
        match self.shared.conflict {
            Conflict::Ask if !Conflict::ask_possible() => Conflict::Theirs,
            policy => policy,
        }
    }

    /// A file changed here and on the master (or changed here and deleted there): does what the policy of the folder says.
    /// `Settled::Receive`: this computer's version is out of the way, the master's may take its place. `Settled::Leave`: the
    /// file here stays as it is and nothing is received for it now.
    fn settle(&self, path: &str, target: &Path, master: Master<'_>, report: &mut Report) -> Result<Settled, SyncError> {
        let state = &self.engine.state;
        let settled = match self.policy() {
            Conflict::Theirs => {
                self.put_aside(path, target, report)?;
                Settled::Receive
            }
            Conflict::Both => {
                self.keep_as_conflict(path, target, report)?;
                Settled::Receive
            }
            Conflict::Mine => {
                // No longer in step, so the next send makes it the master's current version (the old one stays in its history).
                report.conflicts += 1;
                self.engine.report_error(format!(
                    "{}/{path}: changed here and on the server. This computer's version is kept and sent",
                    self.shared.name
                ));
                state.remove_synced(path)?;
                Settled::Leave
            }
            Conflict::Ask => {
                let (local_size, local_mtime_ns) = local_stat(target)?.unwrap_or_default();
                let mut pending = Pending { path: path.to_string(), local_size, local_mtime_ns, found_at: unix_now(), ..Default::default() };
                match master {
                    Master::Deleted => pending.deleted = true,
                    Master::Live(c) => {
                        (pending.root_hash, pending.size, pending.mtime, pending.mode, pending.from_web) = (c.root_hash.clone(), c.size, c.mtime, c.mode, c.from_web);
                    }
                }
                state.add_pending(&pending)?;
                self.engine.report_error(format!(
                    "{}/{path}: changed here and on the server. Nothing is changed until you choose which one to keep",
                    self.shared.name
                ));
                return Ok(Settled::Leave);
            }
        };
        // This computer's version is gone from the path (or is no longer the one in step): if receiving the master's fails, the
        // path must not look deleted here, or the send would delete it on the master.
        state.remove_synced(path)?;
        Ok(settled)
    }

    /// Moves this computer's version of a file into `CONFLICT_DIR` (never synced), so that the master's can take its place.
    fn put_aside(&self, path: &str, target: &Path, report: &mut Report) -> Result<(), SyncError> {
        let dir = self.shared.path.join(CONFLICT_DIR);
        let kept = conflict_name(&local_path(&dir, path), &self.host, unix_now());
        make_parent(&self.shared.path, &kept)?;
        std::fs::rename(target, &kept)?;
        report.conflicts += 1;
        self.engine.report_error(format!(
            "{}/{path}: changed here and on the server. The server's is kept; this computer's is in {CONFLICT_DIR}/{}",
            self.shared.name,
            kept.strip_prefix(&dir).unwrap_or(&kept).display()
        ));
        Ok(())
    }

    /// Applies the answers given for the files that waited, and settles those nobody answered in time as `both`.
    async fn apply_answers(&mut self, report: &mut Report) -> Result<(), SyncError> {
        let (now, hours) = (unix_now(), self.shared.ask_timeout_hours);
        let wait = (hours * 3600) as i64;
        for pending in self.engine.state.pendings()? {
            let choice = match pending.choice.as_deref() {
                Some(choice) => choice.to_string(),
                None if wait > 0 && now - pending.found_at >= wait => {
                    self.engine.report_error(format!(
                        "{}/{}: nobody chose in {} hours; both versions are kept",
                        self.shared.name, pending.path, hours
                    ));
                    "both".to_string()
                }
                None => continue,
            };
            match self.apply_choice(&pending, &choice, report).await {
                Ok(()) => self.engine.state.remove_pending(&pending.path)?,
                Err(e) => {
                    self.file_failed(&pending.path, e, report)?;
                }
            }
        }
        Ok(())
    }

    async fn apply_choice(&mut self, p: &Pending, choice: &str, report: &mut Report) -> Result<(), SyncError> {
        let target = local_path(&self.shared.path, &p.path);
        let here = local_stat(&target)?.is_some();
        let change = pb::Change {
            path: p.path.clone(),
            deleted: p.deleted,
            root_hash: p.root_hash.clone(),
            size: p.size,
            mtime: p.mtime,
            mode: p.mode,
            from_web: p.from_web,
        };
        // This computer's version stays and is sent. (If it is gone, the person deleted it meanwhile: nothing is left to keep.)
        if choice == "mine" && here {
            self.engine.state.remove_synced(&p.path)?;
            self.answered.insert(p.path.clone(), p.root_hash.clone());
            return Ok(());
        }
        if here {
            if choice == "both" {
                self.keep_as_conflict(&p.path, &target, report)?;
            } else {
                self.put_aside(&p.path, &target, report)?;
            }
        }
        self.engine.state.remove_synced(&p.path)?;
        if !p.deleted {
            self.receive_here(&p.path, &target, &change, report).await?;
        }
        Ok(())
    }

    /// Moves this computer's version of a file to a name of its own, so that the master's can take its place.
    fn keep_as_conflict(&self, path: &str, target: &Path, report: &mut Report) -> Result<(), SyncError> {
        let kept = conflict_name(target, &self.host, unix_now());
        std::fs::rename(target, &kept)?;
        report.conflicts += 1;
        self.engine.report_error(format!(
            "{}/{path}: changed here and on the server. Both are kept; the one from this computer is {}",
            self.shared.name,
            kept.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        ));
        Ok(())
    }

    /// Receives the current content of a file into place. It is written beside its place, checked against the hash the
    /// master gave, and only then renamed, so a half received file is never taken for the file.
    /// A local file edited while this runs (it can take minutes) is settled as a conflict, not replaced. Returns whether the
    /// master's version was put in place.
    async fn download(&mut self, path: &str, target: &Path, change: &pb::Change, report: &mut Report) -> Result<bool, SyncError> {
        make_parent(&self.shared.path, target)?;
        let before = local_stat(target)?;
        let tmp = target.with_file_name(format!("{PART_PREFIX}{}", hex::encode(&change.root_hash[..change.root_hash.len().min(6)])));
        if let Err(e) = self.receive_checked(path, &tmp, change).await {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        if let Some(now) = local_stat(target)? {
            if Some(now) != before {
                if let Settled::Leave = self.settle(path, target, Master::Live(change), report)? {
                    let _ = std::fs::remove_file(&tmp);
                    return Ok(false);
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // The mode of the other computer, but never one the owner cannot read and write.
            if change.mode & 0o777 != 0 {
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(change.mode & 0o777 | 0o600));
            }
        }
        give_to_owner_of(&self.shared.path, &tmp);
        tokio::fs::rename(&tmp, target).await.map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            let why = if e.kind() == std::io::ErrorKind::PermissionDenied && cfg!(target_os = "macos") && crate::service::disk_access::is_protected(target) {
                format!(" (macOS: {})", crate::service::disk_access::STEPS)
            } else {
                String::new()
            };
            SyncError::File(format!("cannot replace the file: {e}{why}"))
        })?;
        Ok(true)
    }

    /// Writes the file to `tmp` and checks it is what the master listed.
    async fn receive_checked(&mut self, path: &str, tmp: &Path, change: &pb::Change) -> Result<(), SyncError> {
        let mut stream = self
            .engine
            .client
            .download_shared(pb::DownloadSharedRequest { shared: self.shared.name.clone(), path: path.to_string() })
            .await?
            .into_inner();
        let mut file = tokio::fs::File::create(tmp).await?;
        while let Some(part) = stream.message().await? {
            file.write_all(&part.data).await?;
        }
        file.flush().await?;
        let got = hash_in_background(tmp).await?;
        if got.root[..] != change.root_hash[..] || got.size != change.size {
            return Err(SyncError::File("what was received is not what the server has (it changed meanwhile)".into()));
        }
        Ok(())
    }
}
/// The place of a path of the shared folder on this computer. The path is clean: relative, without `..`.
fn local_path(root: &Path, remote: &str) -> PathBuf {
    remote.split('/').fold(root.to_path_buf(), |p, part| p.join(part))
}

/// Makes the folders above `target`, inside the attached folder `root`, for whoever owns that folder.
fn make_parent(root: &Path, target: &Path) -> Result<(), SyncError> {
    let Some(parent) = target.parent() else { return Ok(()) };
    no_link_between(root, parent)?;
    let missing: Vec<&Path> = parent.ancestors().take_while(|p| !p.exists()).collect();
    std::fs::create_dir_all(parent)?;
    for dir in missing {
        give_to_owner_of(root, dir);
    }
    Ok(())
}

/// The service is root and the folder is the user's: a link in it (made by the user) must not lead what the service writes
/// somewhere else. Refuses when a folder from `root` down to `dir` is a link.
fn no_link_between(root: &Path, dir: &Path) -> Result<(), SyncError> {
    for ancestor in dir.ancestors().take_while(|p| *p != root && p.starts_with(root)) {
        if std::fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(SyncError::File(format!("{} is a link: files are not written through links", ancestor.display())));
        }
    }
    Ok(())
}

/// A file or folder the service makes in an attached folder is given to whoever owns that folder. The service is root, and
/// what root owns cannot be changed by the programs of the user (an editor, Obsidian) that works in the folder.
#[cfg(unix)]
fn give_to_owner_of(root: &Path, path: &Path) {
    use std::os::unix::fs::MetadataExt;
    if let Ok(owner) = std::fs::metadata(root) {
        // Not through a link: only what the service made is given away.
        let _ = std::os::unix::fs::lchown(path, Some(owner.uid()), Some(owner.gid()));
    }
}

#[cfg(not(unix))]
fn give_to_owner_of(_: &Path, _: &Path) {}

/// (size, modification time) of the file at a path; `None` when there is none. A folder in its place is a problem.
fn local_stat(target: &Path) -> Result<Option<(u64, i64)>, SyncError> {
    match std::fs::metadata(target) {
        Ok(m) if m.is_dir() => Err(SyncError::File("a folder of that name is in the way".into())),
        Ok(_) => Ok(Some(stat(target)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Whether the local file is the one that was in step: it has the size and time recorded then.
fn unchanged(base: &Synced, (size, mtime_ns): (u64, i64)) -> bool {
    base.size == size as i64 && base.mtime_ns == mtime_ns
}

async fn hash_in_background(path: &Path) -> Result<crate::sync::hash::Hashed, SyncError> {
    let p = path.to_path_buf();
    tokio::task::spawn_blocking(move || hash_file(&p))
        .await
        .map_err(|e| SyncError::File(format!("hashing failed: {e}")))?
        .map_err(|e| SyncError::File(format!("{e:#}")))
}

/// `name (conflict <computer> <date>).ext`, or with a number when that name is taken too.
fn conflict_name(target: &Path, host: &str, now: i64) -> PathBuf {
    let stem = target.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = target.extension().map(|e| e.to_string_lossy().into_owned());
    let host: String = host.chars().map(|c| if c.is_alphanumeric() || "-_.".contains(c) { c } else { '-' }).collect();
    let date = civil_date(now);
    for n in 0.. {
        let tag = if n == 0 { format!("conflict {host} {date}") } else { format!("conflict {host} {date} {n}") };
        let name = match &ext {
            Some(e) => format!("{stem} ({tag}).{e}"),
            None => format!("{stem} ({tag})"),
        };
        let candidate = target.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("some number is free")
}

/// `YYYY-MM-DD` (UTC) of a unix time.
fn civil_date(unix: i64) -> String {
    let z = unix.div_euclid(86400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn nothing_is_written_through_a_link_in_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (root, outside) = (dir.path().join("shared"), dir.path().join("outside"));
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        assert!(super::make_parent(&root, &root.join("real/new/a.txt")).is_ok());
        let e = super::make_parent(&root, &root.join("link/a.txt")).unwrap_err();
        assert!(format!("{e:?}").contains("is a link"), "{e:?}");
        assert!(super::make_parent(&root, &root.join("link/deep/a.txt")).is_err(), "nor below it");
        assert!(!outside.join("deep").exists(), "nothing was made outside");
    }

    use super::*;
    use crate::sync::tests::{serve, Calls};

    #[test]
    fn dates_are_civil_dates() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(951_782_400), "2000-02-29");
        assert_eq!(civil_date(1_793_059_200), "2026-10-27");
    }

    #[test]
    fn conflicts_get_a_name_of_their_own_next_to_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        let first = conflict_name(&file, "laptop", 0);
        assert_eq!(first.file_name().unwrap(), "notes (conflict laptop 1970-01-01).txt");
        std::fs::write(&first, b"x").unwrap();
        assert_eq!(conflict_name(&file, "laptop", 0).file_name().unwrap(), "notes (conflict laptop 1970-01-01 1).txt");
        let bare = dir.path().join("Makefile");
        assert_eq!(conflict_name(&bare, "my pc", 0).file_name().unwrap(), "Makefile (conflict my-pc 1970-01-01)");
    }

    #[test]
    fn a_read_only_share_stops_that_folder_not_the_client() {
        let read_only = SyncError::from(tonic::Status::failed_precondition("\"bob/family\" is shared with you to read only"));
        assert!(matches!(read_only, SyncError::Refused(_)), "a problem with the folder, recorded against no file");
        let in_use = SyncError::from(tonic::Status::permission_denied("device in use: another computer is connected"));
        assert!(matches!(in_use, SyncError::Offline(_)), "a problem with the device: nothing can sync");
    }

    /// Changes of a shared folder as the master lists them.
    mod master {
        use super::*;
        use fafnirx_common::{root_hash, CHUNK_SIZE};

        pub fn root_of(data: &[u8]) -> Vec<u8> {
            assert!(data.len() < CHUNK_SIZE);
            let chunks = if data.is_empty() { vec![] } else { vec![chunk_hash(data)] };
            root_hash(&chunks, data.len() as u64).to_vec()
        }

        pub fn live(path: &str, data: &[u8]) -> pb::Change {
            pb::Change {
                path: path.into(),
                deleted: false,
                root_hash: root_of(data),
                size: data.len() as u64,
                mtime: 1,
                mode: 0,
                from_web: false,
            }
        }

        pub fn gone(path: &str) -> pb::Change {
            pb::Change { path: path.into(), deleted: true, ..Default::default() }
        }

        pub fn gone_in_the_web_app(path: &str) -> pb::Change {
            pb::Change { from_web: true, ..gone(path) }
        }
    }

    struct Rig {
        attached: Attached,
        cfg: Config,
        held: Arc<Mutex<Calls>>,
        folder: tempfile::TempDir,
        _state: tempfile::TempDir,
    }

    impl Rig {
        async fn new() -> Self {
            let held = Arc::new(Mutex::new(Calls::default()));
            let url = serve(held.clone()).await;
            let (folder, state) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
            let shared = Shared {
                name: "alice/family".into(),
                path: folder.path().to_path_buf(),
                conflict: Conflict::Both,
                ask_timeout_hours: 24,
            };
            let mut cfg = Config::new(url);
            cfg.shared.push(shared.clone());
            let attached = Attached::open(&cfg, "token", state.path(), &shared, Arc::new(Mutex::new(Vec::new()))).unwrap();
            Self { attached, cfg, held, folder, _state: state }
        }

        /// Puts a file on the master and lists it as changed.
        fn remote_has(&self, path: &str, data: &[u8]) {
            let mut h = self.held.lock().unwrap();
            h.files.insert(path.into(), data.to_vec());
            h.changes = vec![master::live(path, data)];
        }

        fn remote_deleted(&self, path: &str) {
            let mut h = self.held.lock().unwrap();
            h.files.remove(path);
            h.changes = vec![master::gone(path)];
        }

        async fn cycle(&mut self) -> Report {
            self.attached.cycle(&self.cfg).await.expect("the cycle works")
        }

        fn local(&self, path: &str) -> Option<String> {
            std::fs::read_to_string(self.folder.path().join(path)).ok()
        }

        /// The names at the top of the folder, sorted.
        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(self.folder.path())
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }

        /// The names in the folder that are conflict copies.
        fn conflicts(&self) -> Vec<String> {
            self.names().into_iter().filter(|n| n.contains("(conflict ")).collect()
        }
    }

    #[tokio::test]
    async fn a_new_file_on_the_master_is_received_and_not_sent_back() {
        let mut rig = Rig::new().await;
        rig.remote_has("docs/a.txt", b"hello");
        let report = rig.cycle().await;
        assert_eq!((report.downloaded, report.conflicts, report.failed), (1, 0, 0));
        assert_eq!(rig.local("docs/a.txt").as_deref(), Some("hello"), "in its folder, below the root");
        assert!(rig.held.lock().unwrap().commits.is_empty(), "what came from the master is not sent back to it");
        // The same listing again changes nothing: the file is already what the master has.
        assert_eq!(rig.cycle().await.downloaded, 0);
        assert!(std::fs::read_dir(rig.folder.path().join("docs")).unwrap().count() == 1, "no part file is left");
    }

    #[tokio::test]
    async fn a_file_changed_on_both_sides_keeps_both() {
        let mut rig = Rig::new().await;
        std::fs::write(rig.folder.path().join("b.txt"), "mine").unwrap();
        rig.remote_has("b.txt", b"theirs");
        let report = rig.cycle().await;
        assert_eq!((report.downloaded, report.conflicts), (1, 1));
        assert_eq!(rig.local("b.txt").as_deref(), Some("theirs"), "the master's version has the name");
        let conflicts = rig.conflicts();
        assert_eq!(conflicts.len(), 1, "{conflicts:?}");
        assert!(conflicts[0].starts_with("b (conflict "), "{}", conflicts[0]);
        assert_eq!(rig.local(&conflicts[0]).as_deref(), Some("mine"), "ours is kept under its own name");
        assert!(
            rig.held.lock().unwrap().commits.iter().any(|(path, _)| *path == conflicts[0]),
            "and sent, like any new file"
        );
    }

    impl Rig {
        fn with_policy(mut self, conflict: Conflict) -> Self {
            self.attached.shared.conflict = conflict;
            self
        }

        /// The versions of this computer that were put aside, by name.
        fn aside(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(self.folder.path().join(CONFLICT_DIR))
                .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                .unwrap_or_default();
            names.sort();
            names
        }

        fn sent(&self, path: &str) -> bool {
            self.held.lock().unwrap().commits.iter().any(|(p, _)| p == path)
        }

        /// The master lists nothing new (a cursor that went past the last change).
        fn nothing_new(&self) {
            self.held.lock().unwrap().changes.clear();
        }
    }

    #[tokio::test]
    async fn theirs_keeps_the_masters_version_and_puts_ours_aside_unsent() {
        let mut rig = Rig::new().await.with_policy(Conflict::Theirs);
        std::fs::write(rig.folder.path().join("b.txt"), "mine").unwrap();
        rig.remote_has("b.txt", b"theirs");
        let report = rig.cycle().await;
        assert_eq!((report.downloaded, report.conflicts), (1, 1));
        assert_eq!(rig.local("b.txt").as_deref(), Some("theirs"));
        assert!(rig.conflicts().is_empty(), "no conflict copy among the files");
        let aside = rig.aside();
        assert_eq!(aside.len(), 1, "{aside:?}");
        assert_eq!(
            std::fs::read_to_string(rig.folder.path().join(CONFLICT_DIR).join(&aside[0])).unwrap(),
            "mine",
            "ours is not lost"
        );
        rig.nothing_new();
        rig.cycle().await;
        assert!(rig.held.lock().unwrap().commits.is_empty(), "what was put aside is never sent");
    }

    #[tokio::test]
    async fn mine_keeps_ours_and_sends_it() {
        let mut rig = Rig::new().await.with_policy(Conflict::Mine);
        std::fs::write(rig.folder.path().join("b.txt"), "mine").unwrap();
        rig.remote_has("b.txt", b"theirs");
        let report = rig.cycle().await;
        assert_eq!((report.downloaded, report.conflicts), (0, 1));
        assert_eq!(rig.local("b.txt").as_deref(), Some("mine"));
        assert!(rig.sent("b.txt"), "and it becomes the master's version");
        assert!(rig.names() == ["b.txt"], "nothing is put aside: {:?}", rig.names());
    }

    #[tokio::test]
    async fn a_file_edited_here_and_deleted_there_follows_the_policy() {
        let mut rig = Rig::new().await.with_policy(Conflict::Theirs);
        rig.remote_has("d.txt", b"one");
        rig.cycle().await;
        std::fs::write(rig.folder.path().join("d.txt"), "edited here").unwrap();
        rig.remote_deleted("d.txt");
        rig.cycle().await;
        assert!(rig.local("d.txt").is_none(), "the deletion applies");
        let aside = rig.aside();
        assert_eq!(aside.len(), 1, "{aside:?}");
        assert!(!rig.sent("d.txt") && rig.conflicts().is_empty());

        let mut rig = Rig::new().await.with_policy(Conflict::Mine);
        rig.remote_has("d.txt", b"one");
        rig.cycle().await;
        std::fs::write(rig.folder.path().join("d.txt"), "edited here").unwrap();
        rig.remote_deleted("d.txt");
        rig.cycle().await;
        assert_eq!(rig.local("d.txt").as_deref(), Some("edited here"));
        assert!(rig.sent("d.txt"), "and it is sent again");
    }

    #[cfg(any(windows, target_os = "macos", feature = "tray"))]
    mod ask {
        use super::*;

        async fn waiting() -> Rig {
            let mut rig = Rig::new().await.with_policy(Conflict::Ask);
            std::fs::write(rig.folder.path().join("b.txt"), "mine").unwrap();
            rig.remote_has("b.txt", b"theirs");
            rig.cycle().await;
            rig.nothing_new();
            rig
        }

        #[tokio::test]
        async fn ask_changes_nothing_on_either_side_until_answered() {
            let mut rig = waiting().await;
            assert_eq!(rig.local("b.txt").as_deref(), Some("mine"));
            let pending = rig.attached.engine.state.pendings().unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!((pending[0].path.as_str(), pending[0].local_size), ("b.txt", 4));
            assert_eq!(rig.attached.status().conflicts.len(), 1, "the tray is told");
            rig.cycle().await;
            assert!(rig.held.lock().unwrap().commits.is_empty(), "it is not sent while it waits");
            assert_eq!(rig.local("b.txt").as_deref(), Some("mine"));
        }

        #[tokio::test]
        async fn the_answer_is_applied_at_the_next_cycle() {
            for (choice, name, ours_aside, sent) in [("theirs", "theirs", true, false), ("mine", "mine", false, true), ("both", "theirs", false, false)] {
                let mut rig = waiting().await;
                assert_eq!(rig.attached.engine.state.answer_pending(Some("b.txt"), choice).unwrap(), 1);
                rig.cycle().await;
                assert_eq!(rig.local("b.txt").as_deref(), Some(name), "{choice}");
                assert_eq!(rig.aside().len(), usize::from(ours_aside), "{choice}");
                assert_eq!(rig.conflicts().len(), usize::from(choice == "both"), "{choice}");
                assert_eq!(rig.sent("b.txt"), sent, "{choice}");
                assert!(rig.attached.engine.state.pendings().unwrap().is_empty(), "{choice}: done");
            }
        }

        #[tokio::test]
        async fn a_change_listed_again_after_the_answer_is_not_asked_about_again() {
            let mut rig = waiting().await;
            // The cursor was held back, so the master lists the same change once more.
            rig.remote_has("b.txt", b"theirs");
            rig.attached.engine.state.answer_pending(Some("b.txt"), "mine").unwrap();
            rig.cycle().await;
            assert!(rig.attached.engine.state.pendings().unwrap().is_empty(), "not asked again");
            assert_eq!(rig.local("b.txt").as_deref(), Some("mine"));
            assert!(rig.sent("b.txt"));
        }

        #[tokio::test]
        async fn a_newer_version_of_the_master_needs_a_new_answer() {
            let rig = waiting().await;
            let state = &rig.attached.engine.state;
            state.answer_pending(Some("b.txt"), "theirs").unwrap();
            let mut newer = state.pendings().unwrap().remove(0);
            newer.root_hash = vec![9; 32];
            state.add_pending(&newer).unwrap();
            assert_eq!(state.pendings().unwrap()[0].choice, None, "the answer was about another version");
        }

        #[tokio::test]
        async fn nobody_answering_keeps_both_after_the_timeout() {
            let mut rig = waiting().await;
            rig.attached.shared.ask_timeout_hours = 1;
            rig.cycle().await;
            assert_eq!(rig.attached.engine.state.pendings().unwrap().len(), 1, "not yet");
            rig.attached.engine.state.backdate_pending(unix_now() - 2 * 3600).unwrap();
            rig.cycle().await;
            assert_eq!(rig.local("b.txt").as_deref(), Some("theirs"));
            assert_eq!(rig.conflicts().len(), 1, "ours is kept beside it");
            assert!(rig.attached.engine.state.pendings().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn a_file_deleted_on_the_master_goes_unless_it_was_changed_here() {
        let mut rig = Rig::new().await;
        rig.remote_has("c.txt", b"one");
        rig.cycle().await;
        rig.remote_deleted("c.txt");
        let report = rig.cycle().await;
        assert_eq!(report.deleted, 1);
        assert!(rig.local("c.txt").is_none(), "unchanged here: it goes too");

        rig.remote_has("d.txt", b"one");
        rig.cycle().await;
        std::fs::write(rig.folder.path().join("d.txt"), "edited here after it came").unwrap();
        rig.remote_deleted("d.txt");
        let report = rig.cycle().await;
        assert_eq!((report.deleted, report.conflicts), (0, 1));
        assert!(rig.local("d.txt").is_none());
        let kept = rig.conflicts();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(rig.local(&kept[0]).as_deref(), Some("edited here after it came"), "the edit is not lost");
    }

    #[tokio::test]
    async fn a_rename_on_the_master_moves_the_file_instead_of_receiving_it_again() {
        let mut rig = Rig::new().await;
        rig.remote_has("old.txt", b"same content");
        rig.cycle().await;
        {
            let mut h = rig.held.lock().unwrap();
            h.files.remove("old.txt");
            h.files.insert("new.txt".into(), b"same content".to_vec());
            h.changes = vec![master::gone("old.txt"), master::live("new.txt", b"same content")];
        }
        let report = rig.cycle().await;
        assert_eq!(rig.local("new.txt").as_deref(), Some("same content"));
        assert!(rig.local("old.txt").is_none());
        assert_eq!((report.downloaded, report.conflicts, report.failed), (1, 0, 0));
        assert!(rig.held.lock().unwrap().deleted.is_empty(), "nothing is deleted on the master by moving");
    }

    #[tokio::test]
    async fn a_rename_that_only_changes_the_case_is_a_move_on_every_kind_of_computer() {
        let mut rig = Rig::new().await;
        rig.remote_has("Readme.md", b"same content");
        rig.cycle().await;
        {
            let mut h = rig.held.lock().unwrap();
            h.files.remove("Readme.md");
            h.files.insert("readme.md".into(), b"same content".to_vec());
            h.changes = vec![master::gone("Readme.md"), master::live("readme.md", b"same content")];
        }
        let report = rig.cycle().await;
        assert_eq!(rig.names(), ["readme.md"], "spelled as the master has it, not left under the old spelling");
        assert_eq!((report.downloaded, report.conflicts, report.failed), (1, 0, 0));
        assert!(rig.held.lock().unwrap().deleted.is_empty());
    }

    #[tokio::test]
    async fn a_folder_renamed_to_another_case_is_renamed_here_too_with_everything_in_it() {
        let mut rig = Rig::new().await;
        rig.remote_has("Trip/a.txt", b"first");
        rig.cycle().await;
        rig.remote_has("Trip/b.txt", b"second");
        rig.cycle().await;
        {
            let mut h = rig.held.lock().unwrap();
            h.files.clear();
            h.files.insert("trip/a.txt".into(), b"first".to_vec());
            h.files.insert("trip/b.txt".into(), b"second".to_vec());
            h.changes = vec![
                master::gone("Trip/a.txt"),
                master::gone("Trip/b.txt"),
                master::live("trip/a.txt", b"first"),
                master::live("trip/b.txt", b"second"),
            ];
        }
        let report = rig.cycle().await;
        assert_eq!(rig.names(), ["trip"], "one folder, spelled as the master has it");
        assert_eq!(rig.local("trip/a.txt").as_deref(), Some("first"));
        assert_eq!(
            rig.local("trip/b.txt").as_deref(),
            Some("second"),
            "the second file is not deleted as the old spelling of itself"
        );
        assert_eq!((report.conflicts, report.failed, report.deleted), (0, 0, 0));
        let held = rig.held.lock().unwrap();
        assert!(held.commits.is_empty() && held.deleted.is_empty(), "and nothing goes back under the old spelling");
    }

    #[tokio::test]
    async fn a_file_deleted_in_a_folder_renamed_to_another_case_is_deleted_there() {
        let mut rig = Rig::new().await;
        rig.remote_has("Trip/a.txt", b"first");
        rig.cycle().await;
        rig.remote_has("Trip/b.txt", b"second");
        rig.cycle().await;
        {
            let mut h = rig.held.lock().unwrap();
            h.files.clear();
            h.files.insert("trip/a.txt".into(), b"first".to_vec());
            h.changes = vec![master::gone("Trip/a.txt"), master::gone("Trip/b.txt"), master::live("trip/a.txt", b"first")];
        }
        let report = rig.cycle().await;
        assert_eq!(rig.names(), ["trip"]);
        assert_eq!(rig.local("trip/a.txt").as_deref(), Some("first"));
        assert_eq!(rig.local("trip/b.txt"), None, "deleted on the master, where the folder is now spelled otherwise");
        assert_eq!(report.deleted, 1);
        assert!(rig.held.lock().unwrap().commits.is_empty(), "and not sent back");
    }

    #[tokio::test]
    async fn a_name_that_differs_only_by_case_from_one_here_is_not_received() {
        let mut rig = Rig::new().await;
        std::fs::write(rig.folder.path().join("Notes.txt"), "mine").unwrap();
        rig.remote_has("notes.txt", b"theirs");
        let report = rig.cycle().await;
        assert_eq!((report.downloaded, report.conflicts), (0, 0));
        assert_eq!(rig.names(), ["Notes.txt"], "one name, not two, whatever the file system");
        assert_eq!(rig.local("Notes.txt").as_deref(), Some("mine"));
    }

    #[tokio::test]
    async fn names_windows_cannot_keep_are_not_received_anywhere() {
        let mut rig = Rig::new().await;
        {
            let mut h = rig.held.lock().unwrap();
            h.files.insert("a:b.txt".into(), b"x".to_vec());
            h.files.insert("ok.txt".into(), b"y".to_vec());
            h.changes = vec![master::live("a:b.txt", b"x"), master::live("ok.txt", b"y")];
        }
        let report = rig.cycle().await;
        assert_eq!((report.downloaded, report.failed), (1, 0), "skipped, not failed: nothing to retry");
        assert_eq!(rig.names(), ["ok.txt"]);
    }

    #[test]
    fn another_spelling_is_found_by_reading_the_folders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Docs")).unwrap();
        std::fs::write(dir.path().join("Docs").join("A.txt"), "x").unwrap();
        let root = dir.path();
        let mut seen = Spellings::new(root);
        assert_eq!(seen.clash_with("docs/a.txt"), Some(root.join("Docs").join("A.txt")));
        assert_eq!(seen.clash_with("Docs/a.txt"), Some(root.join("Docs").join("A.txt")), "the file alone");
        assert_eq!(seen.clash_with("docs/new.txt"), Some(root.join("Docs").join("new.txt")), "a folder above it");
        assert_eq!(seen.clash_with("Docs/A.txt"), None, "the same spelling is the same file");
        assert_eq!(seen.clash_with("Docs/new.txt"), None);
        assert_eq!(seen.clash_with("other/a.txt"), None);
    }

    #[test]
    fn what_was_read_follows_the_files_that_are_received_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut seen = Spellings::new(root);
        assert_eq!(seen.clash_with("a/b.txt"), None, "nothing is there; the folders are now read");
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("a").join("b.txt"), "x").unwrap();
        seen.added(&root.join("a").join("b.txt"));
        assert_eq!(seen.clash_with("A/b.txt"), Some(root.join("a").join("b.txt")), "a folder made since is known");
        assert!(seen.exists_spelled("a/b.txt"));
        seen.removed(&root.join("a").join("b.txt"));
        assert!(!seen.exists_spelled("a/b.txt"), "and a file deleted since is gone");
    }

    #[tokio::test]
    async fn what_was_deleted_while_this_computer_was_away_goes_when_the_master_lists_everything() {
        let mut rig = Rig::new().await;
        for (path, text) in [("keep.txt", "1"), ("gone.txt", "2"), ("edited.txt", "3")] {
            rig.remote_has(path, text.as_bytes());
            rig.cycle().await;
        }
        std::fs::write(rig.folder.path().join("edited.txt"), "changed here meanwhile").unwrap();
        {
            // The master no longer remembers the deletions: it lists what is live, and the other two are not in it.
            let mut h = rig.held.lock().unwrap();
            h.complete = true;
            h.changes = vec![master::live("keep.txt", b"1")];
        }
        let report = rig.cycle().await;
        assert_eq!(report.deleted, 1, "the untouched file that is not listed any more is deleted");
        assert_eq!(rig.local("gone.txt"), None);
        assert_eq!(rig.local("keep.txt").as_deref(), Some("1"), "a listed one stays");
        assert_eq!(report.conflicts, 1, "and the one edited here is kept, under another name");
        let kept = rig.conflicts();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(rig.local(&kept[0]).as_deref(), Some("changed here meanwhile"));
        assert_eq!(rig.local("edited.txt"), None);
    }

    #[tokio::test]
    async fn a_listing_of_changes_alone_never_deletes_what_it_does_not_mention() {
        let mut rig = Rig::new().await;
        rig.remote_has("a.txt", b"1");
        rig.cycle().await;
        rig.remote_has("b.txt", b"2");
        let report = rig.cycle().await;
        assert_eq!((report.deleted, report.downloaded), (0, 1));
        assert_eq!(rig.local("a.txt").as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn the_state_of_a_folder_says_how_it_is_doing() {
        let mut rig = Rig::new().await;
        let before = rig.attached.status();
        assert_eq!((before.files, before.last_cycle, before.last_error.as_str()), (0, 0, ""));
        assert_eq!(before.summary(1_000), "waiting for the first sync");

        rig.remote_has("a.txt", b"hello");
        rig.cycle().await;
        let after = rig.attached.status();
        assert_eq!(
            (after.name.as_str(), after.files, after.bytes, after.held, after.failing),
            ("alice/family", 1, 5, 0, 0)
        );
        assert!(after.last_cycle > 0 && after.last_error.is_empty());
        assert!(
            after.summary(after.last_cycle + 90).starts_with("in step, 1 files, synced 1m ago"),
            "{}",
            after.summary(after.last_cycle + 90)
        );

        // A problem of the cycle is told, in the state and in its line.
        rig.remote_has("b.txt", b"x");
        rig.held.lock().unwrap().files.remove("b.txt"); // listed, but the master cannot give it
        rig.cycle().await;
        let failed = rig.attached.status();
        assert!(failed.last_error.contains("b.txt"), "{}", failed.last_error);
        assert!(failed.summary(failed.last_cycle).contains("needs attention") || failed.summary(failed.last_cycle).contains("last problem"));
    }

    /// A folder with twelve files, in step with the master.
    async fn synced_rig() -> (Rig, Vec<String>) {
        let mut rig = Rig::new().await;
        let names: Vec<String> = (0..12).map(|i| format!("f{i}.txt")).collect();
        for n in &names {
            rig.remote_has(n, b"x");
            rig.cycle().await;
        }
        (rig, names)
    }

    /// The same, with all of them deleted by another computer, so that the deletions are held back.
    async fn held_rig() -> (Rig, Vec<String>) {
        let (mut rig, names) = synced_rig().await;
        rig.held.lock().unwrap().changes = names.iter().map(|n| master::gone(n)).collect();
        rig.cycle().await;
        (rig, names)
    }

    #[tokio::test]
    async fn held_deletions_are_kept_in_the_state_so_they_show_with_the_client_stopped() {
        let (rig, _) = held_rig().await;
        assert_eq!(rig.attached.engine.state.pull_hold().unwrap().map(|(n, _)| n), Some(12));
        let shared = Shared {
            name: "alice/family".into(),
            path: rig.folder.path().to_path_buf(),
            conflict: Conflict::Both,
            ask_timeout_hours: 24,
        };
        // Another process (`fafnirx status`) opens the same database and sees the same.
        let reopened = State::open(&state_file(rig._state.path(), "alice/family")).unwrap();
        let seen = status_of(&shared, &reopened);
        assert_eq!((seen.held, seen.files), (12, 12));
        assert!(seen.last_cycle > 0, "and how the last cycle ended");
        assert_eq!(seen, rig.attached.status());
        assert!(
            seen.last_error.contains("keep alice/family"),
            "the last problem says what to do: {}",
            seen.last_error
        );
        // Held means held: no later cycle applies them by itself.
        let mut rig = rig;
        for _ in 0..3 {
            rig.cycle().await;
        }
        assert_eq!(rig.attached.engine.state.pull_hold().unwrap().map(|(n, _)| n), Some(12));
        assert!(rig.local("f0.txt").is_some());
    }

    #[tokio::test]
    async fn deletions_made_in_the_web_app_are_applied_at_once_however_many() {
        let (mut rig, names) = synced_rig().await;
        rig.held.lock().unwrap().changes = names.iter().map(|n| master::gone_in_the_web_app(n)).collect();
        let report = rig.cycle().await;
        assert_eq!(report.deleted, 12, "deliberate, and in the Trash to restore");
        assert!(names.iter().all(|n| rig.local(n).is_none()));
        assert!(rig.attached.engine.state.pull_hold().unwrap().is_none());
    }

    #[tokio::test]
    async fn only_what_other_computers_deleted_is_held() {
        let (mut rig, names) = synced_rig().await;
        let mut changes: Vec<_> = names[..2].iter().map(|n| master::gone_in_the_web_app(n)).collect();
        changes.extend(names[2..].iter().map(|n| master::gone(n)));
        rig.held.lock().unwrap().changes = changes;
        let report = rig.cycle().await;
        assert_eq!(report.deleted, 2, "the web app's two go, the other computer's ten wait");
        assert_eq!(rig.attached.engine.state.pull_hold().unwrap().map(|(n, _)| n), Some(10));
    }

    #[tokio::test]
    async fn held_deletions_can_be_released() {
        let (mut rig, names) = held_rig().await;
        rig.attached.engine.state.request_release().unwrap();
        let report = rig.cycle().await;
        assert_eq!(report.deleted, 12);
        assert!(names.iter().all(|n| rig.local(n).is_none()));
        assert!(rig.attached.engine.state.pull_hold().unwrap().is_none(), "nothing is held any more");
        assert_eq!(rig.attached.status().held, 0);
    }

    #[tokio::test]
    async fn held_deletions_can_be_undone_by_keeping_the_files() {
        let (mut rig, names) = held_rig().await;
        rig.attached.engine.state.request_keep().unwrap();
        let report = rig.cycle().await;
        assert_eq!(report.deleted, 0);
        assert!(names.iter().all(|n| rig.local(n).is_some()), "the files stay");
        assert!(rig.attached.engine.state.pull_hold().unwrap().is_none());
        let sent = rig.held.lock().unwrap().commits.clone();
        assert_eq!(sent.len(), 12, "and go up again, since they are no longer in step: {sent:?}");
        // The master's deletions were let go of, not held for ever: the next listing is quiet.
        assert_eq!(rig.attached.status().held, 0);
    }

    #[tokio::test]
    async fn a_request_to_keep_that_was_not_needed_does_not_wait_for_the_next_hold() {
        let (mut rig, _) = held_rig().await;
        rig.attached.engine.state.request_keep().unwrap();
        // Before it is used the deletions shrink to a few, which are not held: the request goes with them.
        rig.held.lock().unwrap().changes = vec![master::gone("f0.txt"), master::gone("f1.txt")];
        let report = rig.cycle().await;
        assert_eq!(report.deleted, 2);
        assert!(!rig.attached.engine.state.take_keep().unwrap(), "used up, not left behind");
        assert!(rig.attached.engine.state.pull_hold().unwrap().is_none());
    }
}
