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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Extensions of images that get their metadata read and a preview.
const IMAGE_EXTENSIONS: [&str; 11] =
    ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif", "avif"];
/// Extensions of videos.
const VIDEO_EXTENSIONS: [&str; 11] = ["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "mts", "3gp", "mpg", "mpeg"];
/// Extensions the `image` crate can decode here (HEIC and AVIF have metadata but need the video module for a preview).
const PREVIEW_EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff"];
const MAX_ATTEMPTS: i64 = 3;

/// Jobs worked on at the same time: one for each core, but at least two and at most six (each holds a decoded picture).
fn workers() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get().clamp(2, 6))
}

/// The setting that turns thumbnails and previews on or off (on when it is not set).
pub const THUMBNAILS: &str = "thumbnails";

/// Whether pictures are made at all. Kept here so that the answer is free where a page is put together; it is loaded when the
/// master starts (`load_thumbnails`) and set when the administrator changes it.
static THUMBNAILS_ON: AtomicBool = AtomicBool::new(true);

pub fn thumbnails_enabled() -> bool {
    THUMBNAILS_ON.load(Ordering::Relaxed)
}

pub fn set_thumbnails(on: bool) {
    THUMBNAILS_ON.store(on, Ordering::Relaxed);
}

pub fn load_thumbnails(c: &mut impl postgres::GenericClient) {
    set_thumbnails(settings::get_i64(c, THUMBNAILS, 1) != 0);
}

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
    if !thumbnails_enabled() {
        return false;
    }
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

