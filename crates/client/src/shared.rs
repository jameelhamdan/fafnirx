//! Shared folders: a local folder kept in step with a shared folder of the master, in both directions.
//!
//! A cycle receives what changed on the master, then sends what changed here with the engine that uploads the collected
//! folders, so a file changed on both sides is noticed before the upload replaces the other side's version. Nothing is
//! overwritten silently: this computer's version is kept as `name (conflict <computer> <date>).ext`.
//!
//! `State::synced` records each path as it was when last in step. A local file that still matches it is unchanged here, so
//! the master's change is safe to apply; one that does not was changed here too.

use crate::config::{Config, Folder, Shared};
use crate::daemon::{self, SharedStatus};
use crate::guard;
use crate::hash::{hash_file, stat};
use crate::net;
use crate::state::{State, Synced};
use crate::sync::{Engine, Report, SyncError};
use crate::throttle::Limiter;
use anyhow::Result;
use fafnirx_common::{chunk_hash, clean_remote_path, fold_path, not_portable, pb, unix_now};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;

/// A file being received is written under this prefix next to its place and renamed when it is complete and checked.
pub const PART_PREFIX: &str = ".fafnirx-part-";

/// Where in the state the point up to which the master's changes were seen is kept.
const CURSOR: &str = "shared_cursor";

/// A shared folder attached to a folder of this computer.
pub struct Attached {
    engine: Engine,
    shared: Shared,
    host: String,
    spellings: Spellings,
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
    }
}

impl Attached {
    pub fn open(
        cfg: &Config,
        token: &str,
        state_dir: &Path,
        shared: &Shared,
        errors: Arc<Mutex<Vec<String>>>,
    ) -> Result<Self> {
        let client = net::shared_client(cfg, token, &crate::instance::client_id(state_dir), &shared.name)?;
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
        Ok(Self { engine, shared: shared.clone(), host, spellings: Spellings::new(&shared.path) })
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
        let cursor = cursor
            .ok_or_else(|| SyncError::Offline("the master listed no changes and no point to continue from".into()))?;
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
            if let Some(why) = not_portable(&path) {
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
    fn queue_deletion(
        &self,
        path: &str,
        from_web: bool,
        plan: &mut Plan,
        report: &mut Report,
    ) -> Result<(), SyncError> {
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
                Ok(None)
            }
            Some(st) if unchanged(&base, st) => Ok(Some(base)),
            Some(_) => {
                // Changed here, deleted there: our version is kept, under another name so that it can be sent again.
                self.keep_as_conflict(path, target, report)?;
                self.engine.state.remove_synced(path)?;
                Ok(None)
            }
        }
    }

