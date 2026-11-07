use super::*;
use fafnirx_common::LockExt;

#[test]
fn photos_and_videos_are_recognised_by_extension() {
    let _on = THUMBNAILS_TEST.locked();
    assert!(wants("photos/IMG_1.JPG") && wants("a.heic") && wants("clip.MOV") && !wants("notes.txt") && !wants("jpg"));
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
    let _on = THUMBNAILS_TEST.locked();
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
        jobs::enqueue(&mut *db.get().unwrap(), JobKind::Thumb, &serde_json::to_value(job).unwrap()).unwrap();
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
    let mut paths: Vec<String> = IMAGE_EXTENSIONS.iter().chain(&VIDEO_EXTENSIONS).map(|e| format!("a/x.{e}")).collect();
    paths.extend(["a/X.JPG", "a/video.MP4", "a/x.jpg.txt", "a/jpg", "a/x.docx", "a/.mp4x"].map(String::from));
    for path in &paths {
        c.execute("INSERT INTO files (device_id, path) VALUES (1, $1)", &[path]).unwrap();
        let media: Option<String> = c.query_one("SELECT media FROM files WHERE path = $1", &[path]).unwrap().get(0);
        assert_eq!(media.as_deref(), kind(path), "{path}");
    }
}
