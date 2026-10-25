//! Shared by the client and the server: generated gRPC types, chunking,
//! hashing and path rules.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub mod probe;

#[allow(clippy::result_large_err)]
pub mod pb {
    tonic::include_proto!("fafnirx.v1");
}

/// Current time in unix seconds.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Whether `name` can name a device (and so its folder): 1-63 characters, lowercase letters, digits,
/// `.`, `_` or `-`, starting with a letter or digit, and not a name Windows reserves. The database
/// enforces the same rule.
pub fn valid_device_name(name: &str) -> bool {
    let mut chars = name.chars();
    let shape = name.len() <= 63
        && chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    let stem = name.split('.').next().unwrap_or(name);
    let reserved = matches!(stem, "con" | "prn" | "aux" | "nul")
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.ends_with(|c: char| c.is_ascii_digit()));
    shape && !reserved
}

/// Modification time of a file in unix nanoseconds (0 when unknown).
pub fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as i64)
}

/// Permission bits of a file (0 where the platform has none).
pub fn mode_of(meta: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o7777
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0
    }
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

/// Hashes a stream of bytes the way a file is hashed: per chunk, then the root.
pub struct ChunkHasher {
    hasher: blake3::Hasher,
    filled: usize,
    size: u64,
    chunks: Vec<[u8; HASH_LEN]>,
}

impl Default for ChunkHasher {
    fn default() -> Self {
        Self { hasher: blake3::Hasher::new(), filled: 0, size: 0, chunks: Vec::new() }
    }
}

impl ChunkHasher {
    pub fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let n = (CHUNK_SIZE - self.filled).min(data.len());
            self.hasher.update(&data[..n]);
            self.filled += n;
            self.size += n as u64;
            data = &data[n..];
            if self.filled == CHUNK_SIZE {
                self.chunks.push(*self.hasher.finalize().as_bytes());
                self.hasher = blake3::Hasher::new();
                self.filled = 0;
            }
        }
    }

    /// (root hash, size) of everything fed so far.
    pub fn finish(mut self) -> ([u8; HASH_LEN], u64) {
        if self.filled > 0 {
            self.chunks.push(*self.hasher.finalize().as_bytes());
        }
        (root_hash(&self.chunks, self.size), self.size)
    }
}

pub fn to_hash(bytes: &[u8]) -> Result<[u8; HASH_LEN]> {
    match bytes.try_into() {
        Ok(h) => Ok(h),
        Err(_) => bail!("hash must be {HASH_LEN} bytes, got {}", bytes.len()),
    }
}

/// The message an update signature covers: binds the platform, version and exact bytes of a release.
pub fn update_message(platform: &str, version: &str, sha256_hex: &str) -> String {
    format!("fafnirx-update\n{platform}\n{version}\n{sha256_hex}")
}

/// The path without the `\\?\` prefix that Windows puts on canonicalized paths.
pub fn plain_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

/// (total, available) bytes of the disk that holds `path`.
pub fn disk_usage(path: &Path) -> Option<(u64, u64)> {
    Some((fs4::total_space(path).ok()?, fs4::available_space(path).ok()?))
}

/// Expected number of chunks for a file of `size` bytes.
pub fn chunk_count(size: u64) -> u64 {
    size.div_ceil(CHUNK_SIZE as u64)
}

/// The composed (NFC) form of a name, which is how every path is stored.
pub fn nfc(name: &str) -> String {
    name.nfc().collect()
}

/// Seconds in a day.
pub const DAY_SECS: i64 = 86_400;

/// A unix time in nanoseconds (as `mtime_ns` gives) in seconds.
pub fn secs_of_ns(ns: i64) -> i64 {
    ns / 1_000_000_000
}

/// The last component of a remote path: the name of the file or folder.
pub fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Resolves when the process is asked to stop: Ctrl-C, or SIGTERM, which is what `docker stop` and systemd send. A process
/// that does not handle SIGTERM is not stopped by it when it is the first process of a container.
#[cfg(feature = "server")]
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

/// Validates a remote path and returns it NFC normalized. Remote paths are
/// relative, '/' separated, and never contain '.', '..', empty or control
/// components. The master checks them again when it maps them to a place on disk.
pub fn clean_remote_path(path: &str) -> Result<String> {
    let path = nfc(path);
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
        // Longer than a file name can be on the filesystems the master runs on.
        if part.len() > 255 {
            bail!("a name in the path is longer than 255 bytes");
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one-way rule: a device can put data on the master and say what it deleted, and the master can answer with
    /// nothing but its identity, the signed client update and a name. There is no call that reads or lists the master's
    /// files, so a new call in the contract has to be added here on purpose.
    #[test]
    fn the_device_protocol_only_lets_devices_send() {
        let proto = include_str!("../../../proto/fafnirx.proto");
        let rpcs: Vec<&str> =
            proto.lines().filter_map(|l| l.trim().strip_prefix("rpc ")).map(|l| l.split('(').next().unwrap()).collect();
        assert_eq!(
            rpcs,
            [
                "Heartbeat",
                "Identify",
                "Negotiate",
                "UploadChunks",
                "Commit",
                "MarkDeleted",
                "GetUpdateInfo",
                "DownloadUpdate"
            ]
        );
        // The only streams from the master to a device carry the client update.
        let returning_streams: Vec<&str> = proto
            .lines()
            .filter(|l| l.contains("returns (stream"))
            .map(|l| l.trim().strip_prefix("rpc ").unwrap().split('(').next().unwrap())
            .collect();
        assert_eq!(returning_streams, ["DownloadUpdate"]);
    }

    #[test]
    fn root_hash_depends_on_size_and_chunks() {
        let a = chunk_hash(b"a");
        let b = chunk_hash(b"b");
        assert_ne!(root_hash(&[a, b], 2), root_hash(&[b, a], 2));
        assert_ne!(root_hash(&[a], 1), root_hash(&[a], 2));
    }

    #[test]
    fn chunk_hasher_matches_chunk_by_chunk_hashing() {
        let data: Vec<u8> = (0..CHUNK_SIZE * 2 + 10).map(|i| (i % 251) as u8).collect();
        let chunks: Vec<_> = data.chunks(CHUNK_SIZE).map(chunk_hash).collect();
        let mut h = ChunkHasher::default();
        for piece in data.chunks(1_000_003) {
            h.update(piece);
        }
        assert_eq!(h.finish(), (root_hash(&chunks, data.len() as u64), data.len() as u64));
        assert_eq!(ChunkHasher::default().finish(), (root_hash(&[], 0), 0));
    }

    #[test]
    fn a_name_longer_than_a_filesystem_allows_is_refused() {
        assert!(clean_remote_path(&format!("a/{}", "x".repeat(255))).is_ok());
        assert!(clean_remote_path(&format!("a/{}", "x".repeat(256))).is_err());
    }

    #[test]
    fn device_names_are_slugs() {
        for ok in ["laptop", "pc-2", "a", "my.phone_1", "9lives", "console"] {
            assert!(valid_device_name(ok), "{ok}");
        }
        for bad in ["", "-a", ".a", "A", "a b", "a/b", "a:b", "con", "nul.txt", "com1", "lpt9", &"a".repeat(64), "é"] {
            assert!(!valid_device_name(bad), "{bad}");
        }
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
