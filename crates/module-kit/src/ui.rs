//! The module's part of the web app: the script and style the master passes on to the browser (`GET /ui/<file>`, see
//! `fafnirx_module_api::UI_PREFIX`). They are built with the web app's module build and kept in `FAFNIRX_PLUGIN_UI`.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use fafnirx_module_api::{Ui, WEB_SDK};
use std::path::Path;

const ENTRY: &str = "index.js";
const STYLE: &str = "style.css";

/// What the manifest says of the module's screens: nothing when there is no built script to serve.
pub fn describe(dir: Option<&Path>) -> Option<Ui> {
    let dir = dir?;
    dir.join(ENTRY)
        .is_file()
        .then(|| Ui { entry: ENTRY.into(), css: dir.join(STYLE).is_file().then(|| STYLE.into()), sdk: WEB_SDK })
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
    !name.is_empty() && name.len() <= 128 && !name.starts_with('.') && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The file `name` of the web part in `dir`, or the status and words to answer with.
pub async fn file(dir: Option<&Path>, name: &str) -> Result<Response, (StatusCode, &'static str)> {
    let dir = dir.ok_or((StatusCode::NOT_FOUND, "this module has no screens"))?;
    if !plain(name) {
        return Err((StatusCode::NOT_FOUND, "no such file"));
    }
    let bytes = tokio::fs::read(dir.join(name)).await.map_err(|_| (StatusCode::NOT_FOUND, "no such file"))?;
    let mut response = Response::new(bytes.into());
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(name)));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600"));
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_names_are_served() {
        for ok in ["index.js", "queue-3fa9.js", "style.css", "a_b-c.js.map"] {
            assert!(plain(ok), "{ok}");
        }
        for bad in ["", ".env", "..", "a/b", "a\\b", "a b", "é.js", &"a".repeat(129)] {
            assert!(!plain(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_manifest_names_what_was_built() {
        assert_eq!(describe(None), None);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(describe(Some(dir.path())), None, "nothing built");
        std::fs::write(dir.path().join("index.js"), "export default {}").unwrap();
        assert_eq!(describe(Some(dir.path())).unwrap().css, None);
        std::fs::write(dir.path().join("style.css"), "").unwrap();
        let ui = describe(Some(dir.path())).unwrap();
        assert_eq!((ui.entry.as_str(), ui.css.as_deref(), ui.sdk), ("index.js", Some("style.css"), WEB_SDK));
    }

    #[tokio::test]
    async fn a_built_file_is_served_with_its_type_and_the_rest_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.js"), "export default {}").unwrap();
        let ok = file(Some(dir.path()), "index.js").await.unwrap();
        assert_eq!(ok.headers()[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
        for (given, name) in [(Some(dir.path()), "missing.js"), (Some(dir.path()), "../x"), (None, "index.js")] {
            assert_eq!(file(given, name).await.unwrap_err().0, StatusCode::NOT_FOUND, "{name}");
        }
    }
}