    /// A path has other content on the master than it had when it was last in step.
    async fn changed_on_master(
        &mut self,
        path: &str,
        target: &Path,
        change: &pb::Change,
        deletions: &mut Vec<(Synced, PathBuf)>,
        report: &mut Report,
    ) -> Result<(), SyncError> {
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
                    return Ok(());
                }
                self.keep_as_conflict(path, target, report)?;
            }
        }
        // The content of a file that is deleted on the master is what a rename moved: move it here too.
        if let Some(i) = deletions.iter().position(|(b, _)| b.root_hash == change.root_hash) {
            let (old, old_target) = deletions.remove(i);
            make_parent(target)?;
            std::fs::rename(&old_target, target)?;
            self.spellings.removed(&old_target);
            self.spellings.added(target);
            self.engine.state.remove_synced(&old.path)?;
            self.record(path, target, &change.root_hash)?;
            report.downloaded += 1;
            return Ok(());
        }
        self.download(path, target, change, report).await?;
        self.spellings.added(target);
        self.record(path, target, &change.root_hash)?;
        report.downloaded += 1;
        Ok(())
    }

    /// The master renamed `old` to `path`, which differs from it only in case. The folders above are renamed here the same,
    /// whole: a file system that ignores case would keep the old spelling of one and send that back. Returns true when that
    /// was all (same content, the file is moved); false when the old file is gone and the new content is still to be received.
    fn follow_case_rename(
        &mut self,
        path: &str,
        target: &Path,
        old: &Synced,
        change: &pb::Change,
        report: &mut Report,
    ) -> Result<bool, SyncError> {
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
    fn apply_deletions(
        &mut self,
        deletions: Vec<(Synced, PathBuf)>,
        from_web: &HashSet<String>,
        folded: &HashSet<String>,
        report: &mut Report,
    ) -> Result<Applied, SyncError> {
        let synced = self.engine.state.totals()?.0;
        // What was deleted in the web app is applied whatever its number; the guard is for what other computers delete.
        let (web, deletions): (Vec<_>, Vec<_>) =
            deletions.into_iter().partition(|(base, _)| from_web.contains(&base.path));
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
                self.engine.report_error(format!(
                    "{}: kept the {} files that were deleted on the server",
                    self.shared.name,
                    deletions.len()
                ));
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
    fn delete_here(
        &mut self,
        deletions: Vec<(Synced, PathBuf)>,
        folded: &HashSet<String>,
        report: &mut Report,
    ) -> Result<(), SyncError> {
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
            // Edited since it was listed (downloads can take minutes): kept, as for a file changed on both sides.
            if local_stat(&here)?.is_some_and(|st| !unchanged(&base, st)) {
                self.keep_as_conflict(&base.path, &here, report)?;
                self.engine.state.remove_synced(&base.path)?;
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
    /// A local file edited while this runs (it can take minutes) is kept as a conflict copy, not replaced.
    async fn download(
        &mut self,
        path: &str,
        target: &Path,
        change: &pb::Change,
        report: &mut Report,
    ) -> Result<(), SyncError> {
        make_parent(target)?;
        let before = local_stat(target)?;
        let tmp = target.with_file_name(format!(
            "{PART_PREFIX}{}",
            hex::encode(&change.root_hash[..change.root_hash.len().min(6)])
        ));
        if let Err(e) = self.receive_checked(path, &tmp, change).await {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        if let Some(now) = local_stat(target)? {
            if Some(now) != before {
                self.keep_as_conflict(path, target, report)?;
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if change.mode & 0o777 != 0 {
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(change.mode & 0o777));
            }
        }
        tokio::fs::rename(&tmp, target).await.map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            SyncError::File(format!("cannot replace the file: {e}"))
        })
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

/// What a folder holds, by name and by folded name, so that a lookup does not read the folder again.
#[derive(Default)]
struct DirNames {
    names: HashSet<String>,
    folded: HashMap<String, Vec<String>>,
}

impl DirNames {
    fn read(dir: &Path) -> Self {
        let mut dir_names = Self::default();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            dir_names.add(entry.file_name().to_string_lossy().into_owned());
        }
        dir_names
    }

    fn add(&mut self, name: String) {
        if self.names.insert(name.clone()) {
            self.folded.entry(fold_path(&name)).or_default().push(name);
        }
    }

    fn remove(&mut self, name: &str) {
        self.names.remove(name);
        if let Some(same) = self.folded.get_mut(&fold_path(name)) {
            same.retain(|n| n != name);
        }
    }

    /// The entry called `name`, spelled as it is here: the name itself if it exists, else one that differs only in case.
    fn find(&self, name: &str) -> Option<String> {
        if self.names.contains(name) {
            return Some(name.to_string());
        }
        self.folded.get(&fold_path(name)).and_then(|same| same.first().cloned())
    }
}

/// How the names of a shared folder are spelled on this computer. Every kind of computer treats names that differ only in
/// case (or Unicode form) as one, and a file system that folds case cannot be asked, so the folders are read. Each is read once
/// and then kept up to date as files are received, moved and deleted: asking the file system per path would read a folder of
/// ten thousand files ten thousand times.
struct Spellings {
    root: PathBuf,
    dirs: HashMap<PathBuf, DirNames>,
}

impl Spellings {
    fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf(), dirs: HashMap::new() }
    }

    /// Forgets what was read; for the cases (renames of folders, conflict copies) not worth tracking.
    fn clear(&mut self) {
        self.dirs.clear();
    }

    fn find(&mut self, dir: &Path, name: &str) -> Option<String> {
        self.dirs.entry(dir.to_path_buf()).or_insert_with(|| DirNames::read(dir)).find(name)
    }

    /// `path` was made (and the folders above it, as far as they are known).
    fn added(&mut self, path: &Path) {
        let Ok(relative) = path.strip_prefix(&self.root) else { return };
        let mut dir = self.root.clone();
        for part in relative.components() {
            let name = part.as_os_str().to_string_lossy().into_owned();
            if let Some(listing) = self.dirs.get_mut(&dir) {
                listing.add(name.clone());
            }
            dir.push(name);
        }
    }

    /// `path` was removed or moved away.
    fn removed(&mut self, path: &Path) {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return };
        if let Some(listing) = self.dirs.get_mut(dir) {
            listing.remove(&name.to_string_lossy());
        }
    }

    /// An existing path of the folder that is the same name as `remote` but spelled differently: another case, or another
    /// Unicode form, in the file or in a folder above it. The path is returned as it is spelled here.
    fn clash_with(&mut self, remote: &str) -> Option<PathBuf> {
        let parts: Vec<&str> = remote.split('/').collect();
        let mut dir = self.root.clone();
        let mut differs = false;
        for (i, part) in parts.iter().enumerate() {
            let Some(found) = self.find(&dir, part) else {
                // Nothing of this name is here, so nothing below it is either.
                dir.extend(&parts[i..]);
                return differs.then_some(dir);
            };
            differs |= found != *part;
            dir.push(found);
        }
        differs.then_some(dir)
    }

    /// Gives the folders above `remote` the spelling `remote` has, renaming a folder here that has the same name in another case.
    fn respell_folders(&mut self, remote: &str) -> std::io::Result<()> {
        let mut dir = self.root.clone();
        let parts: Vec<&str> = remote.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            if let Some(found) = self.find(&dir, part).filter(|found| found != part) {
                std::fs::rename(dir.join(&found), dir.join(part))?;
                self.clear();
            }
            dir.push(part);
        }
        Ok(())
    }

    /// Whether `remote` exists here spelled exactly so, in the file and in every folder above it.
    fn exists_spelled(&mut self, remote: &str) -> bool {
        let mut dir = self.root.clone();
        for part in remote.split('/') {
            if self.find(&dir, part).as_deref() != Some(part) {
                return false;
            }
            dir.push(part);
        }
        true
    }
}

