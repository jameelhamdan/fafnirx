//! What the master knows about the content of a file beyond its bytes: the kind, the size of an image or
//! video, the EXIF data and GPS position of a photo, and previews. Read by the worker role, shown by the
//! admin API.

use crate::library::media;
use crate::platform::db::{self, Db};
use crate::platform::store::Store;
use anyhow::{Context, Result};
use fafnirx_common::unix_now;
use image::ImageDecoder;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Extensions of images that get their metadata read and a preview.
const IMAGE_EXTENSIONS: [&str; 11] =
    ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif", "avif"];
/// Extensions of videos.
const VIDEO_EXTENSIONS: [&str; 11] = ["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "mts", "3gp", "mpg", "mpeg"];
/// Extensions the `image` crate can decode here (HEIC and AVIF have metadata but need ffmpeg for a preview).
const PREVIEW_EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff"];
const MAX_ATTEMPTS: i64 = 3;

/// The two sizes of picture made for every photo and video: a grid cell and a full-screen view.
pub const THUMB: u32 = 320;
pub const PREVIEW: u32 = 1600;

pub fn extension(path: &str) -> String {
    path.rsplit('.').next().filter(|e| *e != path).unwrap_or("").to_ascii_lowercase()
}

/// `image` or `video` for the files that belong in the photo library.
pub fn kind(path: &str) -> Option<&'static str> {
    let ext = extension(path);
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Some("image")
    } else if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        Some("video")
    } else {
        None
    }
}

/// Whether the metadata of this file is worth reading.
pub fn wants(path: &str) -> bool {
    kind(path).is_some()
}

/// Whether a preview picture can be made from this file.
pub fn can_preview(path: &str) -> bool {
    match kind(path) {
        Some("video") => true,
        Some(_) => PREVIEW_EXTENSIONS.contains(&extension(path).as_str()) || media::available(),
        None => false,
    }
}

/// Media type from the file name.
pub fn mime(path: &str) -> String {
    mime_guess::from_path(path).first_or_octet_stream().essence_str().to_string()
}

pub struct Meta {
    pub kind: &'static str,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub taken_at: Option<i64>,
    pub exif: serde_json::Map<String, serde_json::Value>,
    pub duration_ms: Option<i64>,
    pub codec: Option<String>,
    pub position: Option<(f64, f64)>,
}

/// The EXIF fields worth showing, by tag name.
const EXIF_FIELDS: [&str; 17] = [
    "Make",
    "Model",
    "LensModel",
    "DateTimeOriginal",
    "ExposureTime",
    "FNumber",
    "ISOSpeed",
    "PhotographicSensitivity",
    "FocalLength",
    "FocalLengthIn35mmFilm",
    "Flash",
    "Orientation",
    "Software",
    "GPSLatitude",
    "GPSLongitude",
    "GPSAltitude",
    "ExposureBiasValue",
];

/// A latitude and longitude that mean a place: in range, and not the 0,0 that broken cameras write.
pub fn valid_position(lat: f64, lon: f64) -> Option<(f64, f64)> {
    let in_range = lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0;
    (in_range && !(lat == 0.0 && lon == 0.0)).then_some((lat, lon))
}

/// The GPS position of a photo, in degrees (south and west are negative).
fn gps(data: &exif::Exif) -> Option<(f64, f64)> {
    let coordinate = |tag, reference| -> Option<f64> {
        let exif::Value::Rational(parts) = &data.get_field(tag, exif::In::PRIMARY)?.value else { return None };
        let [deg, min, sec, ..] = parts[..] else { return None };
        let value = deg.to_f64() + min.to_f64() / 60.0 + sec.to_f64() / 3600.0;
        let shown = data.get_field(reference, exif::In::PRIMARY)?.display_value().to_string();
        Some(if shown.contains('S') || shown.contains('W') { -value } else { value })
    };
    valid_position(
        coordinate(exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef)?,
        coordinate(exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef)?,
    )
}

