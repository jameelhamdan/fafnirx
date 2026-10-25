//! The API end to end, through the real router: login, a local folder, a chunked upload, browsing,
//! downloading (whole, ranged, zipped), history and delete, and what each role may reach.

use super::{router, AppState};
use crate::platform::db::testing;
use crate::platform::store::Store;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::response::IntoResponse;
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;

/// The database pool must be made and dropped outside the runtime, so the tests are plain functions.
fn block<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(f)
}

struct Api {
    app: Router,
    db: crate::platform::db::Db,
    #[allow(dead_code)]
    store: Arc<Store>,
    cookie: String,
    _dir: tempfile::TempDir,
}

impl Api {
    fn new() -> Self {
        Self::with_stream(None)
    }

    fn with_stream(stream: Option<crate::library::stream::Backend>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let db = testing::db();
        super::auth::bootstrap(&mut *db.get().unwrap(), "secret").unwrap();
        let mut state = AppState::new(db.clone(), store.clone(), dir.path().to_path_buf()).unwrap();
        state.stream = stream;
        let state = Arc::new(state);
        let app = router(state.clone()).layer(MockConnectInfo("127.0.0.1:1".parse::<SocketAddr>().unwrap()));
        Self { app, db, store, cookie: String::new(), _dir: dir }
    }
    /// What the minute job does: the totals shown for devices catch up with their files.
    async fn catch_up_totals(&self) {
        crate::platform::db::with(&self.db, crate::devices::totals::refresh_stale).await.unwrap();
    }

    async fn send(
        &self,
        method: Method,
        uri: &str,
        body: Body,
        content_type: Option<&str>,
        extra: &[(&str, &str)],
    ) -> (StatusCode, header::HeaderMap, Vec<u8>) {
        let mut req = Request::builder().method(method).uri(uri).header("x-requested-with", "test");
        if !self.cookie.is_empty() {
            req = req.header(header::COOKIE, &self.cookie);
        }
        if let Some(t) = content_type {
            req = req.header(header::CONTENT_TYPE, t);
        }
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let resp = self.app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let (status, headers) = (resp.status(), resp.headers().clone());
        (status, headers, resp.into_body().collect().await.unwrap().to_bytes().to_vec())
    }

    /// The status of an empty JSON request made with another session.
    async fn cookie_for(&self, cookie: &str, method: Method, uri: String) -> StatusCode {
        let mut req = Request::builder().method(method).uri(uri).header("x-requested-with", "test");
        if !cookie.is_empty() {
            req = req.header(header::COOKIE, cookie);
        }
        let req = req.header(header::CONTENT_TYPE, "application/json").body(Body::from("{}")).unwrap();
        self.app.clone().oneshot(req).await.unwrap().status()
    }

