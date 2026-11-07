#[test]
fn paths_stay_inside_the_tree_and_master_files_are_one_part() {
    for ok in ["clip.mp4", "scans/2024/clip.mp4", "admin/pc/clip.mp4", "اناشيد/ملف.mp4"] {
        assert!(super::valid_path(ok), "{ok}");
    }
    for bad in ["", "/", "../clip.mp4", "a/../b.mp4", "a//b.mp4", "a\\b.mp4", "c:/b.mp4", "a/./b.mp4"] {
        assert!(!super::valid_path(bad), "{bad}");
    }
}

use super::*;
use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use tower::ServiceExt;

const H: &str = "ab12cd34";

struct Fixture {
    app: Router,
    data: tempfile::TempDir,
    cache: PathBuf,
    _ui: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let ui = tempfile::tempdir().unwrap();
    std::fs::write(ui.path().join("index.js"), "export default {}").unwrap();
    std::fs::create_dir_all(data.path().join("files/pc")).unwrap();
    std::fs::write(data.path().join("files/pc/clip.mkv"), b"not really a video").unwrap();
    let cache = data.path().join("cache");
    let app = router(App::new(Config {
        data: data.path().to_path_buf(),
        cache: cache.clone(),
        token: "secret-token".into(),
        // Programs that do not exist: a request that needs one fails, one that does not, works.
        ffmpeg: "fafnirx-no-such-ffmpeg".into(),
        ffprobe: "fafnirx-no-such-ffprobe".into(),
        vcodec: "libx264".into(),
        jobs: 2,
        ui: Some(ui.path().to_path_buf()),
    }));
    Fixture { app, data, cache, _ui: ui }
}

/// The video as if it had been probed already: 1280x720, 14.5 s.
fn probed(f: &Fixture) {
    let dir = f.cache.join(cache_key("pc/clip.mkv", H));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("probe.txt"),
        source_text(&Source { width: 1280, height: 720, duration: 14.5, has_audio: true }),
    )
    .unwrap();
}

async fn get(f: &Fixture, uri: &str, token: Option<&str>) -> (StatusCode, String) {
    let mut req = Request::builder().uri(uri);
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let resp = f.app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    (status, String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned())
}

const T: Option<&str> = Some("secret-token");

