//! The module's part of the web app: the script and style the master passes on to the browser (`GET /ui/<file>`, see
//! `fafnirx_module_api::UI_PREFIX`). They are built with the web app's module build and kept in `FAFNIRX_MODULE_UI`.

use crate::service::{fail, App, Failure};
use axum::extract::{Path as UrlPath, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use fafnirx_module_api::{Ui, WEB_SDK};
use std::path::Path;
use std::sync::Arc;

const ENTRY: &str = "index.js";
const STYLE: &str = "style.css";

/// What the manifest says of the module's screens: nothing when there is no built script to serve.
pub(crate) fn describe(dir: Option<&Path>) -> Option<Ui> {
    let dir = dir?;
    dir.join(ENTRY).is_file().then(|| Ui {
        entry: ENTRY.into(),
        css: dir.join(STYLE).is_file().then(|| STYLE.into()),
        sdk: WEB_SDK,
    })
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "map" | "json" => "application/json",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "png" => "image/png",
        _ => "application/octet-stream",
    }
}

/// A plain file name, so that the address cannot lead out of the folder.
fn plain(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

pub(crate) async fn file(State(app): State<Arc<App>>, UrlPath(name): UrlPath<String>) -> Result<Response, Failure> {
    let dir = app.cfg.ui.as_ref().ok_or_else(|| fail(StatusCode::NOT_FOUND, "this module has no screens"))?;
    if !plain(&name) {
        return Err(fail(StatusCode::NOT_FOUND, "no such file"));
    }
    let bytes = tokio::fs::read(dir.join(&name)).await.map_err(|_| fail(StatusCode::NOT_FOUND, "no such file"))?;
    let mut response = Response::new(bytes.into());
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(&name)));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600"));
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_names_are_served() {
        for ok in ["index.js", "viewer-3fa9.js", "style.css", "a_b-c.js.map"] {
            assert!(plain(ok), "{ok}");
        }
        for bad in ["", ".env", "..", "a/b", "a\\b", "a b", "é.js", &"a".repeat(129)] {
            assert!(!plain(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_manifest_names_what_was_built() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(describe(Some(dir.path())), None, "nothing built");
        assert_eq!(describe(None), None);
        std::fs::write(dir.path().join("index.js"), "export default {}").unwrap();
        assert_eq!(describe(Some(dir.path())).unwrap().css, None);
        std::fs::write(dir.path().join("style.css"), "").unwrap();
        let ui = describe(Some(dir.path())).unwrap();
        assert_eq!((ui.entry.as_str(), ui.css.as_deref(), ui.sdk), ("index.js", Some("style.css"), WEB_SDK));
    }
}