    async fn json(&self, method: Method, uri: &str, body: Value) -> (StatusCode, Value) {
        let (status, _, bytes) =
            self.send(method, uri, Body::from(body.to_string()), Some("application/json"), &[]).await;
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let (status, _, bytes) = self.send(Method::GET, uri, Body::empty(), None, &[]).await;
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn upload(&self, device: &str, path: &str, data: &[u8]) {
        let (status, started) =
            self.json(Method::POST, "/api/uploads", json!({"device": device, "path": path, "size": data.len()})).await;
        assert_eq!(status, StatusCode::OK, "{started}");
        let id = started["id"].as_str().unwrap();
        let mut offset = 0;
        for chunk in data.chunks(7) {
            let (status, _, _) = self
                .send(Method::PUT, &format!("/api/uploads/{id}?offset={offset}"), Body::from(chunk.to_vec()), None, &[])
                .await;
            assert_eq!(status, StatusCode::OK);
            offset += chunk.len();
        }
        let (status, done) = self.json(Method::POST, &format!("/api/uploads/{id}/finish"), json!({})).await;
        assert_eq!(status, StatusCode::OK, "{done}");
    }
}

#[test]
fn streaming_is_for_whoever_can_see_the_file_and_only_when_a_service_is_set_up() {
    // Not set up: nothing to offer, and nothing is passed on.
    let mut api = Api::new();
    block(async {
        assert_eq!(api.get("/api/stream/1/master.m3u8").await.0, StatusCode::UNAUTHORIZED, "the login comes first");
        api.cookie = login(&api).await;
        assert_eq!(api.get("/api/session").await.1["stream"], false);
        assert_eq!(api.get("/api/stream/1/master.m3u8").await.0, StatusCode::NOT_FOUND);
    });

    // A stand-in for the streaming service: answers only with the right token, and says what it was asked for.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut api = Api::with_stream(Some(crate::library::stream::Backend::new(&url, "stream-token")));
    block(async {
        let service = Router::new().route(
            "/hls/*rest",
            axum::routing::get(
                |axum::extract::Path(rest): axum::extract::Path<String>,
                 axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
                 headers: axum::http::HeaderMap| async move {
                    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer stream-token") {
                        return (StatusCode::UNAUTHORIZED, "no".to_string()).into_response();
                    }
                    if rest.ends_with("seg9.ts") {
                        return (StatusCode::NOT_FOUND, "no such segment".to_string()).into_response();
                    }
                    let body = format!("#EXTM3U {rest} p={} h={}", q["p"], q["h"]);
                    ([(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")], body).into_response()
                },
            ),
        );
        tokio::spawn(async move {
            axum::serve(tokio::net::TcpListener::from_std(listener).unwrap(), service).await.unwrap();
        });

        assert_eq!(api.get("/api/stream/1/master.m3u8").await.0, StatusCode::UNAUTHORIZED);
        api.cookie = login(&api).await;
        assert_eq!(api.get("/api/session").await.1["stream"], true);
        api.json(Method::POST, "/api/devices", json!({"name": "videos", "kind": "local"})).await;
        api.upload("admin/videos", "trip/clip.mkv", b"video bytes").await;
        let id: i64 = crate::platform::db::with(&api.db, |c| {
            Ok(c.query_one("SELECT id FROM files WHERE path = 'trip/clip.mkv'", &[])?.get(0))
        })
        .await
        .unwrap();

        let (status, headers, body) =
            api.send(Method::GET, &format!("/api/stream/{id}/720p/index.m3u8"), Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "application/vnd.apple.mpegurl");
        let text = String::from_utf8(body).unwrap();
        assert!(
            text.contains("720p/index.m3u8 p=admin/videos/trip/clip.mkv h="),
            "the service is told which file and content: {text}"
        );
        assert!(!text.contains("stream-token"));

        let (status, error) = api.get(&format!("/api/stream/{id}/720p/seg9.ts")).await;
        assert_eq!(
            (status, error["error"].as_str()),
            (StatusCode::NOT_FOUND, Some("no such segment")),
            "its answer is passed on"
        );
        for bad in ["../master.m3u8", "720p/seg1.mp4", "other"] {
            assert_eq!(api.get(&format!("/api/stream/{id}/{bad}")).await.0, StatusCode::NOT_FOUND, "{bad}");
        }
        assert_eq!(api.get("/api/stream/99999/master.m3u8").await.0, StatusCode::NOT_FOUND, "no such file");

        api.json(
            Method::POST,
            "/api/files/delete",
            json!({"items": [{"device": "admin/videos", "path": "trip/clip.mkv"}]}),
        )
        .await;
        assert_eq!(
            api.get(&format!("/api/stream/{id}/master.m3u8")).await.0,
            StatusCode::NOT_FOUND,
            "a deleted file is not streamed"
        );
    });
}

#[test]
fn login_is_required_and_wrong_passwords_are_refused() {
    let mut api = Api::new();
    block(async {
        assert_eq!(api.get("/api/dashboard").await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(api.get("/api/session").await.1["user"], Value::Null);
        assert_eq!(
            api.json(Method::POST, "/api/login", json!({"username": "admin", "password": "nope"})).await.0,
            StatusCode::UNAUTHORIZED
        );

        let (status, headers, _) = api
            .send(
                Method::POST,
                "/api/login",
                Body::from(json!({"username": "admin", "password": "secret"}).to_string()),
                Some("application/json"),
                &[],
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        api.cookie = headers[header::SET_COOKIE].to_str().unwrap().split(';').next().unwrap().to_string();
        assert_eq!(api.get("/api/session").await.1["user"]["role"], "admin");
        let (status, dash) = api.get("/api/dashboard").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(dash["totals"]["files"], 0);

        // A changing call without the marker header is refused even with a valid cookie.
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/devices")
            .header(header::COOKIE, &api.cookie)
            .body(Body::empty())
            .unwrap();
        assert_eq!(api.app.clone().oneshot(req).await.unwrap().status(), StatusCode::FORBIDDEN);

        // Logging out ends the session in the database, so the cookie is no good afterwards.
        let (status, _, _) = api.send(Method::POST, "/api/logout", Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(api.get("/api/dashboard").await.0, StatusCode::UNAUTHORIZED);

        // Wrong guesses are counted per user name as well as per address, and the right password is refused meanwhile.
        for _ in 0..5 {
            assert_eq!(login_as(&api, "bob", "wrong").await, "");
        }
        let blocked = api.json(Method::POST, "/api/login", json!({"username": "bob", "password": "wrong"})).await;
        assert_eq!(blocked.0, StatusCode::TOO_MANY_REQUESTS);
    });
}

#[test]
fn every_response_carries_the_security_headers() {
    let api = Api::new();
    block(async {
        let (status, page, _) = api.send(Method::GET, "/healthz", Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK, "the health check needs no login");
        let csp = page[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(csp.contains("script-src 'self'") && csp.contains("frame-ancestors 'none'"), "{csp}");
        assert!(csp.contains("https://tile.openstreetmap.org"), "the map tiles may be loaded: {csp}");
        assert_eq!(page[header::X_FRAME_OPTIONS], "DENY");
        assert_eq!(page[header::X_CONTENT_TYPE_OPTIONS], "nosniff");

        let (_, app_page, _) = api.send(Method::GET, "/timeline", Body::empty(), None, &[]).await;
        assert!(
            app_page[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("script-src 'self'"),
            "also on the web app pages"
        );
        assert!(page.get(header::STRICT_TRANSPORT_SECURITY).is_none(), "plain http: no HSTS");

        let (status, refused, _) = api.send(Method::GET, "/api/dashboard", Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(refused[header::X_FRAME_OPTIONS], "DENY", "also on refusals");
        let (_, behind_tls, _) =
            api.send(Method::GET, "/healthz", Body::empty(), None, &[("x-forwarded-proto", "https")]).await;
        assert!(behind_tls.get(header::STRICT_TRANSPORT_SECURITY).is_some());
    });
}

#[test]
fn devices_are_slugs_and_tokens_are_shown_once() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        assert_eq!(
            api.json(Method::POST, "/api/devices", json!({"name": "Bad Name"})).await.0,
            StatusCode::BAD_REQUEST
        );
        let (status, made) = api.json(Method::POST, "/api/devices", json!({"name": " Laptop "})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(made["device"]["name"], "laptop");
        assert!(made["token"].as_str().unwrap().len() >= 40);
        assert_eq!(api.json(Method::POST, "/api/devices", json!({"name": "laptop"})).await.0, StatusCode::CONFLICT);
        let (_, again) = api.get("/api/devices/admin%2Flaptop").await;
        assert!(again["token"].is_null(), "the token is never shown again");
        let (_, rotated) = api.json(Method::POST, "/api/devices/admin%2Flaptop/token", json!({})).await;
        assert_ne!(rotated["token"], made["token"]);
        assert_eq!(api.json(Method::POST, "/api/devices/admin%2Flaptop/revoke", json!({})).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/devices/admin%2Flaptop").await.1["device"]["revoked"], true);
    });
}

#[test]
fn what_the_admin_sets_up_stays_in_the_database_and_the_file_is_not_touched() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        // A mirror needs a remote of the rclone config or a backend.
        assert_eq!(api.json(Method::POST, "/api/mirrors", json!({"name": "m"})).await.0, StatusCode::BAD_REQUEST);
        let mirror = json!({"name": "vault", "remote": "crypt:backups/fafnirx", "keep_runs": 5});
        assert_eq!(api.json(Method::POST, "/api/mirrors", mirror).await.0, StatusCode::OK);
        let direct =
            json!({"name": "offsite", "remote_type": "S3", "options": "region=eu\nprovider=AWS", "remote_path": "b/x"});
        assert_eq!(api.json(Method::POST, "/api/mirrors", direct).await.0, StatusCode::OK);
        api.json(Method::PUT, "/api/settings", json!({"version_count": 9, "retention_days": 45})).await;

        let (_, mirrors) = api.get("/api/mirrors").await;
        assert_eq!(mirrors.as_array().unwrap().len(), 2);
        assert_eq!(mirrors[1]["detail"]["option_keys"], json!([]), "the mirror of the rclone config has no options");
        assert_eq!(
            mirrors[0]["detail"]["option_keys"],
            json!(["provider", "region"]),
            "credentials are never sent back"
        );
        let (_, settings) = api.get("/api/settings").await;
        assert_eq!((settings["version_count"].clone(), settings["retention_days"].clone()), (json!(9), json!(45)));
    });
}

async fn login(api: &Api) -> String {
    login_as(api, "admin", "secret").await
}

/// The session cookie of a user ("" when the login is refused).
async fn login_as(api: &Api, username: &str, password: &str) -> String {
    let (status, headers, _) = api
        .send(
            Method::POST,
            "/api/login",
            Body::from(json!({"username": username, "password": password}).to_string()),
            Some("application/json"),
            &[],
        )
        .await;
    if status != StatusCode::OK {
        return String::new();
    }
    headers[header::SET_COOKIE].to_str().unwrap().split(';').next().unwrap().to_string()
}

#[test]
fn upload_browse_download_history_and_delete() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        // Uploads go to local folders only.
        api.json(Method::POST, "/api/devices", json!({"name": "pc"})).await;
        let (status, _) =
            api.json(Method::POST, "/api/uploads", json!({"device": "admin/pc", "path": "a.txt", "size": 1})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            api.json(Method::POST, "/api/devices", json!({"name": "photos", "kind": "local"})).await.0,
            StatusCode::OK
        );

        api.upload("admin/photos", "2024/a.txt", b"hello world").await;
        api.upload("admin/photos", "2024/b.txt", b"second").await;
        api.upload("admin/photos", "top.txt", b"top").await;

        let (_, before) = api.get("/api/files").await;
        assert!(
            before["entries"].as_array().unwrap().iter().any(|e| e["name"] == "photos" && e["files"] == 0),
            "the totals of a device are not live"
        );
        api.catch_up_totals().await;
        let (_, root) = api.get("/api/files").await;
        assert!(root["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "device" && e["name"] == "photos" && e["files"] == 3));
        let (_, top) = api.get("/api/files?device=admin/photos").await;
        let kinds: Vec<_> = top["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["kind"].as_str().unwrap(), e["name"].as_str().unwrap()))
            .collect();
        assert_eq!(kinds, [("dir", "2024"), ("file", "top.txt")]);
        let (_, year) = api.get("/api/files?device=admin/photos&path=2024&sort=size&order=desc").await;
        let names: Vec<_> = year["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["a.txt", "b.txt"], "largest first");
        let (_, found) = api.get("/api/files?device=admin/photos&q=B.TX").await;
        assert_eq!(found["entries"][0]["path"], "2024/b.txt");
        assert_eq!(api.get("/api/files?device=admin/photos&sort=bogus").await.0, StatusCode::BAD_REQUEST);

        let (status, headers, body) = api
            .send(Method::GET, "/api/files/download?device=admin/photos&path=2024/a.txt", Body::empty(), None, &[])
            .await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"hello world"[..]));
        assert!(headers[header::CONTENT_DISPOSITION].to_str().unwrap().starts_with("attachment"));
        let (status, headers, body) = api
            .send(
                Method::GET,
                "/api/files/download?device=admin/photos&path=2024/a.txt",
                Body::empty(),
                None,
                &[("range", "bytes=6-")],
            )
            .await;
        assert_eq!((status, body.as_slice()), (StatusCode::PARTIAL_CONTENT, &b"world"[..]));
        assert_eq!(headers[header::CONTENT_RANGE], "bytes 6-10/11");

        let form = format!("items={}", urlencode(&json!([{"device": "admin/photos", "path": "2024"}]).to_string()));
        let (status, _, zipped) = api
            .send(Method::POST, "/api/files/zip", Body::from(form), Some("application/x-www-form-urlencoded"), &[])
            .await;
        assert_eq!(status, StatusCode::OK);
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zipped)).unwrap();
        let mut names: Vec<_> = archive.file_names().map(String::from).collect();
        names.sort();
        assert_eq!(names, ["2024/a.txt", "2024/b.txt"], "the zip starts at the selected folder");
        let mut text = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("2024/b.txt").unwrap(), &mut text).unwrap();
        assert_eq!(text, "second");

        // A second version, then the history and a restore.
        api.upload("admin/photos", "2024/a.txt", b"hello again").await;
        let (_, info) = api.get("/api/files/info?device=admin/photos&path=2024/a.txt").await;
        let versions = info["versions"].as_array().unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().all(|v| v["available"] == true));
        let old = versions.iter().find(|v| v["current"] == false).unwrap()["id"].as_i64().unwrap();
        let (_, _, body) = api
            .send(
                Method::GET,
                &format!("/api/files/download?device=admin/photos&path=2024/a.txt&version={old}"),
                Body::empty(),
                None,
                &[],
            )
            .await;
        assert_eq!(body, b"hello world");
        assert_eq!(
            api.json(
                Method::POST,
                "/api/files/restore",
                json!({"device": "admin/photos", "path": "2024/a.txt", "version": old})
            )
            .await
            .0,
            StatusCode::OK
        );
        let (_, _, body) = api
            .send(Method::GET, "/api/files/download?device=admin/photos&path=2024/a.txt", Body::empty(), None, &[])
            .await;
        assert_eq!(body, b"hello world", "restored as the newest version");

        // Delete a folder: its files leave the listing but stay downloadable until retention removes them.
        let (_, removed) = api
            .json(Method::POST, "/api/files/delete", json!({"items": [{"device": "admin/photos", "path": "2024"}]}))
            .await;
        assert_eq!(removed["deleted"], 2);
        let (_, after) = api.get("/api/files?device=admin/photos").await;
        assert_eq!(after["entries"].as_array().unwrap().len(), 1);
        let (_, with_deleted) = api.get("/api/files?device=admin/photos&deleted=1").await;
        assert_eq!(with_deleted["entries"].as_array().unwrap().len(), 2);
        let (status, _, body) = api
            .send(Method::GET, "/api/files/download?device=admin/photos&path=2024/b.txt", Body::empty(), None, &[])
            .await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"second"[..]));
    });
}