/// Reads the size and EXIF data of an image. A file without EXIF just has no fields.
pub fn read(path: &Path) -> Result<Meta> {
    let (width, height) = match image::image_dimensions(path) {
        Ok((w, h)) => (Some(w as i64), Some(h as i64)),
        Err(_) => (None, None),
    };
    let mut meta = Meta {
        kind: "image",
        width,
        height,
        taken_at: None,
        exif: serde_json::Map::new(),
        duration_ms: None,
        codec: None,
        position: None,
    };
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    if let Ok(data) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) {
        for field in data.fields() {
            let name = field.tag.to_string();
            if !EXIF_FIELDS.contains(&name.as_str()) || meta.exif.contains_key(&name) {
                continue;
            }
            let shown = field.display_value().with_unit(&data).to_string();
            if name == "DateTimeOriginal" {
                meta.taken_at = parse_time(&shown);
            }
            meta.exif.insert(name, serde_json::Value::String(shown));
        }
        meta.position = gps(&data);
        let get = |tag| data.get_field(tag, exif::In::PRIMARY).and_then(|f| f.value.get_uint(0));
        if meta.width.is_none() {
            // Some formats (HEIC) only say their size here.
            meta.width = get(exif::Tag::PixelXDimension).map(i64::from);
            meta.height = get(exif::Tag::PixelYDimension).map(i64::from);
        }
        // A photo taken sideways is stored sideways and shown upright: the size people see is the swapped one.
        if get(exif::Tag::Orientation).is_some_and(|o| (5..=8).contains(&o)) {
            std::mem::swap(&mut meta.width, &mut meta.height);
        }
    }
    Ok(meta)
}

/// Reads a video, or just recognises it when ffmpeg is not there.
fn read_video(path: &Path) -> Meta {
    let probe = if media::available() {
        media::probe(path).map_err(|e| tracing::warn!("{}: {e:#}", path.display())).unwrap_or_default()
    } else {
        media::Probe::default()
    };
    Meta {
        kind: "video",
        width: probe.width,
        height: probe.height,
        taken_at: probe.taken_at,
        exif: serde_json::Map::new(),
        duration_ms: probe.duration_ms,
        codec: probe.codec,
        position: probe.position,
    }
}

/// Reads whatever the file is, by its name.
pub fn read_any(name: &str, path: &Path) -> Result<Meta> {
    match kind(name) {
        Some("video") => Ok(read_video(path)),
        _ => read(path),
    }
}

/// "2023-11-14 22:13:20" (as EXIF displays it) or "2023-11-14T22:13:20Z" to unix seconds, taken as UTC:
/// EXIF has no time zone.
pub fn parse_time(text: &str) -> Option<i64> {
    let digits: Vec<i64> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let [y, mo, d, h, mi, s, ..] = digits[..] else { return None };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    // Days since 1970-01-01 (civil-from-days, Howard Hinnant).
    let (y, m) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + s)
}