#[tokio::test]
async fn only_the_master_may_ask() {
    let f = fixture();
    assert_eq!(get(&f, "/health", None).await.0, StatusCode::OK, "health needs no token");
    let uri = format!("/files/hls/master.m3u8?p=pc/clip.mkv&h={H}");
    assert_eq!(get(&f, &uri, None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(get(&f, &uri, Some("wrong")).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(get(&f, &uri, Some("secret-token-and-more")).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_manifest_says_what_the_module_is() {
    let f = fixture();
    assert_eq!(get(&f, "/manifest", None).await.0, StatusCode::UNAUTHORIZED);
    let (status, body) = get(&f, "/manifest", T).await;
    assert_eq!(status, StatusCode::OK);
    let manifest: fafnirx_module_api::Manifest = serde_json::from_str(&body).unwrap();
    assert_eq!((manifest.id.as_str(), manifest.contract), ("video", fafnirx_module_api::CONTRACT));
    // Whatever the manifest promises is served.
    assert!(manifest.has("probe.video") && manifest.has("poster") && manifest.has("status"));
    assert_eq!(get(&f, "/status", T).await.0, StatusCode::OK);
    assert_eq!(
        manifest.setup.iter().filter(|s| s.contains("cannot be run")).count(),
        2,
        "the programs of this test are not there, and the manifest says so"
    );
    assert_eq!(manifest.ui.map(|u| u.entry), Some("index.js".to_string()), "it has built its screens");
}

#[tokio::test]
async fn the_screens_are_served_to_whoever_has_the_token() {
    let f = fixture();
    assert_eq!(get(&f, "/ui/index.js", None).await.0, StatusCode::UNAUTHORIZED);
    let (status, body) = get(&f, "/ui/index.js", T).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "export default {}"));
    for missing in ["/ui/nope.js", "/ui/.env", "/ui/%2e%2e", "/ui/a%2fb"] {
        assert_eq!(get(&f, missing, T).await.0, StatusCode::NOT_FOUND, "{missing}");
    }
}

#[tokio::test]
async fn what_only_the_master_asks_for_is_not_under_files() {
    let f = fixture();
    for private in ["status", "manifest", "probe", "poster"] {
        let uri = format!("/files/{private}?p=pc/clip.mkv&h={H}&max=100");
        assert_eq!(get(&f, &uri, T).await.0, StatusCode::NOT_FOUND, "{private}");
    }
}

#[tokio::test]
async fn probes_and_posters_need_a_file_in_the_tree() {
    let f = fixture();
    let q = format!("p=pc/clip.mkv&h={H}");
    // ffprobe and ffmpeg are not there in this test: asking for the details of a file that is gets as far as them.
    assert_eq!(get(&f, &format!("/probe?{q}"), T).await.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(get(&f, &format!("/poster?{q}&max=320"), T).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(!f.cache.join("posters").read_dir().unwrap().any(|_| true), "no half-made picture is left");
    for (uri, want) in [
        (format!("/probe?p=pc/missing.mkv&h={H}"), StatusCode::NOT_FOUND),
        (format!("/probe?p=../clip.mkv&h={H}"), StatusCode::BAD_REQUEST),
        (format!("/poster?p=pc/missing.mkv&h={H}&max=320"), StatusCode::NOT_FOUND),
        (format!("/poster?{q}&max=0"), StatusCode::BAD_REQUEST),
        (format!("/poster?{q}&max=100000"), StatusCode::BAD_REQUEST),
        (format!("/poster?{q}"), StatusCode::BAD_REQUEST),
    ] {
        assert_eq!(get(&f, &uri, T).await.0, want, "{uri}");
    }
}

#[tokio::test]
async fn requests_cannot_leave_the_file_tree() {
    let f = fixture();
    for p in [
        "../etc/passwd",
        "pc/../../x",
        "pc/./clip.mkv",
        "/pc/clip.mkv",
        "pc//clip.mkv",
        "clip.mkv",
        "pc%5Cclip.mkv",
        "c:/x",
    ] {
        let (status, _) = get(&f, &format!("/files/hls/master.m3u8?p={p}&h={H}"), T).await;
        assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND, "{p}: {status}");
    }
    assert_eq!(get(&f, "/files/hls/master.m3u8?p=pc/clip.mkv&h=nothex", T).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(get(&f, &format!("/files/hls/master.m3u8?p=pc/missing.mkv&h={H}"), T).await.0, StatusCode::NOT_FOUND);
    assert_eq!(get(&f, "/files/hls/master.m3u8", T).await.0, StatusCode::BAD_REQUEST, "p and h are needed");
}

#[tokio::test]
async fn playlists_come_without_transcoding_anything() {
    let f = fixture();
    probed(&f);
    let q = format!("p=pc/clip.mkv&h={H}");
    let (status, master) = get(&f, &format!("/files/hls/master.m3u8?{q}"), T).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        master.contains("720p/index.m3u8") && master.contains("360p/index.m3u8") && !master.contains("1080p"),
        "{master}"
    );
    let (status, media) = get(&f, &format!("/files/hls/720p/index.m3u8?{q}"), T).await;
    assert_eq!(status, StatusCode::OK);
    assert!(media.contains("seg2.ts") && !media.contains("seg3.ts"), "{media}");
    assert_eq!(
        get(&f, &format!("/files/hls/1080p/index.m3u8?{q}"), T).await.0,
        StatusCode::NOT_FOUND,
        "a quality it was not offered"
    );
    assert_eq!(get(&f, &format!("/files/hls/720p/seg3.ts?{q}"), T).await.0, StatusCode::NOT_FOUND, "past the end");
    assert_eq!(get(&f, &format!("/files/hls/720p/seg1.mp4?{q}"), T).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_segment_that_cannot_be_made_is_an_error_and_leaves_nothing_behind() {
    let f = fixture();
    probed(&f);
    let q = format!("p=pc/clip.mkv&h={H}");
    let (status, body) = get(&f, &format!("/files/hls/720p/seg0.ts?{q}"), T).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let leftovers: Vec<_> = std::fs::read_dir(f.cache.join(cache_key("pc/clip.mkv", H)))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("seg"))
        .collect();
    assert!(leftovers.is_empty());
}

#[tokio::test]
async fn a_segment_already_made_is_served_as_it_is() {
    let f = fixture();
    probed(&f);
    let dir = f.cache.join(cache_key("pc/clip.mkv", H));
    std::fs::write(dir.join("720p-seg1.ts"), b"TS-BYTES").unwrap();
    let resp = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/files/hls/720p/seg1.ts?p=pc/clip.mkv&h={H}"))
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[header::CONTENT_TYPE], "video/mp2t");
    assert_eq!(resp.into_body().collect().await.unwrap().to_bytes().as_ref(), b"TS-BYTES");
}

#[tokio::test]
async fn a_video_that_cannot_be_probed_is_not_a_video() {
    let f = fixture();
    let (status, _) = get(&f, &format!("/files/hls/master.m3u8?p=pc/clip.mkv&h={H}"), T).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[test]
fn changed_content_has_its_own_cache_folder() {
    assert_ne!(cache_key("pc/a.mkv", "aa"), cache_key("pc/a.mkv", "bb"));
    assert_ne!(cache_key("pc/a.mkv", "aa"), cache_key("pc/b.mkv", "aa"));
    assert_eq!(cache_key("pc/a.mkv", "aa").len(), 32);
}

#[test]
fn idle_cache_folders_are_swept_and_used_ones_kept() {
    let f = fixture();
    std::fs::create_dir_all(f.cache.join("old")).unwrap();
    std::fs::write(f.cache.join("old/.used"), b"").unwrap();
    std::fs::create_dir_all(f.cache.join("new")).unwrap();
    std::fs::write(f.cache.join("new/.used"), b"").unwrap();
    let old = std::fs::File::options().write(true).open(f.cache.join("old/.used")).unwrap();
    old.set_modified(std::time::SystemTime::now() - Duration::from_secs(10 * 86400)).unwrap();
    assert_eq!(sweep(&f.cache, Duration::from_secs(7 * 86400)), 1);
    assert!(!f.cache.join("old").exists() && f.cache.join("new").exists());
    drop(f.data);
}