#[test]
fn an_upload_resumes_where_it_stopped_and_rejects_gaps() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "up", "kind": "local"})).await;
        let (_, started) =
            api.json(Method::POST, "/api/uploads", json!({"device": "admin/up", "path": "f.bin", "size": 10})).await;
        let id = started["id"].as_str().unwrap();
        let put = |offset: u64, data: &'static [u8]| {
            api.send(Method::PUT, format!("/api/uploads/{id}?offset={offset}").leak(), Body::from(data), None, &[])
        };
        assert_eq!(put(0, b"01234").await.0, StatusCode::OK);
        let (status, _, body) = put(7, b"789").await;
        assert_eq!(status, StatusCode::CONFLICT, "a gap is refused");
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["received"],
            5,
            "and the server says where to continue"
        );
        assert_eq!(api.get(&format!("/api/uploads/{id}")).await.1["received"], 5);
        assert_eq!(
            api.json(Method::POST, &format!("/api/uploads/{id}/finish"), json!({})).await.0,
            StatusCode::CONFLICT,
            "incomplete"
        );
        assert_eq!(put(5, b"56789").await.0, StatusCode::OK);
        assert_eq!(api.json(Method::POST, &format!("/api/uploads/{id}/finish"), json!({})).await.0, StatusCode::OK);
    });
}

#[test]
fn an_upload_that_could_not_be_finished_can_be_finished_again() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "up", "kind": "local"})).await;
        assert_eq!(try_upload(&api, "admin/up", "a", b"one").await, StatusCode::OK);
        // `a` is a file now, so nothing can be placed below it.
        let (_, started) =
            api.json(Method::POST, "/api/uploads", json!({"device": "admin/up", "path": "a/b.txt", "size": 3})).await;
        let id = started["id"].as_str().unwrap();
        api.send(Method::PUT, &format!("/api/uploads/{id}?offset=0"), Body::from("two"), None, &[]).await;
        let first = api.json(Method::POST, &format!("/api/uploads/{id}/finish"), json!({})).await;
        assert_ne!(first.0, StatusCode::OK, "{}", first.1);
        let again = api.json(Method::POST, &format!("/api/uploads/{id}/finish"), json!({})).await;
        assert_eq!(again.0, first.0, "the upload is still there to be finished: {}", again.1);
        assert_eq!(api.get(&format!("/api/uploads/{id}")).await.1["received"], 3);
    });
}

fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// A valid PNG of one colour.
fn png(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::new();
    image::RgbImage::from_pixel(width, height, image::Rgb(rgb))
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

/// The id of the library file with this name (in the timeline or the archive).
async fn photo_id(api: &Api, name: &str) -> i64 {
    for scope in ["timeline", "archive"] {
        let (_, buckets) = api.get(&format!("/api/timeline/buckets?scope={scope}")).await;
        for m in buckets["months"].as_array().unwrap() {
            let month = m["month"].as_str().unwrap();
            let (_, list) = api.get(&format!("/api/timeline?month={month}&scope={scope}")).await;
            if let Some(a) = list["assets"].as_array().unwrap().iter().find(|a| a["name"] == name) {
                return a["id"].as_i64().unwrap();
            }
        }
    }
    panic!("no photo called {name}");
}

#[test]
fn the_library_groups_photos_by_month_and_keeps_what_people_say_about_them() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "photos", "kind": "local"})).await;
        api.upload("admin/photos", "trip/a.png", &png(8, 4, [200, 10, 10])).await;
        api.upload("admin/photos", "b.png", &png(4, 4, [0, 200, 0])).await;
        api.upload("admin/photos", "notes.txt", b"not a photo").await;

        // Only photos and videos are in the library; with no camera date, the file's own date is used.
        let (_, buckets) = api.get("/api/timeline/buckets").await;
        assert_eq!(buckets["total"], 2);
        let this_month = buckets["months"][0]["month"].as_str().unwrap().to_string();
        assert_eq!(buckets["months"][0]["days"], json!([2]), "both were added today: one day of two files");
        assert_eq!(api.get("/api/timeline").await.0, StatusCode::BAD_REQUEST, "a month is needed");
        assert_eq!(api.get("/api/timeline?month=soon").await.0, StatusCode::BAD_REQUEST);
        assert_eq!(api.get("/api/timeline/buckets?scope=bogus").await.0, StatusCode::BAD_REQUEST);
        let (_, listed) = api.get(&format!("/api/timeline?month={this_month}")).await;
        let names: Vec<_> = listed["assets"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"a.png") && names.contains(&"b.png"));
        assert_eq!(listed["assets"][0]["kind"], "image");
        assert_eq!(api.get("/api/timeline/buckets?kind=video").await.1["total"], 0);
        assert_eq!(api.get("/api/timeline/buckets?device=nowhere").await.1["total"], 0);

        let a = photo_id(&api, "a.png").await;
        let b = photo_id(&api, "b.png").await;
        let edit = |id: i64, body: Value| {
            let api = &api;
            async move { api.json(Method::PATCH, &format!("/api/library/{id}"), body).await }
        };

        // Favorite, rating, description and tags (case does not make a new tag).
        let (status, _) = edit(
            a,
            json!({"favorite": true, "rating": 4, "description": " Beach day ", "tags": ["Trip", " trip ", "Summer", ""]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, detail) = api.get(&format!("/api/library/{a}")).await;
        assert_eq!((detail["favorite"].clone(), detail["rating"].clone()), (json!(true), json!(4)));
        assert_eq!(detail["description"], "Beach day");
        let tags: Vec<_> = detail["tags"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(tags, ["Summer", "Trip"]);
        assert_eq!(detail["mime"], "image/png");
        assert_eq!(api.get("/api/timeline/buckets?scope=favorites").await.1["total"], 1);
        assert_eq!(edit(a, json!({"rating": 9})).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(edit(999_999, json!({"favorite": true})).await.0, StatusCode::NOT_FOUND);
        edit(a, json!({"rating": null})).await;
        let detail = api.get(&format!("/api/library/{a}")).await.1;
        assert!(detail["rating"].is_null(), "null clears");
        assert_eq!(detail["favorite"], true, "an absent field leaves things alone");

        let (_, all_tags) = api.get("/api/tags").await;
        assert_eq!(all_tags.as_array().unwrap().len(), 2);
        let trip = all_tags.as_array().unwrap().iter().find(|t| t["name"] == "Trip").unwrap()["id"].as_i64().unwrap();
        assert_eq!(api.get(&format!("/api/timeline/buckets?tag={trip}")).await.1["total"], 1);
        // Dropping a tag from the only file that has it removes it.
        edit(a, json!({"tags": ["Summer"]})).await;
        assert_eq!(api.get("/api/tags").await.1.as_array().unwrap().len(), 1);

        // Bulk: favorite both, then archive one: the timeline no longer shows it, the archive does.
        let (_, changed) = api
            .json(Method::POST, "/api/library/bulk", json!({"ids": [a, b], "favorite": true, "add_tags": ["Bulk"]}))
            .await;
        assert_eq!(changed["changed"], 2);
        assert_eq!(api.get("/api/timeline/buckets?scope=favorites").await.1["total"], 2);
        edit(a, json!({"archived": true})).await;
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 1);
        assert_eq!(api.get("/api/timeline/buckets?scope=archive").await.1["total"], 1);
        assert_eq!(api.json(Method::POST, "/api/library/bulk", json!({"ids": []})).await.0, StatusCode::BAD_REQUEST);
        edit(a, json!({"archived": false})).await;

        // A corrected date moves the photo to another month and can be taken back.
        edit(a, json!({"taken_at": 1_700_000_000})).await;
        let (_, moved) = api.get("/api/timeline?month=2023-11").await;
        assert_eq!(moved["assets"][0]["when"], 1_700_000_000);
        assert_eq!(api.get(&format!("/api/library/{a}")).await.1["when_edited"], true);
        assert_eq!(api.get("/api/timeline/buckets").await.1["months"].as_array().unwrap().len(), 2);
        edit(a, json!({"taken_at": null})).await;
        assert_eq!(api.get("/api/timeline/buckets").await.1["months"].as_array().unwrap().len(), 1);

        // A corrected place shows on the map; nonsense places and the "null island" are refused.
        assert_eq!(api.get("/api/map").await.1["points"].as_array().unwrap().len(), 0);
        let place = |lat: f64, lon: f64| json!({"location": {"lat": lat, "lon": lon}});
        assert_eq!(edit(a, place(95.0, 0.0)).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(edit(a, place(0.0, 0.0)).await.0, StatusCode::BAD_REQUEST);
        edit(a, place(52.5, 13.4)).await;
        let (_, map) = api.get("/api/map").await;
        assert_eq!(json!([map["points"][0][0], map["points"][0][1], map["points"][0][2]]), json!([a, 52.5, 13.4]));
        assert_eq!(api.get(&format!("/api/library/{a}")).await.1["place_edited"], true);
        edit(a, json!({"location": null})).await;
        assert_eq!(api.get("/api/map").await.1["points"].as_array().unwrap().len(), 0);

        // Pictures and originals by id.
        let (status, headers, thumb) =
            api.send(Method::GET, &format!("/api/media/{a}/thumb"), Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(image::load_from_memory(&thumb).unwrap().width(), 8);
        let (_, _, original) =
            api.send(Method::GET, &format!("/api/media/{a}/original"), Body::empty(), None, &[]).await;
        assert_eq!(original, png(8, 4, [200, 10, 10]));
        let bogus = api.send(Method::GET, &format!("/api/media/{a}/bogus"), Body::empty(), None, &[]).await;
        assert_eq!(bogus.0, StatusCode::NOT_FOUND);

        // Deleting moves it to the trash (its picture still works there); restoring brings it back with its state.
        api.json(
            Method::POST,
            "/api/files/delete",
            json!({"items": [{"device": "admin/photos", "path": "trip/a.png"}]}),
        )
        .await;
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 1);
        let (_, trash) = api.get("/api/trash").await;
        assert_eq!(trash["total"], 1);
        assert_eq!(trash["entries"][0]["name"], "a.png");
        assert_eq!(trash["entries"][0]["kind"], "image");
        assert!(
            trash["entries"][0]["purge_at"].as_i64().unwrap() > trash["entries"][0]["deleted_at"].as_i64().unwrap()
        );
        let in_trash = api.send(Method::GET, &format!("/api/media/{a}/thumb"), Body::empty(), None, &[]).await;
        assert_eq!(in_trash.0, StatusCode::OK);
        assert_eq!(edit(a, json!({"favorite": false})).await.0, StatusCode::NOT_FOUND);
        let version = trash["entries"][0]["version"].clone();
        let (status, _) = api
            .json(
                Method::POST,
                "/api/files/restore",
                json!({"device": "admin/photos", "path": "trip/a.png", "version": version}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(api.get("/api/trash").await.1["total"], 0);
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 2);
        assert_eq!(
            api.get(&format!("/api/library/{a}")).await.1["favorite"],
            true,
            "what was said comes back with the file"
        );
    });
}

#[test]
fn a_folder_can_be_an_album_and_files_are_added_from_the_file_browser() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "tv", "kind": "local"})).await;
        for (path, shade) in [("Show/S1/Episode 10.png", 1), ("Show/S1/Episode 2.png", 2), ("Other/x.png", 3)] {
            api.upload("admin/tv", path, &png(4, 4, [shade, shade, shade])).await;
        }
        let folder = json!({"device": "admin/tv", "path": "Show"});

        // The folder is the album: its name is the folder's, and what is below it is in it, in the order of the names.
        let (status, album) = api.json(Method::POST, "/api/albums", json!({"name": "", "folder": folder})).await;
        assert_eq!(status, StatusCode::OK, "{album}");
        let id = album["id"].as_i64().unwrap();
        assert_eq!(
            (album["name"].clone(), album["count"].clone(), album["folder"].clone()),
            (json!("Show"), json!(2), folder.clone())
        );
        let names = |detail: &Value| -> Vec<String> {
            detail["assets"].as_array().unwrap().iter().map(|a| a["path"].as_str().unwrap().to_string()).collect()
        };
        let (_, detail) = api.get(&format!("/api/albums/{id}")).await;
        assert_eq!(
            names(&detail),
            ["Show/S1/Episode 2.png", "Show/S1/Episode 10.png"],
            "Episode 2 comes before Episode 10"
        );

        // Asking again gives the same album, and a new file in the folder is in it without doing anything.
        let (_, again) = api.json(Method::POST, "/api/albums", json!({"name": "", "folder": folder})).await;
        assert_eq!(again["id"].as_i64(), Some(id));
        api.upload("admin/tv", "Show/S1/Episode 3.png", &png(4, 4, [9, 9, 9])).await;
        let (_, detail) = api.get(&format!("/api/albums/{id}")).await;
        assert_eq!(
            names(&detail),
            ["Show/S1/Episode 2.png", "Show/S1/Episode 3.png", "Show/S1/Episode 10.png"],
            "a file that comes later is in the album"
        );
        assert_eq!(api.get("/api/albums").await.1[0]["count"], 3);

        // A folder that is not there for this user cannot be an album.
        let nobody = json!({"device": "nobody/none", "path": ""});
        assert_eq!(
            api.json(Method::POST, "/api/albums", json!({"name": "x", "folder": nobody})).await.0,
            StatusCode::NOT_FOUND
        );

        // From the file browser: files and folders by where they are, without ids.
        let (_, trip) = api.json(Method::POST, "/api/albums", json!({"name": "Trip"})).await;
        let trip_id = trip["id"].as_i64().unwrap();
        let items =
            json!({"items": [{"device": "admin/tv", "path": "Other/x.png"}, {"device": "admin/tv", "path": "Show"}]});
        let (_, added) = api.json(Method::POST, &format!("/api/albums/{trip_id}/files"), items).await;
        assert_eq!(added["count"], 4, "the file, and the three below the folder");
        let (_, detail) = api.get(&format!("/api/albums/{trip_id}")).await;
        assert_eq!(detail["album"]["count"], 4);
        assert!(detail["album"]["folder"].is_null());
    });
}

