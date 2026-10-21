//! Shared by the client and the server: generated gRPC types, chunking,
//! hashing and path rules.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

#[allow(clippy::result_large_err)]
pub mod pb {
    tonic::include_proto!("fafnirx.v1");
}

/// Current time in unix seconds.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Fixed chunk size.
pub const CHUNK_SIZE: usize = 4 * 1024 * 1024;

/// Upper bound for one gRPC message (a 4 MiB chunk plus framing).
pub const MAX_MESSAGE: usize = 8 * 1024 * 1024;

pub const HASH_LEN: usize = 32;

/// root_hash = blake3(chunk_hash_0 || ... || chunk_hash_n || size as u64 LE)
pub fn root_hash(chunk_hashes: &[[u8; HASH_LEN]], size: u64) -> [u8; HASH_LEN] {
    let mut h = blake3::Hasher::new();
    for c in chunk_hashes {
        h.update(c);
    }
    h.update(&size.to_le_bytes());
    *h.finalize().as_bytes()
}

pub fn chunk_hash(data: &[u8]) -> [u8; HASH_LEN] {
    *blake3::hash(data).as_bytes()
}

/// The chunk hashes stored back to back in a version's `manifest` blob.
pub fn chunks_of(manifest: &[u8]) -> Vec<[u8; HASH_LEN]> {
    manifest.as_chunks::<HASH_LEN>().0.to_vec()
}

pub fn to_hash(bytes: &[u8]) -> Result<[u8; HASH_LEN]> {
    match bytes.try_into() {
        Ok(h) => Ok(h),
        Err(_) => bail!("hash must be {HASH_LEN} bytes, got {}", bytes.len()),
    }
}

/// The message an update signature covers: binds the platform, version and exact bytes of a release.
pub fn update_message(platform: &str, version: &str, sha256_hex: &str) -> String {
    format!(
        "fafnirx-update
{platform}
{version}
{sha256_hex}"
    )
}

/// The path without the `\?\` prefix that Windows puts on canonicalized paths.
pub fn plain_path(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().trim_start_matches(r"\?\"))
}

/// (total, available) bytes of the disk that holds `path`.
pub fn disk_usage(path: &Path) -> Option<(u64, u64)> {
    // Windows paths are case-insensitive.
    let norm =
        |p: &Path| if cfg!(windows) { PathBuf::from(p.to_string_lossy().to_lowercase()) } else { p.to_path_buf() };
    let abs = norm(&plain_path(&std::fs::canonicalize(path).ok()?));
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| abs.starts_with(norm(d.mount_point())))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| (d.total_space(), d.available_space()))
}

/// Expected number of chunks for a file of `size` bytes.
pub fn chunk_count(size: u64) -> u64 {
    size.div_ceil(CHUNK_SIZE as u64)
}

/// Validates a remote path and returns it NFC normalized. Remote paths are
/// relative, '/' separated, and never contain '.', '..', empty or control
/// components. They are only ever stored as database values, never used as
/// server-side filesystem paths.
pub fn clean_remote_path(path: &str) -> Result<String> {
    let path: String = path.nfc().collect();
    if path.is_empty() || path.len() > 4096 {
        bail!("invalid path length");
    }
    if path.starts_with('/') || path.contains('\\') {
        bail!("path must be relative and use '/'");
    }
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.chars().any(|c| c.is_control()) {
            bail!("invalid path component {part:?}");
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_hash_depends_on_size_and_chunks() {
        let a = chunk_hash(b"a");
        let b = chunk_hash(b"b");
        assert_ne!(root_hash(&[a, b], 2), root_hash(&[b, a], 2));
        assert_ne!(root_hash(&[a], 1), root_hash(&[a], 2));
    }

    #[test]
    fn chunk_counts() {
        assert_eq!(chunk_count(0), 0);
        assert_eq!(chunk_count(1), 1);
        assert_eq!(chunk_count(CHUNK_SIZE as u64), 1);
        assert_eq!(chunk_count(CHUNK_SIZE as u64 + 1), 2);
    }

    #[test]
    fn remote_paths() {
        assert_eq!(clean_remote_path("docs/a.txt").unwrap(), "docs/a.txt");
        assert_eq!(clean_remote_path("e\u{301}.txt").unwrap(), "\u{e9}.txt");
        for bad in ["", "/etc/passwd", "a/../b", "a//b", "a\\b", "./a", "a/\u{0}b"] {
            assert!(clean_remote_path(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
