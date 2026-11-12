//! Plain-file store. The current version of every file lives under its own name:
//!   files/users/<user>/<name>/<path>               the live tree: the files of a folder of a user; a device's directory is <user>/<device>/<directory>
//!   files/<name>/<path>                            a master folder, which belongs to no user (its "folder" is master:<name>)
//!   versions/<version id>/<key>/<path>             what a newer version or a deletion replaced
//!   tmp/                                         uploads and downloads that are not complete yet

use anyhow::{bail, Context, Result};
use fafnirx_common::{file_has_root, hash_path, CHUNK_SIZE, HASH_LEN};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The start of the name of a master folder, a folder of the master files that belongs to no user: `master:<path>`. The same
/// words the clients and the API use.
pub const MASTER_PREFIX: &str = "master:";

/// Whether a folder (`<user>/<folder>`, or `master:<path>`) is a master folder.
pub fn is_master(folder: &str) -> bool {
    folder.starts_with(MASTER_PREFIX)
}

/// The folder of `files/` that holds the folders of all users, apart from the master files, so that a network share can
/// make it read-only in one place. Master files cannot start with this name.
pub const USERS_DIR: &str = "users";

/// A file as modules and the streaming proxy name it: its place below `files/` (`users/<key>/<path>` for the files
/// of a folder of a user or a device, `<name>/<path>` for a file of the master folder `<name>`).
pub fn tree_path(folder: &str, path: &str) -> String {
    match folder.strip_prefix(MASTER_PREFIX) {
        Some(name) => format!("{name}/{path}"),
        None => format!("{USERS_DIR}/{folder}/{path}"),
    }
}