#[test]
fn albums_collect_files_and_leave_the_files_alone() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "photos", "kind": "local"})).await;
        api.upload("admin/photos", "a.png", &png(8, 4, [1, 1, 1])).await;
        api.upload("admin/photos", "b.png", &png(4, 4, [2, 2, 2])).await;
        api.upload("admin/photos", "c.png", &png(2, 2, [3, 3, 3])).await;
        let (a, b, c) = (photo_id(&api, "a.png").await, photo_id(&api, "b.png").await, photo_id(&api, "c.png").await);

        assert_eq!(api.json(Method::POST, "/api/albums", json!({"name": "  "})).await.0, StatusCode::BAD_REQUEST);
        let (status, album) = api.json(Method::POST, "/api/albums", json!({"name": " Trip ", "ids": [a]})).await;
        assert_eq!(status, StatusCode::OK);
        let id = album["id"].as_i64().unwrap();
        assert_eq!(
            (album["name"].clone(), album["count"].clone(), album["cover"].clone()),
            (json!("Trip"), json!(1), json!(a))
        );

        let (_, added) = api.json(Method::POST, &format!("/api/albums/{id}/files"), json!({"ids": [a, b]})).await;
        assert_eq!(added["count"], 1, "a is already in it");
        let (_, detail) = api.get(&format!("/api/albums/{id}")).await;
        assert_eq!(detail["album"]["count"], 2);
        assert_eq!(detail["assets"].as_array().unwrap().len(), 2);
        assert_eq!(api.get(&format!("/api/library/{a}")).await.1["albums"][0]["name"], "Trip");

        let not_member = api.json(Method::PATCH, &format!("/api/albums/{id}"), json!({"cover": c})).await;
        assert_eq!(not_member.0, StatusCode::BAD_REQUEST, "c is not in it");
        let (_, edited) = api
            .json(
                Method::PATCH,
                &format!("/api/albums/{id}"),
                json!({"name": "Trip 2024", "cover": b, "description": "x"}),
            )
            .await;
        assert_eq!((edited["name"].clone(), edited["cover"].clone()), (json!("Trip 2024"), json!(b)));

        let (_, removed) = api.json(Method::POST, &format!("/api/albums/{id}/files/remove"), json!({"ids": [b]})).await;
        assert_eq!(removed["count"], 1);
        let (_, after) = api.get("/api/albums").await;
        assert_eq!(after[0]["count"], 1);
        assert_eq!(after[0]["cover"], a, "the cover that left is replaced by the newest file");

        // Deleting a file takes it out of the album's count; deleting the album leaves the photos.
        api.json(Method::POST, "/api/files/delete", json!({"items": [{"device": "admin/photos", "path": "a.png"}]}))
            .await;
        assert_eq!(api.get("/api/albums").await.1[0]["count"], 0);
        let version = api.get("/api/trash").await.1["entries"][0]["version"].clone();
        let restored = api
            .json(
                Method::POST,
                "/api/files/restore",
                json!({"device": "admin/photos", "path": "a.png", "version": version}),
            )
            .await;
        assert_eq!(restored.0, StatusCode::OK);
        assert_eq!(api.get("/api/albums").await.1[0]["count"], 1, "and it is back in the album when restored");
        assert_eq!(
            api.send(Method::DELETE, &format!("/api/albums/{id}"), Body::empty(), None, &[]).await.0,
            StatusCode::OK
        );
        assert_eq!(api.get(&format!("/api/albums/{id}")).await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 3);
    });
}

/// Makes a user as the administrator and returns their session cookie. The API is left logged in as the administrator.
async fn add_user(api: &mut Api, name: &str, role: &str) -> String {
    api.cookie = login(api).await;
    let made =
        api.json(Method::POST, "/api/users", json!({"name": name, "role": role, "password": "password-1"})).await;
    assert_eq!(made.0, StatusCode::OK, "{}", made.1);
    let cookie = login_as(api, name, "password-1").await;
    assert!(!cookie.is_empty(), "{name} can log in");
    cookie
}

/// Uploads a file to a device as whoever is logged in; the status of the start and the finish.
async fn try_upload(api: &Api, device: &str, path: &str, data: &[u8]) -> StatusCode {
    let (status, started) =
        api.json(Method::POST, "/api/uploads", json!({"device": device, "path": path, "size": data.len()})).await;
    if status != StatusCode::OK {
        return status;
    }
    let id = started["id"].as_str().unwrap();
    api.send(Method::PUT, &format!("/api/uploads/{id}?offset=0"), Body::from(data.to_vec()), None, &[]).await;
    api.json(Method::POST, &format!("/api/uploads/{id}/finish"), json!({})).await.0
}

