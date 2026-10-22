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
}

/// Fails when the folder itself cannot be read; callers must then not treat
/// its files as deleted.
/// `exclude` is the client's own state folder: if a collected folder contains it, it is not collected.
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

    let skip = exclude.map(Path::to_path_buf);
    let walker = ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .follow_links(false)
        .add_custom_ignore_filename(IGNORE_FILE)
        .filter_entry(move |e| skip.as_deref().is_none_or(|s| !e.path().starts_with(s)))
        .build();
    for item in walker {
        let dirent = match item {
            Ok(d) => d,
            Err(e) => {
                skipped.push(e.to_string());
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
        let meta = match dirent.metadata() {
            Ok(m) => m,
            Err(e) => {
                skipped.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        // A file another process holds open exclusively (Windows) cannot be read.
        if let Err(e) = std::fs::File::open(path) {
            skipped.push(format!("{}: cannot be opened ({e})", path.display()));
            continue;
        }
        let Some(remote) = remote_path(&top, root, path) else {
            skipped.push(format!("{}: name is not valid unicode", path.display()));
            continue;
        };
        entries.push(Entry {
            remote,
            local: path.to_path_buf(),
            size: meta.len(),
            mtime_ns: mtime_ns(&meta),
            mode: mode_of(&meta),
        });
    }

    entries.sort_by(|a, b| a.remote.cmp(&b.remote));
    drop_case_conflicts(&mut entries, &mut skipped);
    Ok(Scan { entries, skipped })
}

/// Paths that differ only by case would collide on case-insensitive filesystems (mirrors, restores).
/// The first one in sorted order is kept; entries must be sorted by `remote`.
fn drop_case_conflicts(entries: &mut Vec<Entry>, skipped: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| {
        let unique = seen.insert(e.remote.to_lowercase());
        if !unique {
            skipped.push(format!("{}: path differs only by case from another file", e.local.display()));
        }
        unique
    });
}

fn remote_path(top: &str, root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut parts = vec![top.to_string()];
    for c in rel.components() {
        parts.push(c.as_os_str().to_str()?.nfc().collect::<String>());
    }
    clean_remote_path(&parts.join("/")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn folder(dir: &Path) -> Folder {
        Folder { path: dir.to_path_buf(), name: "docs".into() }
    }

    #[test]
    fn scans_with_ignore_file_and_nested_dirs() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("a/b")).unwrap();
        fs::write(d.path().join("a/b/c.txt"), "hi").unwrap();
        fs::write(d.path().join("keep.txt"), "x").unwrap();
        fs::write(d.path().join("skip.tmp"), "x").unwrap();
        fs::write(d.path().join(IGNORE_FILE), "*.tmp\n").unwrap();
        let scan = scan_folder(&folder(d.path()), None).unwrap();
        let remotes: Vec<_> = scan.entries.iter().map(|e| e.remote.as_str()).collect();
        assert_eq!(remotes, [format!("docs/{IGNORE_FILE}").as_str(), "docs/a/b/c.txt", "docs/keep.txt"]);
        assert_eq!(scan.entries[1].size, 2);
    }

    #[test]
    fn paths_that_differ_only_by_case_keep_the_first() {
        let entry = |remote: &str| Entry { remote: remote.into(), local: remote.into(), size: 0, mtime_ns: 0, mode: 0 };
        let mut entries = vec![entry("docs/A/x.txt"), entry("docs/a/x.txt"), entry("docs/b.txt"), entry("docs/B.TXT")];
        let mut skipped = Vec::new();
        drop_case_conflicts(&mut entries, &mut skipped);
        let kept: Vec<_> = entries.iter().map(|e| e.remote.as_str()).collect();
        assert_eq!(kept, ["docs/A/x.txt", "docs/b.txt"], "directories that differ only by case conflict too");
        assert_eq!(skipped.len(), 2);
    }

    #[test]
    fn missing_folder_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(scan_folder(&folder(&d.path().join("gone")), None).is_err());
    }

    #[test]
    fn names_are_nfc() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("e\u{301}.txt"), "x").unwrap();
        let scan = scan_folder(&folder(d.path()), None).unwrap();
        assert_eq!(scan.entries[0].remote, "docs/\u{e9}.txt");
    }
}
