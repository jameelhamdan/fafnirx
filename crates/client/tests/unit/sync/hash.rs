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
