use crate::scan::mtime_ns;
use anyhow::{bail, Result};
use fafnirx_common::{chunk_hash, root_hash, CHUNK_SIZE, HASH_LEN};
use std::io::Read;
use std::path::Path;

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

/// Reads the file once, in 4 MiB chunks. Fails if the file changes while it
/// is being read.
pub fn hash_file(path: &Path) -> Result<Hashed> {
    let (size0, mtime0) = stat(path)?;
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut chunks = Vec::new();
    let mut total = 0u64;
    loop {
        let mut filled = 0;
        while filled < CHUNK_SIZE {
            let n = file.read(&mut buf[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break;
        }
        chunks.push(chunk_hash(&buf[..filled]));
        total += filled as u64;
        if filled < CHUNK_SIZE {
            break;
        }
    }
    let (size1, mtime1) = stat(path)?;
    if (size0, mtime0) != (size1, mtime1) || total != size1 {
        bail!("file changed while it was being read");
    }
    let root = root_hash(&chunks, total);
    Ok(Hashed { size: total, mtime_ns: mtime1, chunks, root })
}

#[cfg(test)]
mod tests {
    use super::*;

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
