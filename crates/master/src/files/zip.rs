//! A streaming ZIP writer: files are read once and sent as they are, with no temporary archive and no memory use that grows
//! with the size. Entries are stored, not compressed (photos and videos are already compressed), and use ZIP64 so that files
//! and archives can be any size. The `zip` crate writes it, for an output that cannot seek; it blocks, so it runs on a thread
//! of its own, and the thread waits whenever the download is slower than the disk.

use std::io::{self, BufReader};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWrite;
use tokio_util::io::SyncIoBridge;
use zip::write::{SimpleFileOptions, StreamWriter};
use zip::{CompressionMethod, DateTime, ZipWriter};

pub struct Entry {
    /// Name inside the archive, with `/` separators.
    pub name: String,
    pub path: PathBuf,
    /// Modification time, unix seconds.
    pub mtime: i64,
}

/// The time an entry is dated: what the format can say (it starts in 1980), else its first moment.
fn dated(unix: i64) -> DateTime {
    time::OffsetDateTime::from_unix_timestamp(unix)
        .ok()
        .and_then(|t| {
            DateTime::from_date_and_time(
                t.year().try_into().ok()?,
                t.month().into(),
                t.day(),
                t.hour(),
                t.minute(),
                t.second(),
            )
            .ok()
        })
        .unwrap_or_default()
}

/// Passes what is written on until told to throw the rest away. An archive that failed must not be finished into one that
/// looks complete, so what follows the failure never reaches the download, and the writer can still be closed quietly.
struct Gate<W> {
    inner: W,
    discard: Arc<AtomicBool>,
}

impl<W: io::Write> io::Write for Gate<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.discard.load(Ordering::Relaxed) {
            Ok(buf.len())
        } else {
            self.inner.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.discard.load(Ordering::Relaxed) {
            Ok(())
        } else {
            self.inner.flush()
        }
    }
}

fn add_all(
    zip: &mut ZipWriter<StreamWriter<Gate<SyncIoBridge<impl AsyncWrite + Unpin>>>>,
    entries: Vec<Entry>,
) -> io::Result<()> {
    for e in entries {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(true)
            .last_modified_time(dated(e.mtime));
        zip.start_file(e.name, options).map_err(io::Error::other)?;
        let file = std::fs::File::open(&e.path)?;
        io::copy(&mut BufReader::with_capacity(256 * 1024, file), zip)?;
    }
    Ok(())
}

fn write_all(out: SyncIoBridge<impl AsyncWrite + Unpin>, entries: Vec<Entry>, failed: &AtomicBool) -> io::Result<()> {
    let discard = Arc::new(AtomicBool::new(false));
    let mut zip = ZipWriter::new_stream(Gate { inner: out, discard: discard.clone() });
    let added = add_all(&mut zip, entries);
    if added.is_err() {
        // Set before the output is closed, so that whoever reads it sees the failure at its end.
        failed.store(true, Ordering::Relaxed);
        discard.store(true, Ordering::Relaxed);
    }
    let finished = zip.finish().map_err(io::Error::other);
    added?;
    finished?.into_inner().inner.shutdown()
}

