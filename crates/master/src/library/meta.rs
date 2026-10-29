//! What the master knows about the content of a file beyond its bytes: the kind, the size of an image or
//! video, the EXIF data and GPS position of a photo, and previews. Read by the background queue, shown by the
//! web API.

use crate::library::{artwork, embedded};
use crate::modules;
use crate::platform::db::{self, Db};
use crate::platform::store::Store;
use crate::repo::{file_meta, files, jobs, settings};
use anyhow::{Context, Result};
pub use fafnirx_common::meta::{parse_time, valid_position};
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
/// Extensions the `image` crate can decode here (HEIC and AVIF have metadata but need the video module for a preview).
const PREVIEW_EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff"];
const MAX_ATTEMPTS: i64 = 3;
/// Jobs worked on at the same time.
const WORKERS: usize = 2;

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

/// Whether a preview picture can be made from this file: images the `image` crate decodes, and what a module can take a
/// frame of (videos, HEIC and AVIF) while there is one.
pub fn can_preview(path: &str) -> bool {
    match kind(path) {
        Some("video") => modules::can_poster(),
        Some(_) => PREVIEW_EXTENSIONS.contains(&extension(path).as_str()) || modules::can_poster(),
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
    /// The caption written into the file, and its keywords (what photo managers and AI taggers store as tags).
    pub description: Option<String>,
    pub keywords: Vec<String>,
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
        description: None,
        keywords: Vec::new(),
    };
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let data = exif::Reader::new().read_from_container(&mut BufReader::new(file)).ok();
    let found = embedded::read(path, data.as_ref());
    meta.position = found.position;
    meta.description = found.description;
    meta.keywords = found.keywords;
    if let Some(data) = data {
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

/// Reads a video, or just recognises it when there is no video module to ask. `hash` is the hex hash of its content.
fn read_video(path: &Path, hash: &str) -> Result<Meta> {
    // A module that is there and fails is an error: the job is tried again, and not saved as a video without details.
    let probe = modules::probe_video(path, hash)?.unwrap_or_default();
    Ok(Meta {
        kind: "video",
        width: probe.width,
        height: probe.height,
        taken_at: probe.taken_at,
        exif: serde_json::Map::new(),
        duration_ms: probe.duration_ms,
        codec: probe.codec,
        position: probe.position,
        description: None,
        keywords: Vec::new(),
    })
}

/// Reads whatever the file is, by its name. `hash` is the hex hash of its content.
pub fn read_any(name: &str, path: &Path, hash: &str) -> Result<Meta> {
    match kind(name) {
        Some("video") => read_video(path, hash),
        _ => read(path),
    }
}

/// Decodes an image and turns it upright according to its EXIF orientation.
fn decode_upright(src: &Path) -> Result<image::DynamicImage> {
    let mut decoder = image::ImageReader::open(src)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Pictures nobody asked for this long are made again on demand (asking again renews them, see `thumbnail`).
const THUMB_KEEP: std::time::Duration = std::time::Duration::from_secs(30 * 86400);
/// A copy that was being written when the master stopped.
const PARTIAL_KEEP: std::time::Duration = std::time::Duration::from_secs(86400);

/// Clears the cache: old pictures (they are made again when someone looks) and unfinished files.
pub fn clean_cache(cache_root: &Path) {
    for e in std::fs::read_dir(cache_root.join("thumbs")).into_iter().flatten().flatten() {
        let partial = e.file_name().to_string_lossy().contains(".tmp");
        let age = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).unwrap_or_default();
        if age > if partial { PARTIAL_KEEP } else { THUMB_KEEP } {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// A JPEG preview no larger than `max` on its longest side, cached by content and size. Videos get a frame.
pub fn thumbnail(src: &Path, root_hex: &str, max: u32, cache: &Path) -> Result<PathBuf> {
    let dest = cache.join(format!("{root_hex}-{max}.jpg"));
    if let Ok(modified) = std::fs::metadata(&dest).and_then(|m| m.modified()) {
        // Asked for again: the picture is kept another `THUMB_KEEP` (once a day is often enough to note it).
        if modified.elapsed().is_ok_and(|age| age > Duration::from_secs(86400)) {
            let _ = std::fs::File::options()
                .write(true)
                .open(&dest)
                .and_then(|f| f.set_modified(std::time::SystemTime::now()));
        }
        return Ok(dest);
    }
    std::fs::create_dir_all(cache)?;
    // Unique per call: two requests for the same preview can run at once.
    let n = hex::encode(crate::platform::db::random_bytes::<8>());
    let tmp = cache.join(format!("{root_hex}-{max}.{n}.tmp.jpg"));
    let result = make_thumbnail(src, root_hex, max, &tmp);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result?;
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

/// A frame of the file, drawn by a module, written to `tmp`.
fn module_poster(src: &Path, root_hex: &str, max: u32, tmp: &Path) -> Result<()> {
    let jpeg = modules::poster(src, root_hex, max)?.context("no module can draw a frame of this file")?;
    std::fs::write(tmp, jpeg)?;
    Ok(())
}

fn make_thumbnail(src: &Path, root_hex: &str, max: u32, tmp: &Path) -> Result<()> {
    let name = src.to_string_lossy();
    if kind(&name) == Some("video") {
        return module_poster(src, root_hex, max, tmp);
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
        // Formats the image crate cannot read (HEIC, AVIF) are left to a module that has ffmpeg, when there is one.
        Err(e) if modules::can_poster() => {
            module_poster(src, root_hex, max, tmp).with_context(|| format!("decode {}: {e}", src.display()))
        }
        Err(e) => Err(e).with_context(|| format!("decode {}", src.display())),
    }
}

/// Runs queued jobs until the process ends. Several workers can run side by
/// side: a job is claimed with `FOR UPDATE SKIP LOCKED`.
pub fn run_queue(db: Db, store: std::sync::Arc<Store>, cache_root: PathBuf) {
    // The database client blocks, so everything here runs on threads of its own, never on the async runtime.
    std::thread::spawn(move || {
        // A restart interrupts whatever was running.
        if let Ok(mut c) = db.get() {
            let _ = jobs::requeue_running(&mut *c);
        }
        for _ in 0..WORKERS {
            let (db, store, cache_root) = (db.clone(), store.clone(), cache_root.clone());
            std::thread::spawn(move || work(&db, &store, &cache_root));
        }
    });
}

/// The setting that holds the queue back while it is `1` (see `monitoring::queue`). Jobs already running finish.
pub const PAUSED: &str = "jobs_paused";

/// One lane of the queue, forever.
fn work(db: &Db, store: &Store, cache_root: &Path) {
    loop {
        if db.get().is_ok_and(|mut c| settings::get_i64(&mut *c, PAUSED, 0) == 1) {
            std::thread::sleep(Duration::from_secs(5));
            continue;
        }
        match claim(db) {
            Ok(Some(jobs::Claimed { id, kind, payload, attempts })) => {
                let result = match kind.as_str() {
                    "meta" => read_job(db, store, &payload).map(|()| None),
                    "artwork" => artwork_job(db, &payload),
                    "thumb" => thumb_job(db, store, cache_root, &payload),
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

fn claim(db: &Db) -> Result<Option<jobs::Claimed>> {
    Ok(jobs::claim(&mut *db.get()?, unix_now())?)
}

fn finish(db: &Db, id: uuid::Uuid, attempts: i64, result: Result<Option<String>>) {
    let Ok(mut c) = db.get() else { return };
    let done = match result {
        Ok(note) => jobs::complete(&mut *c, id, note.as_deref()),
        Err(e) => {
            tracing::warn!("job {id} failed (attempt {attempts}): {e:#}");
            let give_up = attempts >= MAX_ATTEMPTS;
            let wait = if give_up { 0 } else { 30 * attempts };
            jobs::fail(&mut *c, id, give_up, unix_now() + wait, &format!("{e:#}"))
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
    let current = file_meta::path_holds(c, &p.device, &p.path, &root)?;
    Ok(current.then_some(root))
}

/// Reads the metadata of the file a `meta` job names, if it still holds that content, and queues its pictures.
fn read_job(db: &Db, store: &Store, payload: &str) -> Result<()> {
    let p: JobFile = serde_json::from_str(payload)?;
    let mut c = db.get()?;
    let Some(root) = current_file(&mut c, &p)? else {
        return Ok(()); // replaced or deleted since; the newer content has its own job
    };
    let disk = store.live_path(&p.device, &p.path)?;
    let meta = read_any(&p.path, &disk, &p.root)?;
    let exif = serde_json::Value::Object(meta.exif).to_string();
    let keywords = serde_json::to_string(&meta.keywords)?;
    file_meta::save(
        &mut *c,
        &root,
        &file_meta::Meta {
            mime: &mime(&p.path),
            kind: meta.kind,
            width: meta.width,
            height: meta.height,
            taken_at: meta.taken_at,
            exif: &exif,
            duration_ms: meta.duration_ms,
            codec: meta.codec.as_deref(),
            place: meta.position,
            description: meta.description.as_deref(),
            keywords: &keywords,
        },
        unix_now(),
    )?;
    db::notify(&mut *c, "files", &p.device)?;
    // Each step is a job of its own, so that the queue shows it (and what went wrong with it): looking for the picture
    // and description that sit next to a video, then drawing the pictures.
    let next = if kind(&p.path) == Some("video") { "artwork" } else { "thumb" };
    jobs::enqueue(&mut *c, next, &serde_json::json!({ "device": p.device, "path": p.path, "root": p.root }))?;
    Ok(())
}

/// Looks for the picture and the .nfo next to a video, and says what it found. Nothing is stored: the pictures are looked
/// up again whenever they are asked for, so one that arrives later is used.
fn artwork_job(db: &Db, payload: &str) -> Result<Option<String>> {
    let p: JobFile = serde_json::from_str(payload)?;
    let mut c = db.get()?;
    if current_file(&mut c, &p)?.is_none() {
        return Ok(Some("replaced or deleted since".into()));
    }
    let picture = match artwork::find_picture(&mut c, &p.device, &p.path) {
        Some((art, _, artwork::Source::Own)) => format!("picture: {art}"),
        Some((art, _, artwork::Source::Folder)) => format!("picture of its folder: {art}"),
        None => "no picture next to it: a frame of the video is used".to_string(),
    };
    let near = files::neighbours(&mut *c, &p.device, &p.path)?;
    let details = match artwork::description(&p.path, &near) {
        Some((nfo, _)) => format!("details: {nfo}"),
        None => "no .nfo".to_string(),
    };
    jobs::enqueue(&mut *c, "thumb", &serde_json::json!({ "device": p.device, "path": p.path, "root": p.root }))?;
    db::notify(&mut *c, "files", &p.device)?;
    Ok(Some(format!("{picture}; {details}")))
}

/// Makes the pictures of the file a `thumb` job names, so that opening the timeline finds them ready. A video with a picture
/// next to it (see `artwork`) gets its pictures from that; any other needs a module that can draw a frame of it.
fn thumb_job(db: &Db, store: &Store, cache_root: &Path, payload: &str) -> Result<Option<String>> {
    let p: JobFile = serde_json::from_str(payload)?;
    let mut c = db.get()?;
    let Some(root) = current_file(&mut c, &p)? else {
        return Ok(Some("replaced or deleted since".into()));
    };
    let own = store.live_path(&p.device, &p.path)?;
    let (source, root_hex) = match artwork::lookup(&mut c, store, &p.device, &p.path) {
        Some((art, art_root)) => (art, hex::encode(art_root)),
        None if !can_preview(&p.path) => {
            return Ok(Some("no picture next to it, and no video module to draw a frame".into()));
        }
        None => (own.clone(), hex::encode(&root)),
    };
    let thumbs = cache_root.join("thumbs");
    for size in [THUMB, PREVIEW] {
        thumbnail(&source, &root_hex, size, &thumbs).with_context(|| format!("the {size}px picture of {}", p.path))?;
    }
    db::notify(&mut *c, "files", &p.device)?;
    Ok(None)
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
        assert!(can_preview("a.png"), "images are drawn here");
        assert!(
            !can_preview("a.mp4") && !can_preview("a.heic"),
            "frames come from the video module, which is not here"
        );
        assert_eq!(mime("a.png"), "image/png");
        assert_eq!(mime("noext"), "application/octet-stream");
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

        let jobs::Claimed { kind, payload, .. } = claim(&db).unwrap().unwrap();
        assert_eq!(kind, "meta");
        let cache = dir.path().join("cache");
        read_job(&db, &store, &payload).unwrap();
        let jobs::Claimed { kind, payload, .. } = claim(&db).unwrap().unwrap();
        assert_eq!(kind, "thumb", "the pictures are a job of their own");
        thumb_job(&db, &store, &cache, &payload).unwrap();
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
}
