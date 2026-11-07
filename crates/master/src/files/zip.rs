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
        .and_then(|t| DateTime::from_date_and_time(t.year().try_into().ok()?, t.month().into(), t.day(), t.hour(), t.minute(), t.second()).ok())
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

fn add_all(zip: &mut ZipWriter<StreamWriter<Gate<SyncIoBridge<impl AsyncWrite + Unpin>>>>, entries: Vec<Entry>) -> io::Result<()> {
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
pub async fn write<W: AsyncWrite + Unpin + Send + 'static>(out: W, entries: Vec<Entry>, failed: Arc<AtomicBool>) -> io::Result<()> {
    let out = SyncIoBridge::new(out);
    tokio::task::spawn_blocking(move || write_all(out, entries, &failed))
        .await
        .map_err(io::Error::other)?
}

#[cfg(test)]
#[path = "../../tests/unit/files/zip.rs"]
mod tests;