fn make_parent(target: &Path) -> Result<(), SyncError> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

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

async fn hash_in_background(path: &Path) -> Result<crate::hash::Hashed, SyncError> {
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
        let read_only =
            SyncError::from(tonic::Status::failed_precondition("\"bob/family\" is shared with you to read only"));
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
            let shared = Shared { name: "alice/family".into(), path: folder.path().to_path_buf() };
            let mut cfg = Config::new(url);
            cfg.shared.push(shared.clone());
            let attached =
                Attached::open(&cfg, "token", state.path(), &shared, Arc::new(Mutex::new(Vec::new()))).unwrap();
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
            h.changes =
                vec![master::gone("Trip/a.txt"), master::gone("Trip/b.txt"), master::live("trip/a.txt", b"first")];
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
        assert!(
            failed.summary(failed.last_cycle).contains("needs attention")
                || failed.summary(failed.last_cycle).contains("last problem")
        );
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
        let shared = Shared { name: "alice/family".into(), path: rig.folder.path().to_path_buf() };
        // Another process (`fafnirx status`) opens the same database and sees the same.
        let reopened = State::open(&state_file(rig._state.path(), "alice/family")).unwrap();
        let seen = status_of(&shared, &reopened);
        assert_eq!((seen.held, seen.files), (12, 12));
        assert!(seen.last_cycle > 0, "and how the last cycle ended");
        assert_eq!(seen, rig.attached.status());
        assert!(seen.last_error.contains("keep alice/family"), "the last problem says what to do: {}", seen.last_error);
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
