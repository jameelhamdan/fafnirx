use anyhow::{bail, Result};
use fafnirx_common::{hash_path, mtime_ns, HASH_LEN};
use std::path::Path;

#[derive(Clone)]
pub struct Hashed {
    pub size: u64,
    pub mtime_ns: i64,
    pub chunks: Vec<[u8; HASH_LEN]>,
    pub root: [u8; HASH_LEN],
}

pub fn stat(path: &Path) -> std::io::Result<(u64, i64)> {
    let m = std::fs::metadata(path)?;
    Ok((m.len(), mtime_ns(&m)))
}

/// Reads the file once. Fails if the file changes while it is being read.
pub fn hash_file(path: &Path) -> Result<Hashed> {
    let (size0, mtime0) = stat(path)?;
    let hashed = hash_path(path)?;
    let (size1, mtime1) = stat(path)?;
    if (size0, mtime0) != (size1, mtime1) || hashed.size != size1 {
        bail!("file changed while it was being read");
    }
    Ok(Hashed { size: hashed.size, mtime_ns: mtime1, chunks: hashed.chunks, root: hashed.root })
}

#[cfg(test)]
#[path = "../../tests/unit/sync/hash.rs"]
mod tests;