/// Decodes an image and turns it upright according to its EXIF orientation.
fn decode_upright(src: &Path) -> Result<image::DynamicImage> {
    let mut decoder = image::ImageReader::open(src)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Pictures nobody asked for this long are made again on demand.
const THUMB_KEEP: std::time::Duration = std::time::Duration::from_secs(30 * 86400);
/// A copy that was being written when the master stopped.
const PARTIAL_KEEP: std::time::Duration = std::time::Duration::from_secs(86400);

/// Clears the cache: old pictures (they are made again when someone looks), unfinished files, and playable
/// copies of videos that no stored version has any more.
pub fn clean_cache(db: &Db, cache_root: &Path) -> Result<()> {
    let age = |e: &std::fs::DirEntry| {
        e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).unwrap_or_default()
    };
    for e in std::fs::read_dir(cache_root.join("thumbs")).into_iter().flatten().flatten() {
        let partial = e.file_name().to_string_lossy().contains(".tmp");
        if age(&e) > if partial { PARTIAL_KEEP } else { THUMB_KEEP } {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let mut c = db.get()?;
    for e in std::fs::read_dir(cache_root.join("video")).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let (stem, partial) = (name.split('.').next().unwrap_or("").to_string(), name.contains(".tmp"));
        let used = hex::decode(&stem).is_ok_and(|root| {
            c.query_one("SELECT EXISTS (SELECT 1 FROM versions WHERE root_hash = $1)", &[&root]).is_ok_and(|r| r.get(0))
        });
        if if partial { age(&e) > PARTIAL_KEEP } else { !used } {
            let _ = std::fs::remove_file(e.path());
        }
    }
    Ok(())
}

/// A JPEG preview no larger than `max` on its longest side, cached by content and size. Videos get a frame.
pub fn thumbnail(src: &Path, root_hex: &str, max: u32, cache: &Path) -> Result<PathBuf> {
    let dest = cache.join(format!("{root_hex}-{max}.jpg"));
    if dest.is_file() {
        return Ok(dest);
    }
    std::fs::create_dir_all(cache)?;
    // Unique per call: two requests for the same preview can run at once.
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = cache.join(format!("{root_hex}-{max}.{}-{n}.tmp.jpg", std::process::id()));
    let result = make_thumbnail(src, max, &tmp);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result?;
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

fn make_thumbnail(src: &Path, max: u32, tmp: &Path) -> Result<()> {
    let name = src.to_string_lossy();
    if kind(&name) == Some("video") {
        anyhow::ensure!(media::available(), "ffmpeg is not installed");
        return media::poster(src, max, tmp);
    }
    match decode_upright(src) {
        Ok(img) => {
            // Never enlarge: a small picture stays as it is.
            let small = if img.width() > max || img.height() > max { img.thumbnail(max, max) } else { img }.to_rgb8();
            let mut out = std::io::BufWriter::new(std::fs::File::create(tmp)?);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
                .encode_image(&small)
                .context("encode preview")
        }
        // Formats the image crate cannot read (HEIC, AVIF) are left to ffmpeg when it is there.
        Err(e) if media::available() => {
            media::poster(src, max, tmp).with_context(|| format!("decode {}: {e}", src.display()))
        }
        Err(e) => Err(e).with_context(|| format!("decode {}", src.display())),
    }
}

/// Where the playable copy of a video is (or will be) kept.
pub fn proxy_path(cache_root: &Path, root_hex: &str) -> PathBuf {
    cache_root.join("video").join(format!("{root_hex}.mp4"))
}

/// Runs queued jobs until the process ends. Several workers (in one process or many) can run side by
/// side: a job is claimed with `FOR UPDATE SKIP LOCKED`. One lane skips the slow video conversions, so that
/// a long film never keeps the thumbnails of new photos waiting.
pub fn run_queue(db: Db, store: std::sync::Arc<Store>, cache_root: PathBuf) {
    // The database client blocks, so everything here runs on threads of its own, never on the async runtime.
    std::thread::spawn(move || {
        // A restart interrupts whatever was running.
        if let Ok(mut c) = db.get() {
            let _ = c.execute("UPDATE jobs SET status = 'queued' WHERE status = 'running'", &[]);
        }
        for lane_with_proxies in [true, false] {
            let (db, store, cache_root) = (db.clone(), store.clone(), cache_root.clone());
            std::thread::spawn(move || work(&db, &store, &cache_root, lane_with_proxies));
        }
    });
}

/// One lane of the queue, forever.
fn work(db: &Db, store: &Store, cache_root: &Path, with_proxies: bool) {
    loop {
        match claim(db, with_proxies) {
            Ok(Some((id, kind, payload, attempts))) => {
                let result = match kind.as_str() {
                    "meta" => read_job(db, store, cache_root, &payload),
                    "proxy" => proxy_job(db, store, cache_root, &payload),
                    other => Err(anyhow::anyhow!("unknown job kind {other:?}")),
                };
                finish(db, id, attempts, result);
            }
            Ok(None) => std::thread::sleep(Duration::from_secs(2)),
            Err(e) => {
                tracing::error!("job queue: {e:#}");
                std::thread::sleep(Duration::from_secs(10));
            }
        }
    }
}

type Claimed = (i64, String, String, i64);

fn claim(db: &Db, with_proxies: bool) -> Result<Option<Claimed>> {
    let mut c = db.get()?;
    let row = c.query_opt(
        "UPDATE jobs SET status = 'running', attempts = attempts + 1
         WHERE id = (SELECT id FROM jobs WHERE status = 'queued' AND run_after <= $1 AND ($2 OR kind <> 'proxy')
                     ORDER BY (kind = 'proxy'), id LIMIT 1 FOR UPDATE SKIP LOCKED)
         RETURNING id, kind, payload, attempts",
        &[&unix_now(), &with_proxies],
    )?;
    Ok(row.map(|r| (r.get(0), r.get(1), r.get(2), r.get(3))))
}

fn finish(db: &Db, id: i64, attempts: i64, result: Result<()>) {
    let Ok(mut c) = db.get() else { return };
    let done = match result {
        Ok(()) => c.execute("DELETE FROM jobs WHERE id = $1", &[&id]),
        Err(e) => {
            tracing::warn!("job {id} failed (attempt {attempts}): {e:#}");
            let (status, wait) = if attempts >= MAX_ATTEMPTS { ("failed", 0) } else { ("queued", 30 * attempts) };
            c.execute(
                "UPDATE jobs SET status = $2, run_after = $3, error = $4 WHERE id = $1",
                &[&id, &status, &(unix_now() + wait), &format!("{e:#}")],
            )
        }
    };
    if let Err(e) = done {
        tracing::error!("recording the result of job {id}: {e}");
    }
}

#[derive(serde::Deserialize)]
struct JobFile {
    device: String,
    path: String,
    root: String,
}

/// The job's file, if it still holds the content the job was made for.
fn current_file(c: &mut postgres::Client, p: &JobFile) -> Result<Option<Vec<u8>>> {
    let root = hex::decode(&p.root)?;
    let current: bool = c
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM files f JOIN devices d ON d.id = f.device_id
                            JOIN versions v ON v.id = f.current_version_id
                            WHERE d.name = $1 AND f.path = $2 AND v.root_hash = $3)",
            &[&p.device, &p.path, &root],
        )?
        .get(0);
    Ok(current.then_some(root))
}

