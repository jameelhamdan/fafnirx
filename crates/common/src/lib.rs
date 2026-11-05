//! Shared by the client and the server: generated gRPC types, chunking,
//! hashing and path rules.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub mod meta;

#[allow(clippy::result_large_err)]
pub mod pb {
    tonic::include_proto!("fafnirx.v1");
}

/// Current time in unix seconds.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Whether `name` can name a device (and so its folder): 1-63 characters, lowercase letters, digits,
/// `.`, `_` or `-`, starting with a letter or digit, and not a name Windows reserves. The database
/// enforces the same rule.
pub fn valid_device_name(name: &str) -> bool {
    let mut chars = name.chars();
    let shape = name.len() <= 63 && chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    let stem = name.split('.').next().unwrap_or(name);
    let reserved = matches!(stem, "con" | "prn" | "aux" | "nul") || (stem.len() == 4 && (stem.starts_with("com") || stem.starts_with("lpt")) && stem.ends_with(|c: char| c.is_ascii_digit()));
    shape && !reserved
}

/// Modification time of a file in unix nanoseconds (0 when unknown).
pub fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64)
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

    /// The hashes of everything fed so far.
    pub fn finish(mut self) -> Hashed {
        if self.filled > 0 {
            self.chunks.push(*self.hasher.finalize().as_bytes());
        }
        Hashed { root: root_hash(&self.chunks, self.size), size: self.size, chunks: self.chunks }
    }
}

/// What hashing a content gives: the hash of each chunk, the root hash over them, and the size.
#[derive(Debug, Clone, PartialEq)]
pub struct Hashed {
    pub root: [u8; HASH_LEN],
    pub size: u64,
    pub chunks: Vec<[u8; HASH_LEN]>,
}

/// Hashes everything `reader` gives, the way a file is hashed.
pub fn hash_reader(mut reader: impl std::io::Read) -> std::io::Result<Hashed> {
    let mut hasher = ChunkHasher::default();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        match reader.read(&mut buf)? {
            0 => return Ok(hasher.finish()),
            n => hasher.update(&buf[..n]),
        }
    }
}

/// Hashes a file.
pub fn hash_path(path: &Path) -> std::io::Result<Hashed> {
    hash_reader(std::fs::File::open(path)?)
}

