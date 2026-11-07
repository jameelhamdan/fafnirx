//! The local side of a synced folder: where a path is, making the folders above it, what a file looks like on disk, names for conflicts.

use crate::sync::hash::hash_file;
use crate::sync::hash::stat;
use crate::sync::state::Synced;
use crate::sync::SyncError;
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;

/// The place of a path of the synced folder on this computer. The path is clean: relative, without `..`.
pub(super) fn local_path(root: &Path, remote: &str) -> PathBuf {
    remote.split('/').fold(root.to_path_buf(), |p, part| p.join(part))
}

/// Makes the folders above `target`, inside the attached folder `root`, for whoever owns that folder.
pub(super) fn make_parent(root: &Path, target: &Path) -> Result<(), SyncError> {
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
pub(super) fn no_link_between(root: &Path, dir: &Path) -> Result<(), SyncError> {
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
pub(super) fn give_to_owner_of(root: &Path, path: &Path) {
    use std::os::unix::fs::MetadataExt;
    if let Ok(owner) = std::fs::metadata(root) {
        // Not through a link: only what the service made is given away.
        let _ = std::os::unix::fs::lchown(path, Some(owner.uid()), Some(owner.gid()));
    }
}

#[cfg(not(unix))]
pub(super) fn give_to_owner_of(_: &Path, _: &Path) {}

/// (size, modification time) of the file at a path; `None` when there is none. A folder in its place is a problem.
pub(super) fn local_stat(target: &Path) -> Result<Option<(u64, i64)>, SyncError> {
    match std::fs::metadata(target) {
        Ok(m) if m.is_dir() => Err(SyncError::File("a folder of that name is in the way".into())),
        Ok(_) => Ok(Some(stat(target)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Whether the local file is the one that was in step: it has the size and time recorded then.
pub(super) fn unchanged(base: &Synced, (size, mtime_ns): (u64, i64)) -> bool {
    base.size == size as i64 && base.mtime_ns == mtime_ns
}

pub(super) async fn hash_in_background(path: &Path) -> Result<crate::sync::hash::Hashed, SyncError> {
    let p = path.to_path_buf();
    tokio::task::spawn_blocking(move || hash_file(&p))
        .await
        .map_err(|e| SyncError::File(format!("hashing failed: {e}")))?
        .map_err(|e| SyncError::File(format!("{e:#}")))
}

/// `name (conflict <computer> <date>).ext`, or with a number when that name is taken too.
pub(super) fn conflict_name(target: &Path, host: &str, now: i64) -> PathBuf {
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
pub(super) fn civil_date(unix: i64) -> String {
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