/// Writes the entries as one archive and ends `out`. A file that cannot be read stops the archive with an error, leaves
/// what was sent unfinished (no central directory) and sets `failed` before `out` ends, so that the download fails
/// visibly instead of being quietly incomplete.
pub async fn write<W: AsyncWrite + Unpin + Send + 'static>(
    out: W,
    entries: Vec<Entry>,
    failed: Arc<AtomicBool>,
) -> io::Result<()> {
    let out = SyncIoBridge::new(out);
    tokio::task::spawn_blocking(move || write_all(out, entries, &failed)).await.map_err(io::Error::other)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tokio::io::AsyncReadExt;

    /// The archive `write` makes of `entries`, read whole.
    async fn archive_of(entries: Vec<Entry>) -> Vec<u8> {
        let (writer, mut reader) = tokio::io::duplex(64 * 1024);
        let writing = tokio::spawn(write(writer, entries, Arc::default()));
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.unwrap();
        writing.await.unwrap().unwrap();
        bytes
    }

    #[test]
    fn dos_times() {
        let at = |unix| {
            let d = dated(unix);
            (d.year(), d.month(), d.day(), d.hour(), d.minute(), d.second())
        };
        assert_eq!(at(1_700_000_000), (2023, 11, 14, 22, 13, 20));
        assert_eq!(at(0), (1980, 1, 1, 0, 0, 0), "the format starts in 1980");
        assert_eq!(at(i64::MAX), (1980, 1, 1, 0, 0, 0), "a time that means nothing");
    }

    #[tokio::test]
    async fn archives_read_back_with_a_standard_reader() {
        let dir = tempfile::tempdir().unwrap();
        let big: Vec<u8> = (0..700_000u32).map(|i| (i % 253) as u8).collect();
        std::fs::write(dir.path().join("a"), b"hello").unwrap();
        std::fs::write(dir.path().join("b"), &big).unwrap();
        std::fs::write(dir.path().join("empty"), b"").unwrap();
        let entry =
            |name: &str, file: &str| Entry { name: name.into(), path: dir.path().join(file), mtime: 1_700_000_000 };
        let bytes = archive_of(vec![entry("x/a.txt", "a"), entry("x/ü b.bin", "b"), entry("empty", "empty")]).await;

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 3);
        let mut read = |i: usize| {
            let mut f = archive.by_index(i).unwrap();
            assert_eq!(f.compression(), CompressionMethod::Stored);
            let mut data = Vec::new();
            f.read_to_end(&mut data).unwrap();
            (f.name().unwrap().to_string(), data)
        };
        assert_eq!(read(0), ("x/a.txt".to_string(), b"hello".to_vec()));
        assert_eq!(read(1), ("x/ü b.bin".to_string(), big));
        assert_eq!(read(2), ("empty".to_string(), vec![]));
    }

    #[tokio::test]
    async fn a_missing_file_ends_the_archive_with_an_error() {
        let (writer, mut reader) = tokio::io::duplex(1024);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ok"), b"fine").unwrap();
        let entry = |name: &str, path: std::path::PathBuf| Entry { name: name.into(), path, mtime: 0 };
        let entries = vec![entry("ok", dir.path().join("ok")), entry("gone", dir.path().join("gone"))];
        let failed = Arc::new(AtomicBool::new(false));
        let writing = tokio::spawn(write(writer, entries, failed.clone()));
        let mut sent = Vec::new();
        let _ = reader.read_to_end(&mut sent).await;
        assert!(failed.load(Ordering::Relaxed), "the end of the stream says that it failed");
        assert!(writing.await.unwrap().is_err(), "the download must fail, not end quietly");
        assert!(zip::ZipArchive::new(std::io::Cursor::new(sent)).is_err(), "what was sent is not a complete archive");
    }

    #[tokio::test]
    async fn a_download_that_goes_away_stops_the_writer() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f"), vec![7u8; 4 << 20]).unwrap();
        let (writer, reader) = tokio::io::duplex(1024);
        drop(reader);
        let entry = Entry { name: "f".into(), path: dir.path().join("f"), mtime: 0 };
        assert!(
            write(writer, vec![entry], Arc::default()).await.is_err(),
            "nobody reads: the writer gives up instead of waiting"
        );
    }

    /// A file larger than 4 GiB, which needs ZIP64. Slow and takes the disk space of the archive, so it is run by hand:
    /// `cargo test -p fafnirx-master --bins zip -- --ignored`.
    #[tokio::test]
    #[ignore = "writes a 4 GiB archive"]
    async fn a_file_over_4_gib_is_read_back_whole() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("huge");
        let size = (4u64 << 30) + 12_345;
        // Zeros, extended without writing them.
        std::fs::File::create(&source).unwrap().set_len(size).unwrap();
        let out = dir.path().join("huge.zip");
        let sink = tokio::fs::File::create(&out).await.unwrap();
        let small = dir.path().join("small");
        std::fs::write(&small, b"after").unwrap();
        let entry = Entry { name: "huge.bin".into(), path: source, mtime: 1_700_000_000 };
        write(sink, vec![entry, Entry { name: "after.txt".into(), path: small, mtime: 0 }], Arc::default())
            .await
            .unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        let mut file = archive.by_index(0).unwrap();
        assert_eq!(file.size(), size);
        let copied = std::io::copy(&mut file, &mut std::io::sink()).unwrap(); // the checksum is checked at the end
        assert_eq!(copied, size);
        drop(file);
        let mut after = String::new();
        archive.by_index(1).unwrap().read_to_string(&mut after).unwrap();
        assert_eq!(after, "after", "what follows a ZIP64 entry is found");
    }
}