#[test]
fn users_have_roles_and_the_last_administrator_stays() {
    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        let admin = api.cookie.clone();

        // Names are slugs, passwords have a length, and a user exists once.
        let weak = json!({"name": "dave", "role": "normal", "password": "short"});
        assert_eq!(api.json(Method::POST, "/api/users", weak).await.0, StatusCode::BAD_REQUEST);
        let bad_name = json!({"name": "Dave Smith", "role": "normal", "password": "password-1"});
        assert_eq!(api.json(Method::POST, "/api/users", bad_name).await.0, StatusCode::BAD_REQUEST);
        let no_role = json!({"name": "dave", "role": "boss", "password": "password-1"});
        assert_eq!(api.json(Method::POST, "/api/users", no_role).await.0, StatusCode::BAD_REQUEST);
        let twice = json!({"name": "bob", "role": "normal", "password": "password-1"});
        assert_eq!(api.json(Method::POST, "/api/users", twice).await.0, StatusCode::CONFLICT);
        assert!(api._dir.path().join("files/bob").is_dir(), "the folder of the user exists from the start");

        // Only an administrator manages users, settings and mirrors.
        api.cookie = bob.clone();
        for uri in ["/api/users", "/api/settings", "/api/mirrors"] {
            assert_eq!(api.get(uri).await.0, StatusCode::FORBIDDEN, "{uri}");
        }
        // Only an administrator asks for a rescan of the file store.
        assert_eq!(api.json(Method::POST, "/api/files/rescan", json!({})).await.0, StatusCode::FORBIDDEN);
        // Sources are per user: bob has his own, which nobody but an administrator sees.
        let source = |name: &str| {
            json!({"name": name, "folders": "abcdefghij0123456789", "service_account": json!({"client_email": "a@b.iam", "private_key": "-----BEGIN PRIVATE KEY-----
x
-----END PRIVATE KEY-----
"}).to_string()})
        };
        assert_eq!(api.json(Method::POST, "/api/sources", source("bobs")).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/sources").await.1.as_array().unwrap().len(), 1);
        api.cookie = admin.clone();
        assert_eq!(api.json(Method::POST, "/api/sources", source("roots")).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/sources").await.1.as_array().unwrap().len(), 2, "an administrator sees all");
        api.cookie = bob.clone();
        assert_eq!(api.get("/api/sources").await.1.as_array().unwrap().len(), 1);
        assert_eq!(api.json(Method::POST, "/api/sources/admin/roots/toggle", json!({})).await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.json(Method::POST, "/api/sources/bob/bobs/toggle", json!({})).await.0, StatusCode::OK);

        // A source name is only unique per user: the administrator can use bob's name, bob cannot use it twice.
        assert_eq!(api.json(Method::POST, "/api/sources", source("bobs")).await.0, StatusCode::CONFLICT);
        api.cookie = admin.clone();
        assert_eq!(api.json(Method::POST, "/api/files/rescan", json!({})).await.0, StatusCode::OK);
        assert_eq!(api.json(Method::POST, "/api/sources", source("bobs")).await.0, StatusCode::OK);
        let (_, all) = api.get("/api/sources").await;
        let keys: Vec<&str> = all.as_array().unwrap().iter().map(|s| s["key"].as_str().unwrap()).collect();
        assert_eq!(keys, ["admin/bobs", "admin/roots", "bob/bobs"], "named by the user and the name");
        // The administrator can reach bob's source, and bob's route does not reach the administrator's.
        assert_eq!(api.json(Method::POST, "/api/sources/bob/bobs/toggle", json!({})).await.0, StatusCode::OK);
        api.cookie = bob.clone();
        assert_eq!(api.json(Method::POST, "/api/sources/admin/bobs/toggle", json!({})).await.0, StatusCode::NOT_FOUND);

        // What goes wrong with a source is for its owner (and the administrators), not for everyone.
        crate::platform::db::with(&api.db, |c| {
            crate::platform::db::upsert_alert(c, "source_failed", "bob/source-bobs", "Drive said no")?;
            crate::platform::db::upsert_alert(c, "source_failed", "admin/source-roots", "Drive said no")?;
            Ok(())
        })
        .await
        .unwrap();
        let (_, board) = api.get("/api/dashboard").await;
        let keys: Vec<&str> = board["alerts"].as_array().unwrap().iter().map(|a| a["key"].as_str().unwrap()).collect();
        assert_eq!(keys, ["bob/source-bobs"], "bob is told about his source only");
        api.cookie = admin.clone();
        assert_eq!(api.get("/api/dashboard").await.1["alerts"].as_array().unwrap().len(), 2);
        api.cookie = bob.clone();
        let (_, session) = api.get("/api/session").await;
        assert_eq!((session["user"]["name"].clone(), session["user"]["role"].clone()), (json!("bob"), json!("normal")));

        // The last administrator can be neither demoted nor disabled; with another one, either is fine.
        api.cookie = admin.clone();
        assert_eq!(
            api.json(Method::PATCH, "/api/users/admin", json!({"role": "normal"})).await.0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            api.json(Method::PATCH, "/api/users/admin", json!({"disabled": true})).await.0,
            StatusCode::CONFLICT
        );
        let second = add_user(&mut api, "root2", "admin").await;
        api.cookie = admin.clone();
        assert_eq!(api.json(Method::PATCH, "/api/users/root2", json!({"disabled": true})).await.0, StatusCode::OK);
        api.cookie = second;
        assert_eq!(api.get("/api/session").await.1["user"], Value::Null, "disabling ends the sessions at once");
        assert!(login_as(&api, "root2", "password-1").await.is_empty(), "and the user cannot log in");
        api.cookie = admin.clone();
        assert_eq!(api.json(Method::PATCH, "/api/users/root2", json!({"disabled": false})).await.0, StatusCode::OK);
        assert!(!login_as(&api, "root2", "password-1").await.is_empty());
        assert_eq!(
            api.json(Method::PATCH, "/api/users/nobody", json!({"role": "normal"})).await.0,
            StatusCode::NOT_FOUND
        );

        // Changing the role ends the sessions too, so that the new rights are the ones in force.
        assert_eq!(api.json(Method::PATCH, "/api/users/admin", json!({"role": "normal"})).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/users").await.0, StatusCode::UNAUTHORIZED);

        // A user changing their own password keeps the session they do it in; the others end.
        let other = login_as(&api, "bob", "password-1").await;
        api.cookie = bob;
        let wrong = json!({"current": "not it", "new": "password-2"});
        assert_eq!(api.json(Method::POST, "/api/me/password", wrong).await.0, StatusCode::BAD_REQUEST);
        let changed = json!({"current": "password-1", "new": "password-2"});
        assert_eq!(api.json(Method::POST, "/api/me/password", changed).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/session").await.1["user"]["name"], "bob", "this session stays");
        api.cookie = other;
        assert_eq!(api.get("/api/session").await.1["user"], Value::Null, "the other one ends");
        assert!(login_as(&api, "bob", "password-1").await.is_empty());
        assert!(!login_as(&api, "bob", "password-2").await.is_empty());
    });
}

#[test]
fn names_in_any_script_are_listed_downloaded_and_found() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        assert_eq!(
            api.json(Method::POST, "/api/devices", json!({"name": "d", "kind": "local"})).await.0,
            StatusCode::OK
        );
        // A folder in Arabic, a name with an accent written decomposed (as macOS does) and one in Japanese, at depth.
        let files = ["اناشيد/ألبوم الطفل/ملف.txt", "cafe\u{301}/menu.txt", "日本語/写真/猫.txt"];
        for path in files {
            assert_eq!(try_upload(&api, "admin/d", path, path.as_bytes()).await, StatusCode::OK, "{path}");
        }
        let names = |listing: &Value| -> Vec<String> {
            listing["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_string()).collect()
        };
        let (_, top) = api.get("/api/files?device=admin%2Fd").await;
        let mut top = names(&top);
        top.sort();
        assert_eq!(top, ["caf\u{e9}", "اناشيد", "日本語"], "names are whole, and stored composed");
        let (_, inner) = api.get("/api/files?device=admin%2Fd&path=%D8%A7%D9%86%D8%A7%D8%B4%D9%8A%D8%AF").await;
        assert_eq!(names(&inner), ["ألبوم الطفل"]);
        let (_, deep) = api
            .get("/api/files?device=admin%2Fd&path=%D8%A7%D9%86%D8%A7%D8%B4%D9%8A%D8%AF%2F%D8%A3%D9%84%D8%A8%D9%88%D9%85%20%D8%A7%D9%84%D8%B7%D9%81%D9%84")
            .await;
        assert_eq!(names(&deep), ["ملف.txt"]);
        let uri = "/api/files/download?device=admin%2Fd&path=%E6%97%A5%E6%9C%AC%E8%AA%9E%2F%E5%86%99%E7%9C%9F%2F%E7%8C%AB.txt";
        let (status, headers, body) = api.send(Method::GET, uri, Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "日本語/写真/猫.txt".as_bytes());
        let disposition = headers.get("content-disposition").unwrap().to_str().unwrap();
        assert!(disposition.contains("filename*=UTF-8''%E7%8C%AB.txt"), "{disposition}");
        let items = json!([{"device": "admin/d", "path": "اناشيد"}]).to_string();
        let form = format!("items={}&name={}", urlencode(&items), urlencode("اناشيد"));
        let (status, headers, zipped) = api
            .send(Method::POST, "/api/files/zip", Body::from(form), Some("application/x-www-form-urlencoded"), &[])
            .await;
        assert_eq!(status, StatusCode::OK);
        let disposition = headers.get("content-disposition").unwrap().to_str().unwrap();
        assert!(disposition.contains("filename*=UTF-8''%D8%A7%D9%86%D8%A7%D8%B4%D9%8A%D8%AF.zip"), "{disposition}");
        let archive = zip::ZipArchive::new(std::io::Cursor::new(zipped)).unwrap();
        assert_eq!(archive.file_names().collect::<Vec<_>>(), ["اناشيد/ألبوم الطفل/ملف.txt"]);
    });
}

#[test]
fn every_user_has_a_workspace_and_the_master_files_belong_to_everyone() {
    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        let alice = add_user(&mut api, "alice", "normal").await;
        let carol = add_user(&mut api, "carol", "readonly").await;
        let admin = login(&api).await;

        // The administrator puts a master file in place; nobody owns it.
        api.cookie = admin.clone();
        assert_eq!(try_upload(&api, "_master", "docs/readme.txt", b"for everyone").await, StatusCode::OK);
        assert!(api._dir.path().join("files/docs/readme.txt").is_file(), "master files are at the root of files/");
        let ghost = json!({"name": "a1", "user": "ghost"});
        assert_eq!(api.json(Method::POST, "/api/devices", ghost).await.0, StatusCode::BAD_REQUEST);
        let for_carol = json!({"name": "a1", "user": "carol"});
        assert_eq!(
            api.json(Method::POST, "/api/devices", for_carol).await.0,
            StatusCode::BAD_REQUEST,
            "a user who can only read has no devices"
        );

        // The same device name for two users is two devices, each in the folder of its user.
        for (who, cookie) in [("bob", &bob), ("alice", &alice)] {
            api.cookie = cookie.clone();
            let (status, made) =
                api.json(Method::POST, "/api/devices", json!({"name": "phone", "kind": "local"})).await;
            assert_eq!(status, StatusCode::OK, "{made}");
            assert_eq!(made["device"]["folder"], format!("{who}/phone"));
            assert!(api._dir.path().join(format!("files/{who}/phone")).is_dir());
            assert_eq!(api.json(Method::POST, "/api/devices", json!({"name": "phone"})).await.0, StatusCode::CONFLICT);
            assert_eq!(try_upload(&api, &format!("{who}/phone"), "me.txt", who.as_bytes()).await, StatusCode::OK);
        }

        // Each sees their own devices and the master files, and nothing of the other's.
        api.cookie = bob.clone();
        let (_, devices) = api.get("/api/devices").await;
        assert_eq!(devices.as_array().unwrap().len(), 1);
        assert_eq!(devices[0]["folder"], "bob/phone");
        let (_, top) = api.get("/api/files").await;
        let names: Vec<_> = top["entries"].as_array().unwrap().iter().map(|e| e["device"].as_str().unwrap()).collect();
        assert_eq!(names, ["_master", "bob/phone"]);
        // What the web app may change is told with each entry: the master files and his local folder, both his to write.
        let kinds: Vec<_> = top["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["device_kind"].as_str().unwrap(), e["writable"].as_bool().unwrap()))
            .collect();
        assert_eq!(kinds, [("master", true), ("local", true)]);
        let (_, inside) = api.get("/api/files?device=bob/phone").await;
        assert_eq!((inside["device_kind"].clone(), inside["writable"].clone()), (json!("local"), json!(true)));
        assert_eq!(api.get("/api/files?device=alice/phone").await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.get("/api/files/info?device=alice/phone&path=me.txt").await.0, StatusCode::NOT_FOUND);
        let (status, _, _) =
            api.send(Method::GET, "/api/files/download?device=alice/phone&path=me.txt", Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(api.get("/api/devices/alice%2Fphone").await.0, StatusCode::NOT_FOUND);
        let revoke = api.json(Method::POST, "/api/devices/alice%2Fphone/revoke", json!({})).await;
        assert_eq!(revoke.0, StatusCode::NOT_FOUND);
        assert_eq!(
            try_upload(&api, "alice/phone", "x.txt", b"x").await,
            StatusCode::NOT_FOUND,
            "no writing in another workspace"
        );
        let delete = json!({"items": [{"device": "alice/phone", "path": "me.txt"}]});
        assert_eq!(api.json(Method::POST, "/api/files/delete", delete).await.0, StatusCode::NOT_FOUND);
        let (_, body) = api.get("/api/files?device=_master&path=docs").await;
        assert_eq!(body["entries"][0]["name"], "readme.txt");
        // A normal user writes to the master files too, but not under the name of a user.
        assert_eq!(try_upload(&api, "_master", "docs/from-bob.txt", b"hi").await, StatusCode::OK);
        assert_eq!(
            try_upload(&api, "_master", "alice/sneaky.txt", b"no").await,
            StatusCode::CONFLICT,
            "alice is a user's folder"
        );
        let (_, dash) = api.get("/api/dashboard").await;
        assert_eq!(dash["devices"].as_array().unwrap().len(), 1);
        assert_eq!(dash["mirrors"], json!([]));

        // Read-only: sees the master files, has nothing of its own and changes nothing.
        api.cookie = carol.clone();
        let (_, top) = api.get("/api/files").await;
        let names: Vec<_> = top["entries"].as_array().unwrap().iter().map(|e| e["device"].as_str().unwrap()).collect();
        assert_eq!(names, ["_master"]);
        assert_eq!(api.get("/api/devices").await.1, json!([]));
        assert_eq!(api.get("/api/sources").await.0, StatusCode::FORBIDDEN, "no workspace, no sources");
        let (status, _, body) = api
            .send(Method::GET, "/api/files/download?device=_master&path=docs/readme.txt", Body::empty(), None, &[])
            .await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"for everyone"[..]));
        assert_eq!(api.get("/api/files?device=bob/phone").await.0, StatusCode::NOT_FOUND);
        assert_eq!(try_upload(&api, "_master", "docs/x.txt", b"x").await, StatusCode::FORBIDDEN);
        let own = json!({"name": "mine", "kind": "local"});
        assert_eq!(api.json(Method::POST, "/api/devices", own).await.0, StatusCode::FORBIDDEN);
        let delete = json!({"items": [{"device": "_master", "path": "docs/readme.txt"}]});
        assert_eq!(api.json(Method::POST, "/api/files/delete", delete).await.0, StatusCode::FORBIDDEN);
        assert_eq!(api.json(Method::POST, "/api/albums", json!({"name": "mine"})).await.0, StatusCode::FORBIDDEN);

        // The administrator reaches everything.
        api.cookie = admin;
        let (_, devices) = api.get("/api/devices").await;
        let folders: Vec<_> = devices.as_array().unwrap().iter().map(|d| d["folder"].as_str().unwrap()).collect();
        assert_eq!(folders, ["alice/phone", "bob/phone"]);
        let (status, _, body) =
            api.send(Method::GET, "/api/files/download?device=bob/phone&path=me.txt", Body::empty(), None, &[]).await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"bob"[..]));
        assert_eq!(try_upload(&api, "alice/phone", "from-admin.txt", b"x").await, StatusCode::OK);
        let (status, made) = api.json(Method::POST, "/api/devices", json!({"name": "tablet", "user": "bob"})).await;
        assert_eq!((status, made["device"]["folder"].clone()), (StatusCode::OK, json!("bob/tablet")));

        // A user's name cannot be one the master files use.
        let docs = json!({"name": "docs", "role": "normal", "password": "password-1"});
        assert_eq!(
            api.json(Method::POST, "/api/users", docs).await.0,
            StatusCode::CONFLICT,
            "the master has a docs folder"
        );
    });
}

