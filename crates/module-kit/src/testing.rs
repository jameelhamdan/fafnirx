//! For the tests of a module (feature `testing`): its router with a data folder of its own, one request to it, a small website to read
//! from and a stand-in for a program.

use crate::{router, Common, Fetcher};
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::response::IntoResponse;
use axum::Router;
use http_body_util::BodyExt;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

/// The token the master of a test sends.
pub const TOKEN: &str = "secret-token-0123456789";
/// The idle time of a test module.
pub const IDLE: Duration = Duration::from_secs(20);

/// A module's service over a data folder with its `tmp` folder in it.
pub struct Fixture {
    pub data: tempfile::TempDir,
    pub app: Router,
}

impl Fixture {
    /// The module `make` builds (it is given the data folder), served as the master would reach it.
    pub fn new<M: Fetcher>(make: impl FnOnce(&Path) -> M) -> Self {
        let data = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(data.path().join("tmp")).unwrap();
        let common = Common {
            data: data.path().to_path_buf(),
            token: TOKEN.into(),
            jobs: 2,
            idle: IDLE,
            ui: None,
            listen: "127.0.0.1:0".parse().unwrap(),
            announce: None,
        };
        let app = router(&common, make(data.path()));
        Self { data, app }
    }

    /// A request with the master's token.
    pub async fn call(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, String) {
        call(&self.app, method, uri, Some(TOKEN), body).await
    }

    /// Where the master would have the module write the file `name`.
    pub fn dest(&self, name: &str) -> std::path::PathBuf {
        self.data.path().join("tmp").join(name)
    }

    /// How many files and folders are in the master's `tmp` folder.
    pub fn tmp_count(&self) -> usize {
        std::fs::read_dir(self.data.path().join("tmp")).unwrap().count()
    }
}

/// Sends `body` as JSON (when there is one) with the bearer `token` (when there is one) and gives the status and the text of the answer.
pub async fn call(app: &Router, method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, String) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let body = match body {
        Some(b) => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    (status, String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned())
}

/// A port on this machine, and the address (`http://127.0.0.1:<port>`) a website served on it will have: bound first, so that the
/// pages can name their own address.
pub async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    (listener, base)
}

/// Serves `files` (path, body) on `listener`; any other path is a 404.
pub fn serve_files(listener: tokio::net::TcpListener, files: Vec<(&'static str, Vec<u8>)>) {
    let files = Arc::new(files);
    let app = Router::new().fallback(move |uri: axum::http::Uri| {
        let files = files.clone();
        async move {
            match files.iter().find(|(p, _)| *p == uri.path()) {
                Some((_, body)) => body.clone().into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    });
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
}

/// An executable shell script `dir/name`, a stand-in for a program the module runs.
#[cfg(unix)]
pub fn write_script(dir: &Path, name: &str, script: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}
