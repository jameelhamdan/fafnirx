//! The module's part of the web app: the script and style the master passes on to the browser (`GET /ui/<file>`, see
//! `fafnirx_module_api::UI_PREFIX`). They are built with the web app's module build and kept in `FAFNIRX_PLUGIN_UI`.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use fafnirx_module_api::{Ui, WEB_SDK};
use std::path::Path;

const ENTRY: &str = "index.js";
const STYLE: &str = "style.css";

/// The languages of the `strings.<lang>.json` files in `dir`, sorted.
fn languages(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let lang = name.strip_prefix("strings.")?.strip_suffix(".json")?;
            (!lang.is_empty() && lang.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')).then(|| lang.to_string())
        })
        .collect();
    found.sort();
    found
}

/// What the manifest says of the module's screens: nothing when there is no built script to serve.
pub fn describe(dir: Option<&Path>) -> Option<Ui> {
    let dir = dir?;
    dir.join(ENTRY).is_file().then(|| Ui {
        entry: ENTRY.into(),
        css: dir.join(STYLE).is_file().then(|| STYLE.into()),
        sdk: WEB_SDK,
        strings: languages(dir),
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
#[path = "../tests/unit/ui.rs"]
mod tests;