#[test]
fn what_people_say_about_a_master_file_is_their_own() {
    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        let alice = add_user(&mut api, "alice", "normal").await;
        let carol = add_user(&mut api, "carol", "readonly").await;
        api.cookie = login(&api).await;
        assert_eq!(try_upload(&api, "_master", "shared/pic.png", &png(8, 4, [9, 9, 9])).await, StatusCode::OK);

        api.cookie = bob.clone();
        let id = photo_id(&api, "pic.png").await;
        let edit = json!({"favorite": true, "description": "bob's view", "tags": ["Holiday"]});
        assert_eq!(api.json(Method::PATCH, &format!("/api/library/{id}"), edit).await.0, StatusCode::OK);
        let (status, album) = api.json(Method::POST, "/api/albums", json!({"name": "Bob's", "ids": [id]})).await;
        assert_eq!((status, album["count"].clone()), (StatusCode::OK, json!(1)));
        let album_id = album["id"].as_i64().unwrap();
        assert_eq!(api.get("/api/timeline/buckets?scope=favorites").await.1["total"], 1);

        // Alice sees the same photo with nothing said about it, and none of Bob's albums or tags.
        api.cookie = alice.clone();
        assert_eq!(photo_id(&api, "pic.png").await, id, "one file");
        let detail = api.get(&format!("/api/library/{id}")).await.1;
        let said = (detail["favorite"].clone(), detail["description"].clone(), detail["tags"].clone());
        assert_eq!(said, (json!(false), json!(""), json!([])));
        assert_eq!(api.get("/api/timeline/buckets?scope=favorites").await.1["total"], 0);
        assert_eq!(api.get("/api/albums").await.1, json!([]));
        assert_eq!(api.get(&format!("/api/albums/{album_id}")).await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.get("/api/tags").await.1, json!([]));
        let del = api.send(Method::DELETE, &format!("/api/albums/{album_id}"), Body::empty(), None, &[]).await;
        assert_eq!(del.0, StatusCode::NOT_FOUND);
        api.json(Method::PATCH, &format!("/api/library/{id}"), json!({"tags": ["Holiday"], "rating": 2})).await;
        let mine = api.get(&format!("/api/library/{id}")).await.1;
        assert_eq!((mine["rating"].clone(), mine["tags"][0]["name"].clone()), (json!(2), json!("Holiday")));
        api.json(Method::PATCH, &format!("/api/library/{id}"), json!({"tags": []})).await;

        // Bob's tag is still there after Alice took hers off, and his state is as he left it.
        api.cookie = bob;
        let theirs = api.get(&format!("/api/library/{id}")).await.1;
        let said = (theirs["favorite"].clone(), theirs["rating"].clone(), theirs["tags"][0]["name"].clone());
        assert_eq!(said, (json!(true), Value::Null, json!("Holiday")));

        // Read-only users see the photo and cannot say anything about it.
        api.cookie = carol;
        assert_eq!(photo_id(&api, "pic.png").await, id);
        let fav = json!({"favorite": true});
        assert_eq!(api.json(Method::PATCH, &format!("/api/library/{id}"), fav).await.0, StatusCode::FORBIDDEN);
        let bulk = json!({"ids": [id], "favorite": true});
        assert_eq!(api.json(Method::POST, "/api/library/bulk", bulk).await.0, StatusCode::FORBIDDEN);
        let (status, _, _) =
            api.send(Method::GET, &format!("/api/media/{id}/original"), Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
    });
}

