//! A streaming ZIP writer: files are read once and sent as they are, with no temporary archive and no
//! memory use that grows with the size. Entries are stored, not compressed (photos and videos are
//! already compressed), and use ZIP64 so that files and archives can be any size.

use std::io;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub struct Entry {
    /// Name inside the archive, with `/` separators.
    pub name: String,
    pub path: PathBuf,
    /// Modification time, unix seconds.
    pub mtime: i64,
}

struct Done {
    name: String,
    crc: u32,
    size: u64,
    offset: u64,
    time: u16,
    date: u16,
}

/// MS-DOS (time, date) of a unix time; the format cannot say anything before 1980.
fn dos_time(unix: i64) -> (u16, u16) {
    let unix = unix.max(315_532_800);
    let (days, secs) = (unix.div_euclid(86400), unix.rem_euclid(86400));
    // civil_from_days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let time = ((secs / 3600) << 11) | (((secs % 3600) / 60) << 5) | ((secs % 60) / 2);
    let date = ((year - 1980) << 9) | (month << 5) | day;
    (time as u16, date as u16)
}

const GENERAL_FLAGS: u16 = 0x0808; // sizes follow the data; names are UTF-8
const VERSION: u16 = 45; // ZIP64

fn local_header(name: &str, time: u16, date: u16) -> Vec<u8> {
    let mut h = Vec::with_capacity(30 + name.len() + 20);
    h.extend(0x0403_4b50u32.to_le_bytes());
    h.extend(VERSION.to_le_bytes());
    h.extend(GENERAL_FLAGS.to_le_bytes());
    h.extend(0u16.to_le_bytes()); // stored
    h.extend(time.to_le_bytes());
    h.extend(date.to_le_bytes());
    h.extend([0u8; 12]); // crc and sizes: in the descriptor
    h.extend((name.len() as u16).to_le_bytes());
    h.extend(20u16.to_le_bytes());
    h.extend(name.as_bytes());
    // The ZIP64 extra field tells readers that the descriptor has 8-byte sizes.
    h.extend(1u16.to_le_bytes());
    h.extend(16u16.to_le_bytes());
    h.extend([0u8; 16]);
    h
}

fn descriptor(crc: u32, size: u64) -> Vec<u8> {
    let mut d = Vec::with_capacity(24);
    d.extend(0x0807_4b50u32.to_le_bytes());
    d.extend(crc.to_le_bytes());
    d.extend(size.to_le_bytes());
    d.extend(size.to_le_bytes());
    d
}

fn central_entry(e: &Done) -> Vec<u8> {
    let mut h = Vec::with_capacity(46 + e.name.len() + 28);
    h.extend(0x0201_4b50u32.to_le_bytes());
    h.extend(VERSION.to_le_bytes()); // made by
    h.extend(VERSION.to_le_bytes()); // needed
    h.extend(GENERAL_FLAGS.to_le_bytes());
    h.extend(0u16.to_le_bytes());
    h.extend(e.time.to_le_bytes());
    h.extend(e.date.to_le_bytes());
    h.extend(e.crc.to_le_bytes());
    h.extend(u32::MAX.to_le_bytes());
    h.extend(u32::MAX.to_le_bytes());
    h.extend((e.name.len() as u16).to_le_bytes());
    h.extend(28u16.to_le_bytes());
    h.extend([0u8; 10]); // comment length, disk, internal and external attributes
    h.extend(u32::MAX.to_le_bytes());
    h.extend(e.name.as_bytes());
    h.extend(1u16.to_le_bytes());
    h.extend(24u16.to_le_bytes());
    h.extend(e.size.to_le_bytes());
    h.extend(e.size.to_le_bytes());
    h.extend(e.offset.to_le_bytes());
    h
}

fn end_of_archive(entries: u64, dir_offset: u64, dir_size: u64) -> Vec<u8> {
    let mut t = Vec::with_capacity(98);
    t.extend(0x0606_4b50u32.to_le_bytes());
    t.extend(44u64.to_le_bytes());
    t.extend(VERSION.to_le_bytes());
    t.extend(VERSION.to_le_bytes());
    t.extend([0u8; 8]); // disk numbers
    t.extend(entries.to_le_bytes());
    t.extend(entries.to_le_bytes());
    t.extend(dir_size.to_le_bytes());
    t.extend(dir_offset.to_le_bytes());
    t.extend(0x0706_4b50u32.to_le_bytes());
    t.extend(0u32.to_le_bytes());
    t.extend((dir_offset + dir_size).to_le_bytes());
    t.extend(1u32.to_le_bytes());
    t.extend(0x0605_4b50u32.to_le_bytes());
    t.extend([0u8; 4]);
    t.extend(u16::MAX.to_le_bytes());
    t.extend(u16::MAX.to_le_bytes());
    t.extend(u32::MAX.to_le_bytes());
    t.extend(u32::MAX.to_le_bytes());
    t.extend(0u16.to_le_bytes());
    t
}

/// Writes the entries as one archive. A file that cannot be read stops the archive with an error, so
/// the download fails visibly instead of being quietly incomplete.
pub async fn write<W: AsyncWrite + Unpin>(out: &mut W, entries: Vec<Entry>) -> io::Result<()> {
    let mut offset = 0u64;
    let mut done = Vec::with_capacity(entries.len());
    let mut buf = vec![0u8; 256 * 1024];
    for e in entries {
        let (time, date) = dos_time(e.mtime);
        let header = local_header(&e.name, time, date);
        out.write_all(&header).await?;
        let entry_offset = offset;
        offset += header.len() as u64;
        let mut file = tokio::fs::File::open(&e.path).await?;
        let (mut crc, mut size) = (crc32fast::Hasher::new(), 0u64);
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            crc.update(&buf[..n]);
            out.write_all(&buf[..n]).await?;
            size += n as u64;
        }
        let crc = crc.finalize();
        out.write_all(&descriptor(crc, size)).await?;
        offset += size + 24;
        done.push(Done { name: e.name, crc, size, offset: entry_offset, time, date });
    }
    let dir_offset = offset;
    for e in &done {
        let entry = central_entry(e);
        out.write_all(&entry).await?;
        offset += entry.len() as u64;
    }
    out.write_all(&end_of_archive(done.len() as u64, dir_offset, offset - dir_offset)).await?;
    out.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn dos_times() {
        assert_eq!(dos_time(1_700_000_000), (0xB1AA, 0x576E), "2023-11-14 22:13:20");
        assert_eq!(dos_time(0), dos_time(315_532_800), "clamped to 1980");
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
        let mut bytes = Vec::new();
        write(&mut bytes, vec![entry("x/a.txt", "a"), entry("x/ü b.bin", "b"), entry("empty", "empty")]).await.unwrap();

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 3);
        let mut read = |i: usize| {
            let mut f = archive.by_index(i).unwrap();
            let mut data = Vec::new();
            f.read_to_end(&mut data).unwrap();
            (f.name().to_string(), data)
        };
        assert_eq!(read(0), ("x/a.txt".to_string(), b"hello".to_vec()));
        assert_eq!(read(1), ("x/ü b.bin".to_string(), big));
        assert_eq!(read(2), ("empty".to_string(), vec![]));
    }
}