/// Reads the metadata of the file a `meta` job names, if it still holds that content, and makes its pictures.
fn read_job(db: &Db, store: &Store, cache_root: &Path, payload: &str) -> Result<()> {
    let p: JobFile = serde_json::from_str(payload)?;
    let mut c = db.get()?;
    let Some(root) = current_file(&mut c, &p)? else {
        return Ok(()); // replaced or deleted since; the newer content has its own job
    };
    let disk = store.live_path(&p.device, &p.path)?;
    let meta = read_any(&p.path, &disk)?;
    let (lat, lon) = meta.position.unzip();
    c.execute(
        "INSERT INTO file_meta (root_hash, mime, width, height, taken_at, exif, read_at, kind, duration_ms, codec, lat, lon)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         ON CONFLICT (root_hash) DO UPDATE SET mime = excluded.mime, width = excluded.width, height = excluded.height,
             taken_at = excluded.taken_at, exif = excluded.exif, read_at = excluded.read_at, kind = excluded.kind,
             duration_ms = excluded.duration_ms, codec = excluded.codec, lat = excluded.lat, lon = excluded.lon",
        &[
            &root,
            &mime(&p.path),
            &meta.width,
            &meta.height,
            &meta.taken_at,
            &serde_json::Value::Object(meta.exif).to_string(),
            &unix_now(),
            &meta.kind,
            &meta.duration_ms,
            &meta.codec,
            &lat,
            &lon,
        ],
    )?;
    db::notify(&mut *c, "files", &p.device)?;
    drop(c);

    // Pictures are made now so that opening the timeline finds them ready. A file that cannot be
    // decoded is not worth retrying: it is shown with an icon instead.
    let thumbs = cache_root.join("thumbs");
    let root_hex = hex::encode(&root);
    for size in [THUMB, PREVIEW] {
        if let Err(e) = thumbnail(&disk, &root_hex, size, &thumbs) {
            tracing::warn!("no {size}px preview for {}/{}: {e:#}", p.device, p.path);
            break;
        }
    }
    if meta.kind == "video"
        && media::available()
        && !media::plays_everywhere(&extension(&p.path), meta.codec.as_deref())
        && !proxy_path(cache_root, &root_hex).is_file()
    {
        db::enqueue(
            &mut *db.get()?,
            "proxy",
            &serde_json::json!({ "device": p.device, "path": p.path, "root": p.root }),
        )?;
    }
    Ok(())
}

