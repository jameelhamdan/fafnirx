//! Content-addressed chunk store: `chunks/ab/cd/<64 hex>`.

use anyhow::{Context, Result};
use fafnirx_common::HASH_LEN;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

pub struct Store {
    chunks: PathBuf,
    tmp: PathBuf,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        let chunks = root.join("chunks");
        let tmp = root.join("tmp");
        fs::create_dir_all(&chunks)?;
        // Anything left in tmp is an interrupted write.
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp)?;
        Ok(Self { chunks, tmp })
    }

    pub fn path(&self, hash: &[u8; HASH_LEN]) -> PathBuf {
        let hex = hex::encode(hash);
        self.chunks.join(&hex[0..2]).join(&hex[2..4]).join(hex)
    }

    pub fn has(&self, hash: &[u8; HASH_LEN]) -> bool {
        self.path(hash).is_file()
    }

    /// Stores a chunk that was already verified against `hash`. Returns false
    /// when it was already present.
    pub fn put(&self, hash: &[u8; HASH_LEN], data: &[u8]) -> Result<bool> {
        let dest = self.path(hash);
        if dest.is_file() {
            return Ok(false);
        }
        let tmp = self.tmp.join(format!("{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
        {
            let mut f = fs::File::create(&tmp).context("create temp chunk")?;
            f.write_all(data)?;
            f.sync_all()?;
        }
        fs::create_dir_all(dest.parent().unwrap())?;
        if let Err(e) = fs::rename(&tmp, &dest) {
            let _ = fs::remove_file(&tmp);
            if !dest.is_file() {
                return Err(e).context("move chunk into place");
            }
        }
        Ok(true)
    }

    /// Every stored chunk as (hash, path, modified time in unix seconds). Files that are not chunks are ignored.
    pub fn all(&self) -> Vec<([u8; HASH_LEN], PathBuf, i64)> {
        // chunks/ab/cd/<hash>: two levels of directories, then the files.
        entries(&self.chunks)
            .flat_map(|a| entries(&a.path()))
            .flat_map(|b| entries(&b.path()))
            .filter_map(|file| {
                let mut hash = [0u8; HASH_LEN];
                hex::decode_to_slice(file.file_name().to_string_lossy().as_bytes(), &mut hash).ok()?;
                let mtime = file.metadata().and_then(|m| m.modified()).ok()?.duration_since(UNIX_EPOCH).ok()?;
                Some((hash, file.path(), mtime.as_secs() as i64))
            })
            .collect()
    }
}

/// The readable entries of a directory (none if it cannot be read).
fn entries(dir: &Path) -> impl Iterator<Item = fs::DirEntry> {
    fs::read_dir(dir).into_iter().flatten().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let h = fafnirx_common::chunk_hash(b"hello");
        assert!(!store.has(&h));
        assert!(store.put(&h, b"hello").unwrap());
        assert!(store.has(&h));
        assert!(!store.put(&h, b"hello").unwrap());
        assert_eq!(store.all().len(), 1);
    }
}
