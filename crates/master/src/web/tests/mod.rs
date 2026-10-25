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

mod auth;
mod devices;
mod files;
mod library;
mod routes;
mod users;

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
async fn photo_id(api: &Api, name: &str) -> String {
    for scope in ["timeline", "archive"] {
        let (_, buckets) = api.get(&format!("/api/timeline/buckets?scope={scope}")).await;
        for m in buckets["months"].as_array().unwrap() {
            let month = m["month"].as_str().unwrap();
            let (_, list) = api.get(&format!("/api/timeline?month={month}&scope={scope}")).await;
            if let Some(a) = list["assets"].as_array().unwrap().iter().find(|a| a["name"] == name) {
                return a["id"].as_str().unwrap().to_string();
            }
        }
    }
    panic!("no photo called {name}");
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