/// Makes the playable copy of a video the `proxy` job names.
fn proxy_job(db: &Db, store: &Store, cache_root: &Path, payload: &str) -> Result<()> {
    let p: JobFile = serde_json::from_str(payload)?;
    let Some(_) = current_file(&mut *db.get()?, &p)? else { return Ok(()) };
    let dest = proxy_path(cache_root, &p.root);
    if dest.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(dest.parent().context("cache path")?)?;
    let tmp = dest.with_extension(format!("{}.tmp.mp4", std::process::id()));
    let result = media::proxy(&store.live_path(&p.device, &p.path)?, &tmp);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result?;
    std::fs::rename(&tmp, &dest)?;
    db::notify(&mut *db.get()?, "files", &p.device)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photos_and_videos_are_recognised_by_extension() {
        assert!(
            wants("photos/IMG_1.JPG") && wants("a.heic") && wants("clip.MOV") && !wants("notes.txt") && !wants("jpg")
        );
        assert_eq!((kind("a.png"), kind("a.mp4"), kind("a.pdf")), (Some("image"), Some("video"), None));
        assert!(can_preview("a.png") && can_preview("a.mp4"));
        assert_eq!(mime("a.png"), "image/png");
        assert_eq!(mime("noext"), "application/octet-stream");
    }

    #[test]
    fn times_are_unix_seconds() {
        assert_eq!(parse_time("2023-11-14 22:13:20"), Some(1_700_000_000));
        assert_eq!(parse_time("2023-11-14T22:13:20.000000Z"), Some(1_700_000_000));
        assert_eq!(parse_time("1970-01-01 00:00:00"), Some(0));
        assert_eq!(parse_time("2024-02-29 12:00:00"), Some(1_709_208_000));
        assert_eq!(parse_time("not a time"), None);
        assert_eq!(parse_time("2023-13-14 22:13:20"), None);
        assert_eq!(parse_time("0000:00:00 00:00:00"), None);
    }

    #[test]
    fn thumbnails_are_made_once_and_cached() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.png");
        image::RgbImage::from_pixel(64, 32, image::Rgb([200, 10, 10])).save(&src).unwrap();
        let cache = dir.path().join("cache");
        let thumb = thumbnail(&src, "abc", 16, &cache).unwrap();
        let (w, h) = image::image_dimensions(&thumb).unwrap();
        assert_eq!((w, h), (16, 8));
        assert_eq!(thumbnail(&src, "abc", 16, &cache).unwrap(), thumb);
        let meta = read(&src).unwrap();
        assert_eq!((meta.width, meta.kind), (Some(64), "image"));
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1, "no temporary files are left");
    }

    #[test]
    fn a_photo_stored_sideways_is_shown_upright() {
        // A 4x2 JPEG with the EXIF orientation "rotate 90" (6): people see it 2x4.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("sideways.jpg");
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode_image(&image::RgbImage::from_pixel(4, 2, image::Rgb([9, 9, 9])))
            .unwrap();
        // APP1 segment: "Exif\0\0", a little-endian TIFF header, one entry (0x0112, SHORT, 1, value 6).
        let tiff: Vec<u8> = [
            &b"II*\0"[..],
            &8u32.to_le_bytes(),
            &1u16.to_le_bytes(),
            &0x0112u16.to_le_bytes(),
            &3u16.to_le_bytes(),
            &1u32.to_le_bytes(),
            &6u16.to_le_bytes(),
            &[0, 0],
            &0u32.to_le_bytes(),
        ]
        .concat();
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(tiff);
        let mut with_exif = jpeg[..2].to_vec();
        with_exif.extend([0xFF, 0xE1]);
        with_exif.extend(((app1.len() + 2) as u16).to_be_bytes());
        with_exif.extend(app1);
        with_exif.extend(&jpeg[2..]);
        std::fs::write(&src, with_exif).unwrap();

        let meta = read(&src).unwrap();
        assert_eq!((meta.width, meta.height), (Some(2), Some(4)));
        let thumb = thumbnail(&src, "rot", 100, &dir.path().join("cache")).unwrap();
        assert_eq!(image::image_dimensions(thumb).unwrap(), (2, 4));
    }

    #[test]
    fn the_worker_reads_a_photo_and_makes_its_pictures() {
        use crate::files::ingest::{commit_version, NewVersion};
        use crate::platform::db::testing;
        use fafnirx_common::{chunk_hash, root_hash};

        let db = testing::db_with_device();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut data = Vec::new();
        image::RgbImage::from_pixel(8, 4, image::Rgb([5, 6, 7]))
            .write_to(&mut std::io::Cursor::new(&mut data), image::ImageFormat::Png)
            .unwrap();
        let root = root_hash(&[chunk_hash(&data)], data.len() as u64);
        let staged = store.temp_path();
        std::fs::write(&staged, &data).unwrap();
        let device = testing::device();
        let v = NewVersion {
            device: &device,
            path: "trip/a.png",
            root,
            size: data.len() as u64,
            mtime: 0,
            mode: 0,
            renamed_from: None,
        };
        commit_version(&mut db.get().unwrap(), &store, &staged, &v).unwrap();

        let jobs = |db: &Db| -> i64 { db.get().unwrap().query_one("SELECT COUNT(*) FROM jobs", &[]).unwrap().get(0) };
        assert_eq!(jobs(&db), 1);

        let (_, kind, payload, _) = claim(&db, false).unwrap().unwrap();
        assert_eq!(kind, "meta");
        let cache = dir.path().join("cache");
        read_job(&db, &store, &cache, &payload).unwrap();
        let row = db.get().unwrap().query_one("SELECT kind, width, height, lat FROM file_meta", &[]).unwrap();
        assert_eq!((row.get::<_, String>(0), row.get::<_, i64>(1), row.get::<_, i64>(2)), ("image".into(), 8, 4));
        assert_eq!(row.get::<_, Option<f64>>(3), None);
        let root_hex = hex::encode(root);
        for size in [THUMB, PREVIEW] {
            assert!(cache.join("thumbs").join(format!("{root_hex}-{size}.jpg")).is_file(), "{size}px picture");
        }
    }

    #[test]
    fn the_library_column_of_files_agrees_with_the_extension_lists() {
        use crate::platform::db::testing;
        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        let mut paths: Vec<String> =
            IMAGE_EXTENSIONS.iter().chain(&VIDEO_EXTENSIONS).map(|e| format!("a/x.{e}")).collect();
        paths.extend(["a/X.JPG", "a/video.MP4", "a/x.jpg.txt", "a/jpg", "a/x.docx", "a/.mp4x"].map(String::from));
        for path in &paths {
            c.execute("INSERT INTO files (device_id, path) VALUES (1, $1)", &[path]).unwrap();
            let media: Option<String> = c.query_one("SELECT media FROM files WHERE path = $1", &[path]).unwrap().get(0);
            assert_eq!(media.as_deref(), kind(path), "{path}");
        }
    }

    #[test]
    fn slow_video_copies_never_hold_up_the_fast_lane() {
        use crate::platform::db::testing;
        let db = testing::db_with_device();
        {
            let mut c = db.get().unwrap();
            db::enqueue(&mut *c, "proxy", &serde_json::json!({})).unwrap();
            db::enqueue(&mut *c, "meta", &serde_json::json!({})).unwrap();
        }
        let kinds = |lane: bool| claim(&db, lane).unwrap().map(|c| c.1);
        assert_eq!(kinds(false).as_deref(), Some("meta"));
        assert_eq!(kinds(false), None, "the fast lane leaves proxies alone");
        assert_eq!(kinds(true).as_deref(), Some("proxy"));
    }

    #[test]
    fn positions_must_mean_a_place() {
        assert_eq!(valid_position(52.5, 13.4), Some((52.5, 13.4)));
        assert_eq!(valid_position(0.0, 0.0), None);
        assert_eq!(valid_position(91.0, 0.0), None);
        assert_eq!(valid_position(f64::NAN, 1.0), None);
    }
}