/// Reads EXIF, keeping what could be read when a part of it is damaged: cameras, scanners and editors write entries that
/// do not parse, and one of them must not lose the date and the place that are fine.
fn read_exif(file: std::fs::File) -> Option<exif::Exif> {
    let found = exif::Reader::new().continue_on_error(true).read_from_container(&mut BufReader::new(file));
    found.or_else(|e| e.distill_partial_result(|errors| tracing::debug!("damaged EXIF: {errors:?}"))).ok()
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
        description: None,
        keywords: Vec::new(),
    };
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let data = read_exif(file);
    let found = embedded::read(path, data.as_ref());
    meta.taken_at = found.taken_at;
    meta.position = found.position;
    meta.description = found.description;
    meta.keywords = found.keywords;
    if let Some(data) = data {
        // The small picture inside the file (its own thumbnail IFD) has fields of its own, which are not the photo's.
        for field in data.fields().filter(|f| f.ifd_num == exif::In::PRIMARY) {
            let name = field.tag.to_string();
            if EXIF_FIELDS.contains(&name.as_str()) && !meta.exif.contains_key(&name) {
                let shown = field.display_value().with_unit(&data).to_string();
                meta.exif.insert(name, serde_json::Value::String(shown));
            }
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

/// A copy that was being written when the master stopped.
const PARTIAL_KEEP: std::time::Duration = std::time::Duration::from_secs(86400);

/// Clears the unfinished files of the cache. Pictures are kept for good: they take room, and are made once.
pub fn clean_cache(cache_root: &Path) {
    for e in std::fs::read_dir(cache_root.join("thumbs")).into_iter().flatten().flatten() {
        let partial = e.file_name().to_string_lossy().contains(".tmp");
        let age = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).unwrap_or_default();
        if partial && age > PARTIAL_KEEP {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Where the picture of a content at a size is kept, in the folder of the pictures.
pub fn cached_picture(thumbs: &Path, root_hex: &str, max: u32) -> PathBuf {
    thumbs.join(format!("{root_hex}-{max}.jpg"))
}

/// Makes a JPEG picture no larger than `max` on its longest side, kept by content and size (see `cached_picture`). Videos get
/// a frame: the one at `poster_ms` when it is known, else the first one that is not black; the answer says where that was.
pub fn thumbnail(src: &Path, root_hex: &str, max: u32, cache: &Path, poster_ms: Option<i64>) -> Result<Option<i64>> {
    let dest = cached_picture(cache, root_hex, max);
    if dest.is_file() {
        return Ok(None);
    }
    std::fs::create_dir_all(cache)?;
    // Unique per call: two jobs for the same picture could run at once.
    let n = hex::encode(crate::platform::db::random_bytes::<8>());
    let tmp = cache.join(format!("{root_hex}-{max}.{n}.tmp.jpg"));
    let result = make_thumbnail(src, root_hex, max, poster_ms, &tmp);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    let learned = result?;
    std::fs::rename(&tmp, &dest)?;
    Ok(learned)
}

/// A frame of the file, drawn by a module, written to `tmp`. Returns where in the video it was taken, when the module says.
fn module_poster(src: &Path, root_hex: &str, max: u32, poster_ms: Option<i64>, tmp: &Path) -> Result<Option<i64>> {
    let (jpeg, at_ms) =
        modules::poster(src, root_hex, max, poster_ms)?.context("no module can draw a frame of this file")?;
    std::fs::write(tmp, jpeg)?;
    Ok(at_ms)
}

fn make_thumbnail(src: &Path, root_hex: &str, max: u32, poster_ms: Option<i64>, tmp: &Path) -> Result<Option<i64>> {
    let name = src.to_string_lossy();
    if kind(&name) == Some("video") {
        return module_poster(src, root_hex, max, poster_ms, tmp);
    }
    match decode_upright(src) {
        Ok(img) => {
            // Never enlarge: a small picture stays as it is.
            let small = if img.width() > max || img.height() > max { img.thumbnail(max, max) } else { img }.to_rgb8();
            let mut out = std::io::BufWriter::new(std::fs::File::create(tmp)?);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
                .encode_image(&small)
                .context("encode preview")?;
            Ok(None)
        }
        // Formats the image crate cannot read (HEIC, AVIF) are left to a module that has ffmpeg, when there is one.
        Err(e) if modules::can_poster() => {
            module_poster(src, root_hex, max, None, tmp).with_context(|| format!("decode {}: {e}", src.display()))
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
        for _ in 0..workers() {
            let (db, store, cache_root) = (db.clone(), store.clone(), cache_root.clone());
            std::thread::spawn(move || work(&db, &store, &cache_root));
        }
    });
}

/// The setting that holds the queue back while it is `1` (see `monitoring::queue`). Jobs already running finish.
pub const PAUSED: &str = "jobs_paused";

/// Workers that have nothing to do wait here; a job put in the queue by the web app wakes them at once.
static IDLE: (std::sync::Mutex<()>, std::sync::Condvar) = (std::sync::Mutex::new(()), std::sync::Condvar::new());

/// Tells the workers that there is a job to do.
pub fn wake() {
    IDLE.1.notify_all();
}

fn idle(how_long: Duration) {
    if let Ok(guard) = IDLE.0.lock() {
        let _ = IDLE.1.wait_timeout(guard, how_long);
    }
}

/// One lane of the queue, forever.
fn work(db: &Db, store: &Store, cache_root: &Path) {
    loop {
        if db.get().is_ok_and(|mut c| settings::get_i64(&mut *c, PAUSED, 0) == 1) {
            std::thread::sleep(Duration::from_secs(5));
            continue;
        }
        match work_one(db, store, cache_root) {
            Ok(true) => {}
            Ok(false) => idle(Duration::from_secs(2)),
            Err(e) => {
                tracing::error!("job queue: {e:#}");
                std::thread::sleep(Duration::from_secs(10));
            }
        }
    }
}

/// Takes the next job and does it. Returns whether there was one.
pub(crate) fn work_one(db: &Db, store: &Store, cache_root: &Path) -> Result<bool> {
    let Some(jobs::Claimed { id, kind, payload, attempts }) = claim(db)? else { return Ok(false) };
    let result = match kind.as_str() {
        "meta" => read_job(db, store, &payload).map(|()| None),
        "artwork" => artwork_job(db, &payload),
        "thumb" => thumb_job(db, store, cache_root, &payload),
        other => Err(anyhow::anyhow!("unknown job kind {other:?}")),
    };
    finish(db, id, &kind, &payload, attempts, result);
    Ok(true)
}

fn claim(db: &Db) -> Result<Option<jobs::Claimed>> {
    Ok(jobs::claim(&mut *db.get()?, unix_now())?)
}

fn finish(db: &Db, id: uuid::Uuid, kind: &str, payload: &str, attempts: i64, result: Result<Option<String>>) {
    let Ok(mut c) = db.get() else { return };
    let mut over = true;
    let done = match result {
        Ok(note) => jobs::complete(&mut *c, id, note.as_deref()),
        Err(e) => {
            tracing::warn!("job {id} failed (attempt {attempts}): {e:#}");
            let give_up = attempts >= MAX_ATTEMPTS;
            over = give_up;
            let wait = if give_up { 0 } else { 30 * attempts };
            jobs::fail(&mut *c, id, give_up, unix_now() + wait, &format!("{e:#}"))
        }
    };
    if let Err(e) = done {
        tracing::error!("recording the result of job {id}: {e}");
    }
    // Whoever asked for a picture waits for this: the picture is there, or there will not be one.
    if kind == "thumb" && over {
        if let Some(file) = serde_json::from_str::<ThumbJob>(payload).ok().map(|j| j.id) {
            let _ = db::notify(&mut *c, "thumbs", &file);
        }
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
    // Pictures are not made here: they are made when somebody looks at the file (see `thumb_job`). A video also gets a look
    // for the picture and description that sit next to it, as a job of its own so that the queue shows what it found.
    if kind(&p.path) == Some("video") {
        jobs::enqueue(&mut *c, "artwork", &serde_json::json!({ "device": p.device, "path": p.path, "root": p.root }))?;
    }
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
    db::notify(&mut *c, "files", &p.device)?;
    Ok(Some(format!("{picture}; {details}")))
}

/// A picture somebody asked for: of this file (`id` is how the browser knows it), at this size.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ThumbJob {
    pub id: String,
    pub device: String,
    pub path: String,
    pub root: String,
    /// Where the picture is kept (see `cached_picture`): by the content of the file, or of the picture beside a video.
    pub key: String,
    pub size: u32,
    /// Which content: a file keeps its older versions, and a deleted file its last one, in places of their own.
    pub version: i64,
    pub current: Option<i64>,
    pub deleted: bool,
}

/// Makes the picture a `thumb` job names, so that the browser that asked for it can show it. A video with a picture next to
/// it (see `artwork`) gets its picture from that; any other needs a module that can draw a frame of it.
fn thumb_job(db: &Db, store: &Store, cache_root: &Path, payload: &str) -> Result<Option<String>> {
    if !thumbnails_enabled() {
        // Nothing is made; and no note, so that a job asked for again once thumbnails are on is made.
        return Ok(None);
    }
    let p: ThumbJob = serde_json::from_str(payload)?;
    let mut c = db.get()?;
    let root = hex::decode(&p.root)?;
    let live = !p.deleted && Some(p.version) == p.current;
    // A live file may have been replaced since the picture was asked for: then it is not the content that was asked for.
    if live && !file_meta::path_holds(&mut *c, &p.device, &p.path, &root)? {
        return Ok(Some("replaced or deleted since".into()));
    }
    let own = crate::files::browse::version_file(store, &p.device, &p.path, p.version, p.current, p.deleted)?;
    let art = if live { artwork::lookup(&mut c, store, &p.device, &p.path) } else { None };
    let (source, root_hex, poster_ms) = match art {
        Some((art, art_root)) => (art, hex::encode(art_root), None),
        None if !can_preview(&p.path) => {
            return Ok(Some("no picture next to it, and no video module to draw a frame".into()));
        }
        None => (own, hex::encode(&root), file_meta::poster_ms(&mut *c, &root)?),
    };
    // A picture was added or taken away beside the video since the request: the answer would be kept under another key.
    if root_hex != p.key {
        return Ok(Some("the picture beside it changed".into()));
    }
    let learned = thumbnail(&source, &root_hex, p.size, &cache_root.join("thumbs"), poster_ms)
        .with_context(|| format!("the {}px picture of {}", p.size, p.path))?;
    // Where the frame of a video was found is kept, so that it is always taken from the same place.
    if let (None, Some(ms)) = (poster_ms, learned) {
        file_meta::set_poster_ms(&mut *c, &root, ms)?;
    }
    Ok(None)
}

/// Tests that depend on the setting for pictures, or change it, hold this, since the setting is the master's, not a test's.
#[cfg(test)]
pub(crate) static THUMBNAILS_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Does the picture jobs that are waiting (and no other), as the workers would.
#[cfg(test)]
pub(crate) fn make_queued_pictures(db: &Db, store: &Store, cache_root: &Path) {
    let waiting = || -> i64 {
        db.get()
            .unwrap()
            .query_one("SELECT COUNT(*) FROM jobs WHERE kind = 'thumb' AND status = 'queued'", &[])
            .unwrap()
            .get(0)
    };
    while waiting() > 0 {
        work_one(db, store, cache_root).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photos_and_videos_are_recognised_by_extension() {
        let _on = THUMBNAILS_TEST.lock().unwrap_or_else(|e| e.into_inner());
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
        thumbnail(&src, "abc", 16, &cache, None).unwrap();
        let thumb = cached_picture(&cache, "abc", 16);
        assert_eq!(image::image_dimensions(&thumb).unwrap(), (16, 8));
        thumbnail(&src, "abc", 16, &cache, None).unwrap();
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
        let cache = dir.path().join("cache");
        thumbnail(&src, "rot", 100, &cache, None).unwrap();
        assert_eq!(image::image_dimensions(cached_picture(&cache, "rot", 100)).unwrap(), (2, 4));
    }

    #[test]
    fn the_worker_reads_a_photo_and_makes_a_picture_only_when_asked() {
        let _on = THUMBNAILS_TEST.lock().unwrap_or_else(|e| e.into_inner());
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

        let cache = dir.path().join("cache");
        assert!(work_one(&db, &store, &cache).unwrap(), "the metadata is read");
        assert!(!work_one(&db, &store, &cache).unwrap(), "and no picture is made ahead: nothing else is queued");
        assert_eq!(jobs(&db), 1);
        let row = db.get().unwrap().query_one("SELECT kind, width, height, lat FROM file_meta", &[]).unwrap();
        assert_eq!((row.get::<_, String>(0), row.get::<_, i64>(1), row.get::<_, i64>(2)), ("image".into(), 8, 4));
        assert_eq!(row.get::<_, Option<f64>>(3), None);

        // A picture is a job that is queued when somebody asks for it.
        let root_hex = hex::encode(root);
        let ask = |size: u32| {
            let job = ThumbJob {
                id: "someid".into(),
                device: device.name.clone(),
                path: "trip/a.png".into(),
                root: root_hex.clone(),
                key: root_hex.clone(),
                size,
                version: 1,
                current: Some(1),
                deleted: false,
            };
            jobs::enqueue(&mut *db.get().unwrap(), "thumb", &serde_json::to_value(job).unwrap()).unwrap();
        };
        ask(THUMB);
        ask(THUMB);
        assert_eq!(jobs(&db), 2, "asking twice for the same picture is one job");
        ask(PREVIEW);
        assert!(work_one(&db, &store, &cache).unwrap() && work_one(&db, &store, &cache).unwrap());
        for size in [THUMB, PREVIEW] {
            assert!(cached_picture(&cache.join("thumbs"), &root_hex, size).is_file(), "{size}px picture");
        }
        // With thumbnails off nothing is made, even for a job that was queued before.
        let other = ThumbJob {
            id: "someid".into(),
            device: device.name.clone(),
            path: "trip/a.png".into(),
            root: root_hex.clone(),
            key: root_hex.clone(),
            size: 99,
            version: 1,
            current: Some(1),
            deleted: false,
        };
        set_thumbnails(false);
        let note = thumb_job(&db, &store, &cache, &serde_json::to_string(&other).unwrap()).unwrap();
        set_thumbnails(true);
        assert_eq!(note, None);
        assert!(
            !cached_picture(&cache.join("thumbs"), &root_hex, 99).exists(),
            "a picture was made with thumbnails off"
        );
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
