//! The admin API end to end, through the real router: login, a local folder, a chunked upload, browsing,
//! downloading (whole, ranged, zipped), history and delete.

use super::{router, AppState};
use crate::platform::db::testing;
use crate::platform::store::Store;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
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
    cookie: String,
    _dir: tempfile::TempDir,
}

impl Api {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let db = testing::db();
        let state = Arc::new(AppState::new(db.clone(), store, dir.path().to_path_buf(), "secret".into()).unwrap());
        let app =
            router(state.clone(), true, true).layer(MockConnectInfo("127.0.0.1:1".parse::<SocketAddr>().unwrap()));
        Self { app, db, cookie: String::new(), _dir: dir }
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
fn login_is_required_and_wrong_passwords_are_refused() {
    let mut api = Api::new();
    block(async {
        assert_eq!(api.get("/api/dashboard").await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(api.get("/api/session").await.1["admin"], false);
        assert_eq!(api.json(Method::POST, "/api/login", json!({"password": "nope"})).await.0, StatusCode::UNAUTHORIZED);

        let (status, headers, _) = api
            .send(
                Method::POST,
                "/api/login",
                Body::from(json!({"password": "secret"}).to_string()),
                Some("application/json"),
                &[],
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        api.cookie = headers[header::SET_COOKIE].to_str().unwrap().split(';').next().unwrap().to_string();
        assert_eq!(api.get("/api/session").await.1["admin"], true);
        let (status, dash) = api.get("/api/dashboard").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(dash["totals"]["files"], 0);

        // A changing call without the marker header is refused even with a valid cookie.
        let (status, _, _) = api.send(Method::POST, "/api/logout", Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/devices")
            .header(header::COOKIE, &api.cookie)
            .body(Body::empty())
            .unwrap();
        assert_eq!(api.app.clone().oneshot(req).await.unwrap().status(), StatusCode::FORBIDDEN);
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
        let (_, again) = api.get("/api/devices/laptop").await;
        assert!(again["token"].is_null(), "the token is never shown again");
        let (_, rotated) = api.json(Method::POST, "/api/devices/laptop/token", json!({})).await;
        assert_ne!(rotated["token"], made["token"]);
        assert_eq!(api.json(Method::POST, "/api/devices/laptop/revoke", json!({})).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/devices/laptop").await.1["device"]["revoked"], true);
    });
}

async fn login(api: &Api) -> String {
    let (_, headers, _) = api
        .send(
            Method::POST,
            "/api/login",
            Body::from(json!({"password": "secret"}).to_string()),
            Some("application/json"),
            &[],
        )
        .await;
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
            api.json(Method::POST, "/api/uploads", json!({"device": "pc", "path": "a.txt", "size": 1})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            api.json(Method::POST, "/api/devices", json!({"name": "photos", "kind": "local"})).await.0,
            StatusCode::OK
        );

        api.upload("photos", "2024/a.txt", b"hello world").await;
        api.upload("photos", "2024/b.txt", b"second").await;
        api.upload("photos", "top.txt", b"top").await;

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
        let (_, top) = api.get("/api/files?device=photos").await;
        let kinds: Vec<_> = top["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["kind"].as_str().unwrap(), e["name"].as_str().unwrap()))
            .collect();
        assert_eq!(kinds, [("dir", "2024"), ("file", "top.txt")]);
        let (_, year) = api.get("/api/files?device=photos&path=2024&sort=size&order=desc").await;
        let names: Vec<_> = year["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["a.txt", "b.txt"], "largest first");
        let (_, found) = api.get("/api/files?device=photos&q=B.TX").await;
        assert_eq!(found["entries"][0]["path"], "2024/b.txt");
        assert_eq!(api.get("/api/files?device=photos&sort=bogus").await.0, StatusCode::BAD_REQUEST);

        let (status, headers, body) =
            api.send(Method::GET, "/api/files/download?device=photos&path=2024/a.txt", Body::empty(), None, &[]).await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"hello world"[..]));
        assert!(headers[header::CONTENT_DISPOSITION].to_str().unwrap().starts_with("attachment"));
        let (status, headers, body) = api
            .send(
                Method::GET,
                "/api/files/download?device=photos&path=2024/a.txt",
                Body::empty(),
                None,
                &[("range", "bytes=6-")],
            )
            .await;
        assert_eq!((status, body.as_slice()), (StatusCode::PARTIAL_CONTENT, &b"world"[..]));
        assert_eq!(headers[header::CONTENT_RANGE], "bytes 6-10/11");

        let form = format!("items={}", urlencode(&json!([{"device": "photos", "path": "2024"}]).to_string()));
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
        api.upload("photos", "2024/a.txt", b"hello again").await;
        let (_, info) = api.get("/api/files/info?device=photos&path=2024/a.txt").await;
        let versions = info["versions"].as_array().unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().all(|v| v["available"] == true));
        let old = versions.iter().find(|v| v["current"] == false).unwrap()["id"].as_i64().unwrap();
        let (_, _, body) = api
            .send(
                Method::GET,
                &format!("/api/files/download?device=photos&path=2024/a.txt&version={old}"),
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
                json!({"device": "photos", "path": "2024/a.txt", "version": old})
            )
            .await
            .0,
            StatusCode::OK
        );
        let (_, _, body) =
            api.send(Method::GET, "/api/files/download?device=photos&path=2024/a.txt", Body::empty(), None, &[]).await;
        assert_eq!(body, b"hello world", "restored as the newest version");

        // Delete a folder: its files leave the listing but stay downloadable until retention removes them.
        let (_, removed) =
            api.json(Method::POST, "/api/files/delete", json!({"items": [{"device": "photos", "path": "2024"}]})).await;
        assert_eq!(removed["deleted"], 2);
        let (_, after) = api.get("/api/files?device=photos").await;
        assert_eq!(after["entries"].as_array().unwrap().len(), 1);
        let (_, with_deleted) = api.get("/api/files?device=photos&deleted=1").await;
        assert_eq!(with_deleted["entries"].as_array().unwrap().len(), 2);
        let (status, _, body) =
            api.send(Method::GET, "/api/files/download?device=photos&path=2024/b.txt", Body::empty(), None, &[]).await;
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
            api.json(Method::POST, "/api/uploads", json!({"device": "up", "path": "f.bin", "size": 10})).await;
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
        api.upload("photos", "trip/a.png", &png(8, 4, [200, 10, 10])).await;
        api.upload("photos", "b.png", &png(4, 4, [0, 200, 0])).await;
        api.upload("photos", "notes.txt", b"not a photo").await;

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
        api.json(Method::POST, "/api/files/delete", json!({"items": [{"device": "photos", "path": "trip/a.png"}]}))
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
                json!({"device": "photos", "path": "trip/a.png", "version": version}),
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
fn albums_collect_files_and_leave_the_files_alone() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "photos", "kind": "local"})).await;
        api.upload("photos", "a.png", &png(8, 4, [1, 1, 1])).await;
        api.upload("photos", "b.png", &png(4, 4, [2, 2, 2])).await;
        api.upload("photos", "c.png", &png(2, 2, [3, 3, 3])).await;
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
        api.json(Method::POST, "/api/files/delete", json!({"items": [{"device": "photos", "path": "a.png"}]})).await;
        assert_eq!(api.get("/api/albums").await.1[0]["count"], 0);
        let version = api.get("/api/trash").await.1["entries"][0]["version"].clone();
        let restored = api
            .json(Method::POST, "/api/files/restore", json!({"device": "photos", "path": "a.png", "version": version}))
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