#[test]
fn a_device_can_be_released_for_another_computer() {
    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        api.cookie = login(&api).await;
        let admin = api.cookie.clone();
        let (status, made) = api.json(Method::POST, "/api/devices", json!({"name": "pc"})).await;
        assert_eq!(status, StatusCode::OK, "{made}");
        let hold = |api: &Api| {
            let db = api.db.clone();
            async move {
                crate::platform::db::with(&db, |c| {
                    c.execute("UPDATE devices SET instance_id = 'abc' WHERE name = 'pc'", &[])?;
                    Ok(())
                })
                .await
                .unwrap();
            }
        };
        let held = |api: &Api| {
            let db = api.db.clone();
            async move {
                crate::platform::db::with(&db, |c| {
                    Ok(c.query_one("SELECT instance_id FROM devices WHERE name = 'pc'", &[])?
                        .get::<_, Option<String>>(0))
                })
                .await
                .unwrap()
            }
        };
        hold(&api).await;
        api.cookie = bob;
        assert_eq!(api.json(Method::POST, "/api/devices/admin%2Fpc/release", json!({})).await.0, StatusCode::NOT_FOUND);
        assert_eq!(held(&api).await.as_deref(), Some("abc"), "someone else's device is not theirs to release");
        api.cookie = admin;
        assert_eq!(api.json(Method::POST, "/api/devices/admin%2Fpc/release", json!({})).await.0, StatusCode::OK);
        assert_eq!(held(&api).await, None);
        // A new token is for a new computer: it frees the device too.
        hold(&api).await;
        assert_eq!(api.json(Method::POST, "/api/devices/admin%2Fpc/token", json!({})).await.0, StatusCode::OK);
        assert_eq!(held(&api).await, None);
        assert_eq!(
            api.json(Method::POST, "/api/devices/admin%2Fnope/release", json!({})).await.0,
            StatusCode::NOT_FOUND
        );
    });
}

/// The access rule of every route. A route added to the router without a line here fails the test below, so that
/// nobody ships an endpoint whose rule was never decided.
#[derive(Clone, Copy, PartialEq)]
enum Rule {
    /// Anyone, even without a login.
    Open,
    /// Every logged-in user, the read-only ones too (what they see is limited by the folders they may read).
    Reader,
    /// Users with a workspace of their own: not the read-only role.
    Workspace,
    /// Administrators only.
    Admin,
}

const ROUTES: &[(&str, &str, Rule)] = &[
    ("POST", "/api/login", Rule::Open),
    ("GET", "/healthz", Rule::Open),
    ("GET", "/api/session", Rule::Open),
    ("POST", "/api/logout", Rule::Reader),
    ("POST", "/api/me/password", Rule::Reader),
    ("GET", "/api/users", Rule::Admin),
    ("POST", "/api/users", Rule::Admin),
    ("PATCH", "/api/users/:name", Rule::Admin),
    ("GET", "/api/settings", Rule::Admin),
    ("PUT", "/api/settings", Rule::Admin),
    ("GET", "/api/mirrors", Rule::Admin),
    ("POST", "/api/mirrors", Rule::Admin),
    ("DELETE", "/api/mirrors/:name", Rule::Admin),
    ("POST", "/api/mirrors/:name/run", Rule::Admin),
    ("POST", "/api/mirrors/:name/toggle", Rule::Admin),
    ("POST", "/api/mirrors/:name/resync", Rule::Admin),
    ("POST", "/api/files/rescan", Rule::Admin),
    ("GET", "/api/sources", Rule::Workspace),
    ("POST", "/api/sources", Rule::Workspace),
    ("DELETE", "/api/sources/:user/:name", Rule::Workspace),
    ("POST", "/api/sources/:user/:name/run", Rule::Workspace),
    ("POST", "/api/sources/:user/:name/toggle", Rule::Workspace),
    ("POST", "/api/devices", Rule::Workspace),
    ("POST", "/api/albums", Rule::Workspace),
    ("POST", "/api/files/delete", Rule::Workspace),
    ("POST", "/api/files/restore", Rule::Workspace),
    ("POST", "/api/uploads", Rule::Workspace),
    ("GET", "/api/dashboard", Rule::Reader),
    ("GET", "/api/events", Rule::Reader),
    ("GET", "/api/devices", Rule::Reader),
    ("GET", "/api/devices/:name", Rule::Reader),
    ("POST", "/api/devices/:name/token", Rule::Reader),
    ("POST", "/api/devices/:name/release", Rule::Reader),
    ("POST", "/api/devices/:name/revoke", Rule::Reader),
    ("GET", "/api/files", Rule::Reader),
    ("GET", "/api/files/info", Rule::Reader),
    ("GET", "/api/files/download", Rule::Reader),
    ("GET", "/api/files/thumb", Rule::Reader),
    ("POST", "/api/files/zip", Rule::Reader),
    ("GET", "/api/media/:id/:variant", Rule::Reader),
    ("GET", "/api/stream/:id/*rest", Rule::Reader),
    ("GET", "/api/uploads/:id", Rule::Reader),
    ("PUT", "/api/uploads/:id", Rule::Reader),
    ("DELETE", "/api/uploads/:id", Rule::Reader),
    ("POST", "/api/uploads/:id/finish", Rule::Reader),
    ("GET", "/api/timeline/buckets", Rule::Reader),
    ("GET", "/api/timeline", Rule::Reader),
    ("GET", "/api/library", Rule::Reader),
    ("POST", "/api/library/bulk", Rule::Reader),
    ("GET", "/api/library/:id", Rule::Reader),
    ("PATCH", "/api/library/:id", Rule::Reader),
    ("GET", "/api/tags", Rule::Reader),
    ("DELETE", "/api/tags/:id", Rule::Reader),
    ("GET", "/api/map", Rule::Reader),
    ("GET", "/api/trash", Rule::Reader),
    ("GET", "/api/albums", Rule::Reader),
    ("GET", "/api/albums/:id", Rule::Reader),
    ("PATCH", "/api/albums/:id", Rule::Reader),
    ("DELETE", "/api/albums/:id", Rule::Reader),
    ("POST", "/api/albums/:id/files", Rule::Reader),
    ("POST", "/api/albums/:id/files/remove", Rule::Reader),
    ("POST", "/api/alerts/:id/dismiss", Rule::Reader),
];

#[test]
fn every_route_has_a_rule_and_the_rule_is_kept() {
    // Every route of the router is in the table.
    let source = include_str!("mod.rs");
    let known: Vec<String> = ROUTES.iter().map(|(m, p, _)| format!("{m} {p}")).collect();
    let mut seen = 0;
    for part in source.split(".route(").skip(1) {
        let path = part.trim_start().trim_start_matches('"');
        let path = &path[..path.find('"').unwrap()];
        let end = part.find("\n    }").unwrap_or(part.len());
        let handlers = &part[..end];
        for verb in ["get", "post", "put", "patch", "delete"] {
            let call = format!("{verb}(");
            let used = handlers
                .match_indices(&call)
                .any(|(i, _)| handlers[..i].chars().last().is_some_and(|c| !c.is_alphanumeric() && c != '_'));
            if used {
                seen += 1;
                let route = format!("{} {path}", verb.to_uppercase());
                assert!(known.contains(&route), "{route} has no access rule in ROUTES");
            }
        }
    }

    assert_eq!(seen, ROUTES.len(), "the table and the router list the same routes");

    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        let carol = add_user(&mut api, "carol", "readonly").await;
        let admin = login(&api).await;
        let api = &api;
        for (method, uri, rule) in ROUTES {
            if *rule == Rule::Open {
                continue;
            }
            let uri = uri
                .replace(":name", "x")
                .replace(":user", "x")
                .replace(":id", "1")
                .replace(":variant", "thumb")
                .replace("*rest", "a");
            let status = |cookie: String| {
                let uri = uri.clone();
                async move { api.cookie_for(&cookie, Method::from_bytes(method.as_bytes()).unwrap(), uri).await }
            };
            assert_eq!(status(String::new()).await, StatusCode::UNAUTHORIZED, "{method} {uri} without a login");
            if uri == "/api/logout" {
                continue; // it would end the session the next lines use
            }
            let must_refuse: &[(&str, &String)] = match rule {
                Rule::Admin => &[("normal", &bob), ("readonly", &carol)],
                Rule::Workspace => &[("readonly", &carol)],
                _ => &[],
            };
            for (who, cookie) in must_refuse {
                let got = status((*cookie).clone()).await;
                // A request body that does not parse is turned away before the handler (and its rule) runs, which is
                // as good as a refusal; the tests above send valid requests and see the 403.
                let turned_away = *method != "GET" && matches!(got.as_u16(), 400 | 404 | 422);
                assert!(got == StatusCode::FORBIDDEN || turned_away, "{method} {uri} as {who}: {got}");
            }
            // The role that is allowed gets past the rule, whatever the handler then says about the request.
            let allowed = if *rule == Rule::Admin { &admin } else { &bob };
            assert_ne!(status(allowed.clone()).await, StatusCode::FORBIDDEN, "{method} {uri} as its own role");
        }
    });
}
