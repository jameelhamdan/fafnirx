//! The web app (built from `frontend/`) is part of the binary.

use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../frontend/dist"]
struct Assets;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({ "error": "no such endpoint" }))).into_response();
    }
    let wanted = if path.is_empty() { "index.html" } else { path };
    // Files that exist are served as they are; any other page address belongs to the app's router.
    let (name, file) = match Assets::get(wanted) {
        Some(f) => (wanted, f),
        None if !wanted.contains('.') => match Assets::get("index.html") {
            Some(f) => ("index.html", f),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    // Built files have a hash in their name and never change; the page itself must be fetched fresh.
    let cache = if name.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    (
        [(header::CONTENT_TYPE, mime.essence_str().to_string()), (header::CACHE_CONTROL, cache.to_string())],
        file.data.into_owned(),
    )
        .into_response()
}
