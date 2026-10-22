//! Plain-file store. The current version of every file lives under its own name:
//!   files/<device name>/<path>                     the live tree
//!   versions/<version id>/<device name>/<path>     what a newer version or a deletion replaced
//!   tmp/                                         uploads and downloads that are not complete yet

use anyhow::{bail, Context, Result};
use fafnirx_common::{ChunkHasher, CHUNK_SIZE, HASH_LEN};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct Store {
    files: PathBuf,
    versions: PathBuf,
    tmp: PathBuf,
}

/// A file that is never going to be a chunk index anyone sends: bounds the size of a staging file.
const MAX_CHUNK_INDEX: u32 = 1 << 20;

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        let store = Self { files: root.join("files"), versions: root.join("versions"), tmp: root.join("tmp") };
        fs::create_dir_all(&store.files)?;
        fs::create_dir_all(&store.versions)?;
        // Interrupted transfers are removed by
        // `clean_tmp` once nobody has touched them for a day.
        fs::create_dir_all(&store.tmp)?;
        Ok(store)
    }

    /// `<base>/<device name>/<path>`. Remote paths are already validated; this refuses what could still
    /// leave the directory on the server's platform.
    fn resolve(base: &Path, device: &str, path: &str) -> Result<PathBuf> {
        if device.is_empty() || device.contains(['/', '\\', ':']) || device.starts_with('.') {
            bail!("device name {device:?} cannot be stored");
        }
        let mut out = base.join(device);
        for part in path.split('/') {
            if part.is_empty()
                || part == "."
                || part == ".."
                || part.contains('\\')
                || (cfg!(windows) && part.contains(':'))
            {
                bail!("path component {part:?} cannot be stored");
            }
            out.push(part);
        }
        Ok(out)
    }

    /// The live tree: one folder per device.
    pub fn live_root(&self) -> &Path {
        &self.files
    }

    /// The folder of a device in the live tree.
    pub fn live_dir(&self, device: &str) -> PathBuf {
        self.files.join(device)
    }

    pub fn live_path(&self, device: &str, path: &str) -> Result<PathBuf> {
        Self::resolve(&self.files, device, path)
    }

    pub fn version_path(&self, version_id: i64, device: &str, path: &str) -> Result<PathBuf> {
        Self::resolve(&self.versions.join(version_id.to_string()), device, path)
    }

    /// A new path in `tmp`, unique by its random name.
    pub fn temp_path(&self) -> PathBuf {
        self.tmp.join(hex::encode(crate::platform::db::random_bytes::<16>()))
    }

    /// Where the chunks of an upload accumulate until the commit. Keyed by device and content so that
    /// parallel streams of one file land in the same place.
    fn upload_path(&self, device_id: i64, root: &[u8; HASH_LEN]) -> PathBuf {
        self.tmp.join(format!("up-{device_id}-{}", hex::encode(root)))
    }

    /// Writes one chunk of an upload at its place in the file.
    pub fn write_chunk(&self, device_id: i64, root: &[u8; HASH_LEN], index: u32, data: &[u8]) -> Result<()> {
        if index >= MAX_CHUNK_INDEX {
            bail!("chunk index {index} out of range");
        }
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.upload_path(device_id, root))
            .context("open upload file")?;
        f.seek(SeekFrom::Start(index as u64 * CHUNK_SIZE as u64))?;
        f.write_all(data)?;
        Ok(())
    }

    /// Indexes of the chunks that an interrupted upload of this content has not delivered yet:
    /// those missing from the partial file or not matching their hash.
    pub fn missing_chunks(&self, device_id: i64, root: &[u8; HASH_LEN], hashes: &[[u8; HASH_LEN]]) -> Vec<u32> {
        let Ok(mut f) = fs::File::open(self.upload_path(device_id, root)) else {
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
    pub fn finish_upload(&self, device_id: i64, root: &[u8; HASH_LEN], size: u64) -> Result<PathBuf> {
        let path = self.upload_path(device_id, root);
        if size == 0 && !path.exists() {
            fs::File::create(&path)?;
        }
        let actual = fs::metadata(&path).context("no uploaded data for this file")?.len();
        if actual != size {
            bail!("uploaded file is {actual} bytes, expected {size}");
        }
        let (got, _) = hash_file(&path)?;
        if got != *root {
            bail!("uploaded file does not match its hash");
        }
        Ok(path)
    }

    /// Makes a temporary copy of a live file so that it can be committed again under another path:
    /// identical content needs no upload. It is a real copy, not a link: the live tree can be written
    /// to directly, and a change to one name must not change another. The copy is checked against `root`:
    /// a file edited in the live tree since it was recorded is not the content that was asked for.
    pub fn copy_live_to_temp(&self, device: &str, path: &str, root: &[u8; HASH_LEN]) -> Result<PathBuf> {
        let from = self.live_path(device, path)?;
        let to = self.temp_path();
        fs::copy(&from, &to).with_context(|| format!("copy {}", from.display()))?;
        if hash_file(&to).map(|(got, _)| got != *root).unwrap_or(true) {
            let _ = fs::remove_file(&to);
            bail!("{path} changed since it was recorded");
        }
        Ok(to)
    }

    /// Moves the live file of a path out of the live tree into the folder of `version_id`.
    /// Returns false when there was no live file.
    pub fn archive(&self, device: &str, path: &str, version_id: i64) -> Result<bool> {
        let from = self.live_path(device, path)?;
        if !from.is_file() {
            return Ok(false);
        }
        let to = self.version_path(version_id, device, path)?;
        move_file(&from, &to)?;
        Ok(true)
    }

    /// Undoes [`archive`].
    pub fn unarchive(&self, device: &str, path: &str, version_id: i64) -> Result<()> {
        move_file(&self.version_path(version_id, device, path)?, &self.live_path(device, path)?)
    }

    /// Puts a complete file in place as the live file of a path, with its modification time and mode.
    pub fn install(&self, device: &str, path: &str, from: &Path, mtime_ns: i64, mode: u32) -> Result<()> {
        let to = self.live_path(device, path)?;
        set_attributes(from, mtime_ns, mode)?;
        move_file(from, &to)
    }

    /// Removes the live file of a path (to undo an [`install`]).
    pub fn discard_live(&self, device: &str, path: &str) {
        if let Ok(p) = self.live_path(device, path) {
            let _ = fs::remove_file(p);
        }
    }

    /// Deletes everything kept for a version.
    pub fn remove_version(&self, version_id: i64) {
        let _ = fs::remove_dir_all(self.versions.join(version_id.to_string()));
    }

    /// Deletes the folders of versions that no record points to (left behind when the master stopped between
    /// the database and the disk). Folders younger than `min_age` may belong to a commit in progress.
    pub fn sweep_versions(&self, known: &std::collections::HashSet<i64>, min_age: Duration) -> usize {
        let now = SystemTime::now();
        let mut removed = 0;
        for e in fs::read_dir(&self.versions).into_iter().flatten().flatten() {
            let Some(id) = e.file_name().to_str().and_then(|n| n.parse::<i64>().ok()) else { continue };
            let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| now.duration_since(t).ok());
            if !known.contains(&id) && old.is_some_and(|age| age > min_age) && fs::remove_dir_all(e.path()).is_ok() {
                removed += 1;
            }
        }
        removed
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

/// (root hash, size) of a file's content.
pub fn hash_file(path: &Path) -> Result<([u8; HASH_LEN], u64)> {
    let mut f = fs::File::open(path).with_context(|| format!("read {}", path.display()))?;
    let mut h = ChunkHasher::default();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(h.finish());
        }
        h.update(&buf[..n]);
    }
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
mod tests {
    use super::*;
    use fafnirx_common::{chunk_hash, root_hash};

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn upload_is_assembled_from_chunks_and_installed_under_its_name() {
        let (_dir, store) = store();
        let data = vec![7u8; CHUNK_SIZE + 5];
        let chunks: Vec<_> = data.chunks(CHUNK_SIZE).map(chunk_hash).collect();
        let root = root_hash(&chunks, data.len() as u64);

        store.write_chunk(1, &root, 1, &data[CHUNK_SIZE..]).unwrap();
        assert!(store.finish_upload(1, &root, data.len() as u64).is_err(), "a missing chunk is detected");
        store.write_chunk(1, &root, 0, &data[..CHUNK_SIZE]).unwrap();
        let staged = store.finish_upload(1, &root, data.len() as u64).unwrap();

        store.install("d", "docs/a.bin", &staged, 1_000_000_000, 0).unwrap();
        assert_eq!(fs::read(store.live_path("d", "docs/a.bin").unwrap()).unwrap(), data);
    }

    #[test]
    fn an_interrupted_upload_resumes_with_the_missing_chunks() {
        let (_dir, store) = store();
        let data: Vec<u8> = (0..CHUNK_SIZE * 2 + 5).map(|i| (i % 249) as u8).collect();
        let hashes: Vec<_> = data.chunks(CHUNK_SIZE).map(chunk_hash).collect();
        let root = root_hash(&hashes, data.len() as u64);
        assert_eq!(store.missing_chunks(1, &root, &hashes), [0, 1, 2], "nothing uploaded yet");

        store.write_chunk(1, &root, 1, &data[CHUNK_SIZE..2 * CHUNK_SIZE]).unwrap();
        store.write_chunk(1, &root, 2, b"junk").unwrap();
        assert_eq!(store.missing_chunks(1, &root, &hashes), [0, 2], "a chunk that does not match is still missing");
        assert_eq!(store.missing_chunks(2, &root, &hashes), [0, 1, 2], "uploads are per device");
    }

    #[test]
    fn corrupt_or_wrong_size_uploads_are_rejected() {
        let (_dir, store) = store();
        let root = root_hash(&[chunk_hash(b"hello")], 5);
        store.write_chunk(1, &root, 0, b"hellx").unwrap();
        assert!(store.finish_upload(1, &root, 5).is_err(), "content differs from the hash");
        assert!(store.finish_upload(1, &root, 6).is_err(), "size differs");
    }

    #[test]
    fn empty_files_need_no_chunks() {
        let (_dir, store) = store();
        let root = root_hash(&[], 0);
        let staged = store.finish_upload(1, &root, 0).unwrap();
        store.install("d", "empty", &staged, 0, 0).unwrap();
        assert_eq!(fs::metadata(store.live_path("d", "empty").unwrap()).unwrap().len(), 0);
    }

    #[test]
    fn archive_moves_the_live_file_into_its_version_and_back() {
        let (_dir, store) = store();
        let tmp = store.temp_path();
        fs::write(&tmp, b"one").unwrap();
        store.install("d", "a/b.txt", &tmp, 0, 0).unwrap();

        assert!(store.archive("d", "a/b.txt", 9).unwrap());
        assert!(!store.live_path("d", "a/b.txt").unwrap().exists());
        assert_eq!(fs::read(store.version_path(9, "d", "a/b.txt").unwrap()).unwrap(), b"one");
        assert!(!store.archive("d", "a/b.txt", 10).unwrap(), "nothing live to archive");

        store.unarchive("d", "a/b.txt", 9).unwrap();
        assert!(store.live_path("d", "a/b.txt").unwrap().is_file());
        store.archive("d", "a/b.txt", 9).unwrap();
        store.remove_version(9);
        assert!(!store.version_path(9, "d", "a/b.txt").unwrap().exists());
    }

    #[test]
    fn copying_a_live_file_leaves_the_original() {
        let (_dir, store) = store();
        let tmp = store.temp_path();
        fs::write(&tmp, b"same").unwrap();
        let (root, _) = hash_file(&tmp).unwrap();
        store.install("d", "x", &tmp, 0, 0).unwrap();
        let copy = store.copy_live_to_temp("d", "x", &root).unwrap();
        store.install("d", "y", &copy, 0, 0).unwrap();
        assert_eq!(fs::read(store.live_path("d", "x").unwrap()).unwrap(), b"same");
        assert_eq!(fs::read(store.live_path("d", "y").unwrap()).unwrap(), b"same");

        fs::write(store.live_path("d", "x").unwrap(), b"edited").unwrap();
        assert!(store.copy_live_to_temp("d", "x", &root).is_err(), "a file edited since it was recorded is refused");
    }

    #[test]
    fn only_versions_without_a_record_are_swept() {
        let (_dir, store) = store();
        for id in [1, 2] {
            fs::create_dir_all(store.versions.join(id.to_string())).unwrap();
        }
        let known = [1].into_iter().collect();
        assert_eq!(store.sweep_versions(&known, Duration::from_secs(3600)), 0, "too new: a commit may be running");
        assert_eq!(store.sweep_versions(&known, Duration::ZERO), 1);
        assert!(store.versions.join("1").is_dir() && !store.versions.join("2").exists());
    }

    #[test]
    fn paths_cannot_escape_the_store() {
        let (_dir, store) = store();
        for bad in ["../x", "a/../../x", "a//b", "a\\b", ""] {
            assert!(store.live_path("d", bad).is_err(), "{bad:?}");
        }
    }
}
