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
mod tests {
    use super::*;
    use fafnirx_common::{chunk_hash, root_hash, CHUNK_SIZE};

    #[test]
    fn hashes_match_chunk_layout() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f");
        let data: Vec<u8> = (0..(CHUNK_SIZE + 10)).map(|i| (i % 251) as u8).collect();
        std::fs::write(&p, &data).unwrap();
        let h = hash_file(&p).unwrap();
        assert_eq!(h.size, data.len() as u64);
        assert_eq!(h.chunks.len(), 2);
        assert_eq!(h.chunks[0], chunk_hash(&data[..CHUNK_SIZE]));
        assert_eq!(h.chunks[1], chunk_hash(&data[CHUNK_SIZE..]));
        assert_eq!(h.root, root_hash(&h.chunks, h.size));
    }

    #[test]
    fn empty_file_has_no_chunks() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("empty");
        std::fs::write(&p, b"").unwrap();
        let h = hash_file(&p).unwrap();
        assert!(h.chunks.is_empty());
        assert_eq!(h.root, root_hash(&[], 0));
    }
}