/// Whether the file holds the content with this root hash (a file that cannot be read does not).
pub fn file_has_root(path: &Path, root: &[u8; HASH_LEN]) -> bool {
    hash_path(path).is_ok_and(|h| h.root == *root)
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

/// Paths that vanished, waiting to be paired with new paths that have the same content: a rename or a move. Equal content is
/// interchangeable, so the earliest vanished path is paired first. Whoever finds renames (the client, the mirror) pairs them
/// this way, so that they agree.
pub struct Vanished<K, T> {
    by_key: std::collections::HashMap<K, std::collections::VecDeque<T>>,
}

impl<K: Eq + std::hash::Hash, T> Vanished<K, T> {
    /// `items` are the vanished paths with their content key, in the order they are to be paired.
    pub fn new(items: impl IntoIterator<Item = (K, T)>) -> Self {
        let mut by_key: std::collections::HashMap<K, std::collections::VecDeque<T>> = std::collections::HashMap::new();
        for (key, item) in items {
            by_key.entry(key).or_default().push_back(item);
        }
        Self { by_key }
    }

    /// The vanished path that a new path with this content is a rename of, if there is one left.
    pub fn take(&mut self, key: &K) -> Option<T> {
        self.by_key.get_mut(key).and_then(std::collections::VecDeque::pop_front)
    }

    /// The vanished paths nothing was paired with (in no particular order).
    pub fn into_remaining(self) -> Vec<T> {
        self.by_key.into_values().flatten().collect()
    }
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

/// What decides whether two paths are the same name: composed and lowercase. Some file systems (Windows, macOS) cannot hold
/// two paths that differ only in case, so a shared folder, which every kind of computer must be able to hold, treats them as one
/// name on all of them.
pub fn fold_path(path: &str) -> String {
    nfc(path).to_lowercase()
}

/// Why a path cannot be kept by every kind of computer, if it cannot: a character Windows does not allow in a name, a name
/// that ends in a space or a dot, or a name Windows reserves. Shared folders refuse such paths everywhere, so that a file does
/// not exist on one computer and fail to arrive on another.
pub fn not_portable(path: &str) -> Option<String> {
    for part in path.split('/') {
        if let Some(c) = part.chars().find(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\')) {
            return Some(format!("{part:?} contains {c:?}, which Windows does not allow in a name"));
        }
        if part.ends_with([' ', '.']) {
            return Some(format!("{part:?} ends with a space or a dot, which Windows does not allow"));
        }
        let stem = part.split('.').next().unwrap_or(part).to_ascii_lowercase();
        let device = matches!(stem.as_str(), "con" | "prn" | "aux" | "nul") || (stem.len() == 4 && (stem.starts_with("com") || stem.starts_with("lpt")) && stem.ends_with(|c: char| c.is_ascii_digit()));
        if device {
            return Some(format!("{part:?} is a name Windows reserves"));
        }
    }
    None
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

/// Writes a file whole or not at all: the bytes go to a temporary file beside it, which is renamed over it, so a reader
/// (another process, or the same one a moment later) never sees half of it. The temporary file starts with a dot and is
/// removed again when the write fails.
pub fn write_atomic(path: &std::path::Path, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_file_name(format!(".{name}.{}-{n}.tmp", std::process::id()));
    let written = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// Locking that goes on after a thread panicked while holding the lock: what it left is still good for the status lines,
/// queues and caches this is used for, and skipping the update (or panicking too) would be worse.
pub trait LockExt<T> {
    fn locked(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockExt<T> for std::sync::Mutex<T> {
    fn locked(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_written_whole_over_the_old_one_and_leaves_nothing_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("status.json");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "two");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary file is left");
        // A folder in the way cannot be replaced: the error is reported and the temporary file is removed.
        let blocked = dir.path().join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        assert!(write_atomic(&blocked, b"x").is_err());
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            2,
            "{:?}",
            std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_lock_whose_holder_panicked_is_still_taken() {
        let lock = std::sync::Arc::new(std::sync::Mutex::new(1));
        let l = lock.clone();
        let _ = std::thread::spawn(move || {
            let _held = l.lock().unwrap();
            panic!("while holding the lock");
        })
        .join();
        assert!(lock.lock().is_err(), "the lock is poisoned");
        *lock.locked() += 1;
        assert_eq!(*lock.locked(), 2);
    }

    /// A device can send data and say what it deleted. What it can read is the client update and the shared folders of its
    /// user; there is no call that reads or lists the files of a device or the master files. A new call in the contract has
    /// to be added here on purpose.
    #[test]
    fn the_device_protocol_reads_nothing_but_updates_and_shared_folders() {
        let proto = include_str!("../../../proto/fafnirx.proto");
        let rpcs: Vec<&str> = proto
            .lines()
            .filter_map(|l| l.trim().strip_prefix("rpc "))
            .map(|l| l.split('(').next().unwrap())
            .collect();
        assert_eq!(
            rpcs,
            [
                "Heartbeat",
                "Identify",
                "Negotiate",
                "UploadChunks",
                "Commit",
                "MarkDeleted",
                "ListFolders",
                "CreateFolder",
                "ListChanges",
                "DownloadFolder",
                "WatchFolder"
            ]
        );
        // The only streams from the master to a device carry the shared folders.
        let returning_streams: Vec<&str> = proto
            .lines()
            .filter(|l| l.contains("returns (stream"))
            .map(|l| l.trim().strip_prefix("rpc ").unwrap().split('(').next().unwrap())
            .collect();
        assert_eq!(returning_streams, ["ListChanges", "DownloadFolder", "WatchFolder"]);
    }

    #[test]
    fn equal_content_is_interchangeable_and_the_earliest_vanished_path_is_paired_first() {
        let mut gone = Vanished::new([("a", "old/1"), ("b", "old/2"), ("a", "old/3")]);
        assert_eq!(gone.take(&"a"), Some("old/1"));
        assert_eq!(gone.take(&"a"), Some("old/3"));
        assert_eq!(gone.take(&"a"), None, "each is paired once");
        assert_eq!(gone.take(&"c"), None);
        assert_eq!(gone.into_remaining(), ["old/2"]);
    }

    #[test]
    fn paths_are_one_name_when_they_differ_only_in_case_or_unicode_form() {
        assert_eq!(fold_path("Docs/README.md"), fold_path("docs/readme.MD"));
        assert_eq!(fold_path("caf\u{e9}"), fold_path("cafe\u{301}"), "an accent as one character or two");
        assert_ne!(fold_path("a/b"), fold_path("a/c"));
    }

    #[test]
    fn names_windows_cannot_keep_are_not_portable() {
        for ok in ["a/b.txt", "Report 2026.pdf", "notes.tar.gz", "console.txt", "com.txt", "lpt10.txt", "caf\u{e9}"] {
            assert!(not_portable(ok).is_none(), "{ok}");
        }
        for bad in [
            "a:b.txt",
            "what?.txt",
            "a/b*c",
            "x<y",
            "pipe|d",
            "q\"uote",
            "trailing.",
            "trailing ",
            "dir./f",
            "CON",
            "con.txt",
            "a/AUX.tar.gz",
            "Com1",
            "lpt9.log",
            "nul",
        ] {
            assert!(not_portable(bad).is_some(), "{bad}");
        }
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
        let done = h.finish();
        assert_eq!(
            (done.root, done.size, done.chunks),
            (root_hash(&chunks, data.len() as u64), data.len() as u64, chunks)
        );
        let empty = ChunkHasher::default().finish();
        assert_eq!((empty.root, empty.size, empty.chunks.len()), (root_hash(&[], 0), 0, 0));
        assert!(hash_reader(&b"abc"[..]).unwrap().root == root_hash(&[chunk_hash(b"abc")], 3));
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

/// A folder key as it goes in a request header, which carries ASCII only: anything else is written as `%XX`.
pub fn header_encode(key: &str) -> String {
    let mut out = String::new();
    for b in key.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/' | b':') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The key [`header_encode`] made of `encoded`.
pub fn header_decode(encoded: &str) -> String {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%')
            .then(|| encoded.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(b) => {
                out.push(b);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod header_tests {
    use super::*;

    #[test]
    fn a_key_survives_the_header() {
        for key in ["family", "bob/work", "master:Música/Rock & Roll", "master:100%"] {
            let sent = header_encode(key);
            assert!(sent.is_ascii());
            assert_eq!(header_decode(&sent), key);
        }
    }
}
