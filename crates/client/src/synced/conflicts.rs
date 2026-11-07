//! A file changed on both sides: what the mode of the folder decides, what is put aside, and the answers of a person.

use super::files::{conflict_name, local_path, local_stat, make_parent};
use super::Attached;
use super::Master;
use super::Settled;
use super::CONFLICT_DIR;
use crate::config::Mode;
use crate::sync::state::Pending;
use crate::sync::Report;
use crate::sync::SyncError;
use anyhow::Result;
use fafnirx_common::pb;
use fafnirx_common::unix_now;
use std::path::Path;

impl Attached {
    /// The policy of this folder. `ask` needs the tray to show the question; a build without it cannot, and keeps the master's.
    pub(super) fn policy(&self) -> Mode {
        match self.synced.mode {
            Mode::Ask if !Mode::ask_possible() => Mode::Both,
            mode => mode,
        }
    }

    /// A file changed here and on the master (or changed here and deleted there): does what the policy of the folder says.
    /// `Settled::Receive`: this computer's version is out of the way, the master's may take its place. `Settled::Leave`: the
    /// file here stays as it is and nothing is received for it now.
    pub(super) fn settle(&self, path: &str, target: &Path, master: Master<'_>, report: &mut Report) -> Result<Settled, SyncError> {
        let state = &self.engine.state;
        let settled = match self.policy() {
            Mode::Both | Mode::Up => {
                self.put_aside(path, target, report)?;
                Settled::Receive
            }
            Mode::Keep => {
                self.keep_as_conflict(path, target, report)?;
                Settled::Receive
            }
            Mode::Ask => {
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
                    self.synced.name
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
    pub(super) fn put_aside(&self, path: &str, target: &Path, report: &mut Report) -> Result<(), SyncError> {
        let dir = self.synced.path.join(CONFLICT_DIR);
        let kept = conflict_name(&local_path(&dir, path), &self.host, unix_now());
        make_parent(&self.synced.path, &kept)?;
        std::fs::rename(target, &kept)?;
        report.conflicts += 1;
        self.engine.report_error(format!(
            "{}/{path}: changed here and on the server. The server's is kept; this computer's is in {CONFLICT_DIR}/{}",
            self.synced.name,
            kept.strip_prefix(&dir).unwrap_or(&kept).display()
        ));
        Ok(())
    }

    /// Applies the answers given for the files that waited, and settles those nobody answered in time as `both`.
    pub(super) async fn apply_answers(&mut self, report: &mut Report) -> Result<(), SyncError> {
        let (now, hours) = (unix_now(), self.synced.ask_timeout_hours);
        let wait = (hours * 3600) as i64;
        for pending in self.engine.state.pendings()? {
            let choice = match pending.choice.as_deref() {
                Some(choice) => choice.to_string(),
                None if wait > 0 && now - pending.found_at >= wait => {
                    self.engine.report_error(format!(
                        "{}/{}: nobody chose in {} hours; both versions are kept",
                        self.synced.name, pending.path, hours
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

    pub(super) async fn apply_choice(&mut self, p: &Pending, choice: &str, report: &mut Report) -> Result<(), SyncError> {
        let target = local_path(&self.synced.path, &p.path);
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
        if here {
            // (An answer `mine`, given before that choice was removed and not yet applied, keeps both.)
            if matches!(choice, "both" | "mine") {
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
    pub(super) fn keep_as_conflict(&self, path: &str, target: &Path, report: &mut Report) -> Result<(), SyncError> {
        let kept = conflict_name(target, &self.host, unix_now());
        std::fs::rename(target, &kept)?;
        report.conflicts += 1;
        self.engine.report_error(format!(
            "{}/{path}: changed here and on the server. Both are kept; the one from this computer is {}",
            self.synced.name,
            kept.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        ));
        Ok(())
    }
}