/// The same name for a file found by its path on disk (`files` is the folder that holds the file tree): the way back from
/// `Store::live_path`. Modules are told the name, never the path.
pub fn tree_path_of(files: &Path, file: &Path) -> Result<String> {
    let relative = file.strip_prefix(files).context("the file is not in the file tree")?;
    Ok(relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
}

/// The modification time (unix nanoseconds) and the mode a file is given when it is put in place.
#[derive(Clone, Copy)]
pub struct Attrs {
    pub mtime_ns: i64,
    pub mode: u32,
}

pub struct Store {
    files: PathBuf,
    versions: PathBuf,
    tmp: PathBuf,
}

/// A file that is never going to be a chunk index anyone sends: bounds the size of a staging file.
const MAX_CHUNK_INDEX: u32 = 1 << 20;
/// Version folders the sweep checks against the database at a time.
const SWEEP_BATCH: usize = 10_000;

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        let store = Self { files: root.join("files"), versions: root.join("versions"), tmp: root.join("tmp") };
        fs::create_dir_all(store.files.join(USERS_DIR))?;
        fs::create_dir_all(&store.versions)?;
        // Interrupted transfers are removed by
        // `clean_tmp` once nobody has touched them for a day.
        fs::create_dir_all(&store.tmp)?;
        Ok(store)
    }

    /// `<base>/<key>`: `<user>/<name>` or `<user>/<device>/<directory>` (below `users/` when `nested`), or `<name>` for the master folder `master:<name>`.
    fn folder_in(base: &Path, folder: &str, nested: bool) -> Result<PathBuf> {
        if let Some(name) = folder.strip_prefix(MASTER_PREFIX) {
            let mut out = base.to_path_buf();
            for (n, part) in name.split('/').enumerate() {
                let reserved = n == 0 && part.to_lowercase() == USERS_DIR;
                if part.is_empty() || part.contains(['\\', ':']) || part.starts_with('.') || reserved {
                    bail!("master folder {folder:?} cannot be stored");
                }
                out.push(part);
            }
            return Ok(out);
        }
        let mut out = base.to_path_buf();
        if nested {
            out.push(USERS_DIR);
        }
        // `<user>/<name>`, or `<user>/<device>/<directory>`: the directory is whatever a device calls it, so it only has to be a
        // name that stays inside.
        let parts: Vec<&str> = folder.split('/').collect();
        if !(2..=3).contains(&parts.len()) {
            bail!("folder {folder:?} cannot be stored");
        }
        for (n, part) in parts.iter().enumerate() {
            let bad = if n == 2 {
                part.is_empty() || *part == "." || *part == ".." || part.contains('\\') || (cfg!(windows) && part.contains(':'))
            } else {
                part.is_empty() || part.contains(['\\', ':']) || part.starts_with('.')
            };
            if bad {
                bail!("folder {folder:?} cannot be stored");
            }
            out.push(part);
        }
        Ok(out)
    }

    /// `<base>/<key>/<path>`. Remote paths are already validated; this refuses what could still
    /// leave the directory on the server's platform.
    fn resolve(base: &Path, nested: bool, folder: &str, path: &str) -> Result<PathBuf> {
        let mut out = Self::folder_in(base, folder, nested)?;
        for part in path.split('/') {
            if part.is_empty() || part == "." || part == ".." || part.contains('\\') || (cfg!(windows) && part.contains(':')) {
                bail!("path component {part:?} cannot be stored");
            }
            out.push(part);
        }
        Ok(out)
    }

    /// The live tree: one folder per user with the folders of the user and of their devices in it, and the master folders beside them.
    pub fn live_root(&self) -> &Path {
        &self.files
    }

    /// The directory of a folder in the live tree.
    pub fn live_dir(&self, folder: &str) -> PathBuf {
        Self::folder_in(&self.files, folder, true).unwrap_or_else(|_| self.files.join("\0invalid"))
    }

    /// The folder of a user in the live tree.
    pub fn user_dir(&self, user: &str) -> PathBuf {
        self.files.join(USERS_DIR).join(user)
    }

    /// Moves a folder to its new name: its live folder and what its versions kept of it. Folders that are not there
    /// (nothing was ever stored, or a version kept no file of this folder) are skipped. Undone by calling it the other way.
    pub fn rename_folder(&self, old: &str, new: &str, versions: &[i64]) -> Result<()> {
        let (from, to) = (Self::folder_in(&self.files, old, true)?, Self::folder_in(&self.files, new, true)?);
        if from.is_dir() {
            if to.exists() {
                bail!("{} exists already", to.display());
            }
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(&from, &to).with_context(|| format!("move {} to {}", from.display(), to.display()))?;
        }
        for id in versions {
            let base = self.versions.join(id.to_string());
            let (from, to) = (Self::folder_in(&base, old, false)?, Self::folder_in(&base, new, false)?);
            if from.is_dir() {
                move_file(&from, &to)?;
            }
        }
        Ok(())
    }

    pub fn live_path(&self, folder: &str, path: &str) -> Result<PathBuf> {
        Self::resolve(&self.files, true, folder, path)
    }

    pub fn version_path(&self, version_id: i64, folder: &str, path: &str) -> Result<PathBuf> {
        Self::resolve(&self.versions.join(version_id.to_string()), false, folder, path)
    }

    /// A new path in `tmp`, unique by its random name.
    pub fn temp_path(&self) -> PathBuf {
        self.tmp.join(hex::encode(crate::platform::db::random_bytes::<16>()))
    }

    /// Where the chunks of an upload accumulate until the commit. Keyed by device and content so that
    /// parallel streams of one file land in the same place.
    fn upload_path(&self, folder_id: i64, root: &[u8; HASH_LEN]) -> PathBuf {
        self.tmp.join(format!("up-{folder_id}-{}", hex::encode(root)))
    }

    /// Writes one chunk of an upload at its place in the file.
    pub fn write_chunk(&self, folder_id: i64, root: &[u8; HASH_LEN], index: u32, data: &[u8]) -> Result<()> {
        if index >= MAX_CHUNK_INDEX {
            bail!("chunk index {index} out of range");
        }
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.upload_path(folder_id, root))
            .context("open upload file")?;
        f.seek(SeekFrom::Start(index as u64 * CHUNK_SIZE as u64))?;
        f.write_all(data)?;
        Ok(())
    }

    /// Indexes of the chunks that an interrupted upload of this content has not delivered yet:
    /// those missing from the partial file or not matching their hash.
    pub fn missing_chunks(&self, folder_id: i64, root: &[u8; HASH_LEN], hashes: &[[u8; HASH_LEN]]) -> Vec<u32> {
        let Ok(mut f) = fs::File::open(self.upload_path(folder_id, root)) else {
            return (0..hashes.len() as u32).collect();
        };
        let mut buf = vec![0u8; CHUNK_SIZE];
        (0..hashes.len() as u32)
            .filter(|&i| {
                let have = f
                    .seek(SeekFrom::Start(i as u64 * CHUNK_SIZE as u64))
                    .and_then(|_| read_up_to(&mut f, &mut buf))
                    .unwrap_or(0);
                have == 0 || fafnirx_common::chunk_hash(&buf[..have]) != hashes[i as usize]
            })
            .collect()
    }

    /// Checks that the uploaded file has exactly the announced content and returns where it is.
    /// An empty file has no chunks, so it is created here.
    pub fn finish_upload(&self, folder_id: i64, root: &[u8; HASH_LEN], size: u64) -> Result<PathBuf> {
        let path = self.upload_path(folder_id, root);
        if size == 0 && !path.exists() {
            fs::File::create(&path)?;
        }
        let actual = fs::metadata(&path).context("no uploaded data for this file")?.len();
        if actual != size {
            bail!("uploaded file is {actual} bytes, expected {size}");
        }
        if !file_has_root(&path, root) {
            bail!("uploaded file does not match its hash");
        }
        Ok(path)
    }

    /// Makes a temporary copy of a live file so that it can be committed again under another path:
    /// identical content needs no upload. It is a real copy, not a link: the live tree can be written
    /// to directly, and a change to one name must not change another. The copy is checked against `root`:
    /// a file edited in the live tree since it was recorded is not the content that was asked for.
    pub fn copy_live_to_temp(&self, folder: &str, path: &str, root: &[u8; HASH_LEN]) -> Result<PathBuf> {
        let from = self.live_path(folder, path)?;
        let to = self.temp_path();
        fs::copy(&from, &to).with_context(|| format!("copy {}", from.display()))?;
        if !file_has_root(&to, root) {
            let _ = fs::remove_file(&to);
            bail!("{path} changed since it was recorded");
        }
        Ok(to)
    }

    /// Moves the live file of a path out of the live tree into the folder of `version_id`.
    /// Returns false when there was no live file.
    pub fn archive(&self, folder: &str, path: &str, version_id: i64) -> Result<bool> {
        let from = self.live_path(folder, path)?;
        if !from.is_file() {
            return Ok(false);
        }
        let to = self.version_path(version_id, folder, path)?;
        move_file(&from, &to)?;
        Ok(true)
    }

    /// Undoes [`archive`].
    pub fn unarchive(&self, folder: &str, path: &str, version_id: i64) -> Result<()> {
        move_file(&self.version_path(version_id, folder, path)?, &self.live_path(folder, path)?)
    }

    /// Puts a complete file in place as the live file of a path, with its modification time and mode.
    pub fn install(&self, folder: &str, path: &str, from: &Path, attrs: Attrs) -> Result<()> {
        let to = self.live_path(folder, path)?;
        set_attributes(from, attrs.mtime_ns, attrs.mode)?;
        move_file(from, &to)
    }

    /// Removes the live file of a path (to undo an [`install`]).
    pub fn discard_live(&self, folder: &str, path: &str) {
        if let Ok(p) = self.live_path(folder, path) {
            let _ = fs::remove_file(p);
        }
    }

    /// Deletes everything kept for a version.
    pub fn remove_version(&self, version_id: i64) {
        let _ = fs::remove_dir_all(self.versions.join(version_id.to_string()));
    }

    /// Deletes the folders of versions that no record points to (left behind when the master stopped between
    /// the database and the disk). Folders younger than `min_age` may belong to a commit in progress. `known` says which
    /// of some ids have a record; it is asked about [`SWEEP_BATCH`] folders at a time, so a large store is not held in memory.
    pub fn sweep_versions<E>(&self, min_age: Duration, mut known: impl FnMut(&[i64]) -> Result<HashSet<i64>, E>) -> Result<usize, E> {
        let now = SystemTime::now();
        let mut removed = 0;
        let mut batch: Vec<i64> = Vec::with_capacity(SWEEP_BATCH);
        let mut entries = fs::read_dir(&self.versions).into_iter().flatten().flatten().peekable();
        while entries.peek().is_some() {
            batch.clear();
            for e in entries.by_ref() {
                let Some(id) = e.file_name().to_str().and_then(|n| n.parse::<i64>().ok()) else { continue };
                let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| now.duration_since(t).ok());
                if old.is_some_and(|age| age > min_age) {
                    batch.push(id);
                    if batch.len() == SWEEP_BATCH {
                        break;
                    }
                }
            }
            if batch.is_empty() {
                break;
            }
            let have = known(&batch)?;
            for id in batch.iter().filter(|id| !have.contains(id)) {
                if fs::remove_dir_all(self.versions.join(id.to_string())).is_ok() {
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    /// Deletes uploads and downloads that nobody has touched for `max_age`.
    pub fn clean_tmp(&self, max_age: Duration) -> usize {
        let now = SystemTime::now();
        let mut removed = 0;
        for e in fs::read_dir(&self.tmp).into_iter().flatten().flatten() {
            let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| now.duration_since(t).ok());
            if old.is_some_and(|age| age > max_age) && fs::remove_file(e.path()).is_ok() {
                removed += 1;
            }
        }
        removed
    }
}

/// Makes `to` another name for the content of `from`: a hard link, or a copy where that is not
/// possible. Returns whether it is a link (a copy has a new modification time).
pub fn link_or_copy(from: &Path, to: &Path) -> Result<bool> {
    if fs::hard_link(from, to).is_ok() {
        return Ok(true);
    }
    fs::copy(from, to).with_context(|| format!("copy {}", from.display()))?;
    Ok(false)
}

/// Removes a file or a directory when dropped, however the code that made it ends (a panic included).
pub struct RemoveOnDrop(pub PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0).or_else(|_| fs::remove_file(&self.0));
    }
}

/// (root hash, size) of a file's content.
pub fn hash_file(path: &Path) -> Result<([u8; HASH_LEN], u64)> {
    let h = hash_path(path).with_context(|| format!("read {}", path.display()))?;
    Ok((h.root, h.size))
}

/// Reads until `buf` is full or the file ends; returns the number of bytes read.
fn read_up_to(f: &mut fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match f.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

fn set_attributes(path: &Path, mtime_ns: i64, mode: u32) -> Result<()> {
    let f = fs::OpenOptions::new().write(true).open(path)?;
    if mtime_ns > 0 {
        f.set_modified(UNIX_EPOCH + Duration::from_nanos(mtime_ns as u64))?;
    }
    drop(f);
    #[cfg(unix)]
    if mode != 0 {
        use std::os::unix::fs::PermissionsExt;
        // Keep the owner able to read and rewrite it, whatever the source had.
        fs::set_permissions(path, fs::Permissions::from_mode((mode & 0o777) | 0o600))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

fn move_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    fs::rename(from, to).with_context(|| format!("move {} to {}", from.display(), to.display()))
}

#[cfg(test)]
#[path = "../../tests/unit/platform/store.rs"]
mod tests;
