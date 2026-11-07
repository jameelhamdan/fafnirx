//! Walks a collected folder: applies `.fafnirxignore`, skips symlinks, locked
//! and case-conflicting entries, and builds NFC remote paths.

use crate::config::Folder;
use anyhow::{bail, Result};
use fafnirx_common::{clean_remote_path, mode_of, mtime_ns};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub const IGNORE_FILE: &str = ".fafnirxignore";

#[derive(Debug, Clone)]
pub struct Entry {
    pub remote: String,
    pub local: PathBuf,
    pub size: u64,
    pub mtime_ns: i64,
    pub mode: u32,
}

pub struct Scan {
    pub entries: Vec<Entry>,
    /// Human readable notes about everything that was skipped.
    pub skipped: Vec<String>,
    /// Remote paths of files that exist but cannot be read right now (locked, no permission). They are not
    /// entries, yet they are not deleted either.
    pub unreadable: Vec<String>,
    /// False when part of the tree could not be walked (an unreadable directory): what is missing from the entries
    /// may still exist, so nothing in this folder may be taken for deleted.
    pub complete: bool,
}

/// Fails when the folder itself cannot be read; callers must then not treat
/// its files as deleted.
/// `exclude` is the client's own state folder: if a collected folder contains it, it is not collected.
/// A folder with no name is a synced folder, which every kind of computer must be able to hold: names Windows cannot keep
/// are skipped.
pub fn scan_folder(folder: &Folder, exclude: Option<&Path>) -> Result<Scan> {
    let root = &folder.path;
    match std::fs::metadata(root) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => bail!("{} is not a directory", root.display()),
        Err(e) => bail!("cannot read {}: {e}", root.display()),
    }
    let top: String = folder.name.nfc().collect();
    let mut skipped = Vec::new();
    let mut entries = Vec::new();
    let mut unreadable = Vec::new();
    let mut complete = true;

    let skip = exclude.map(Path::to_path_buf);
    let synced = top.is_empty();
    let walker = ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .follow_links(false)
        .add_custom_ignore_filename(IGNORE_FILE)
        .filter_entry(move |e| {
            // What a synced folder put aside (see `synced::CONFLICT_DIR`) is this computer's own.
            let aside = synced && e.depth() == 1 && e.file_name() == crate::synced::CONFLICT_DIR;
            !aside && skip.as_deref().is_none_or(|s| !e.path().starts_with(s))
        })
        .build();
    for item in walker {
        let dirent = match item {
            Ok(d) => d,
            Err(e) => {
                skipped.push(e.to_string());
                complete = false;
                continue;
            }
        };
        let Some(ft) = dirent.file_type() else { continue };
        if ft.is_dir() {
            continue;
        }
        let path = dirent.path();
        if ft.is_symlink() {
            skipped.push(format!("{}: symlink", path.display()));
            continue;
        }
        if !ft.is_file() {
            skipped.push(format!("{}: not a regular file", path.display()));
            continue;
        }
        // A file of a synced folder that is being received.
        if top.is_empty() && path.file_name().is_some_and(|n| n.to_string_lossy().starts_with(crate::synced::PART_PREFIX)) {
            continue;
        }
        let Some(remote) = remote_path(&top, root, path) else {
            skipped.push(format!("{}: name is not valid unicode", path.display()));
            continue;
        };
        if let Some(why) = top.is_empty().then(|| fafnirx_common::not_portable(&remote)).flatten() {
            skipped.push(format!("{}: {why}", path.display()));
            continue;
        }
        let meta = match dirent.metadata() {
            Ok(m) => m,
            Err(e) => {
                skipped.push(format!("{}: {e}", path.display()));
                unreadable.push(remote);
                continue;
            }
        };
        // A file another process holds open exclusively (Windows) cannot be read.
        if let Err(e) = std::fs::File::open(path) {
            skipped.push(format!("{}: cannot be opened ({e})", path.display()));
            unreadable.push(remote);
            continue;
        }
        entries.push(Entry { remote, local: path.to_path_buf(), size: meta.len(), mtime_ns: mtime_ns(&meta), mode: mode_of(&meta) });
    }

    entries.sort_by(|a, b| a.remote.cmp(&b.remote));
    drop_case_conflicts(&mut entries, &mut skipped);
    Ok(Scan { entries, skipped, unreadable, complete })
}

/// Paths that differ only by case would collide on case-insensitive filesystems (mirrors, restores).
/// The first one in sorted order is kept; entries must be sorted by `remote`.
fn drop_case_conflicts(entries: &mut Vec<Entry>, skipped: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| {
        let unique = seen.insert(fafnirx_common::fold_path(&e.remote));
        if !unique {
            skipped.push(format!("{}: path differs only by case from another file", e.local.display()));
        }
        unique
    });
}

fn remote_path(top: &str, root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    // A synced folder has no name of its own in its paths: its files are at the top.
    let mut parts = if top.is_empty() { Vec::new() } else { vec![top.to_string()] };
    for c in rel.components() {
        parts.push(c.as_os_str().to_str()?.nfc().collect::<String>());
    }
    clean_remote_path(&parts.join("/")).ok()
}

#[cfg(test)]
#[path = "../../tests/unit/sync/scan.rs"]
mod tests;
