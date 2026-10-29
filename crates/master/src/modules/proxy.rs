//! What the web app asks of a module: `GET /api/m/<module>/<file id>/<rest>` becomes `GET /files/<rest>?p=…&h=…` at the
//! module. The master puts its login in front (the file must be one the user can see) and names the file the way modules
//! do. Nothing outside `/files/` of a module is reachable this way: its manifest, probes and reports are for the master.

use crate::platform::id::Id;
use crate::repo::files;
use crate::web::access::Access;
use crate::web::{ApiError, Shared};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Extension;

/// The parts of a module's address: short names, so that the address cannot lead anywhere else.
fn safe_rest(rest: &str) -> bool {
    let ok = |part: &str| {
        !part.is_empty()
            && part.len() <= 64
            && !part.starts_with('.')
            && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
    };
    let parts: Vec<&str> = rest.split('/').collect();
    (1..=4).contains(&parts.len()) && parts.into_iter().all(ok)
}

/// What a module needs to know about a file: where it is in the file tree and which content it holds.
fn locate(c: &mut postgres::Client, who: &Access, id: uuid::Uuid) -> anyhow::Result<(String, String)> {
    let (folder, path, root) = files::location(c, id)?.ok_or_else(|| ApiError::not_found("No such file"))?;
    // A file of someone else's device does not exist for this user.
    who.check_see(&folder).map_err(|_| ApiError::not_found("No such file"))?;
    Ok((crate::platform::store::tree_path(&folder, &path), hex::encode(root)))
}

pub async fn proxy(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path((module, Id(id), rest)): Path<(String, Id, String)>,
) -> Result<Response, ApiError> {
    let module = state.modules.up(&module).ok_or_else(|| ApiError::not_found("No such module"))?;
    if !safe_rest(&rest) {
        return Err(ApiError::not_found("No such resource"));
    }
    let (p, h) = crate::web::run(&state, move |c| locate(c, &who, id)).await?;
    let upstream = module
        .get(&format!("{}/{rest}", fafnirx_module_api::FILES_PREFIX))
        .query(&[("p", p.as_str()), ("h", h.as_str())])
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("module {}: {e}", module.id());
            ApiError(StatusCode::BAD_GATEWAY, format!("The {} module cannot be reached", module.id()))
        })?;
    pass_on(&module, upstream).await
}

/// What the web app asks for of a module's own screens: `GET /api/m/<module>/ui/<file>` becomes `GET /ui/<file>`. The same
/// for every user, so only the login is checked.
pub async fn ui(
    State(state): State<Shared>,
    Path((module, file)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let module = state.modules.up(&module).ok_or_else(|| ApiError::not_found("No such module"))?;
    if !safe_rest(&file) || file.contains('/') {
        return Err(ApiError::not_found("No such resource"));
    }
    let upstream = module.get(&format!("{}/{file}", fafnirx_module_api::UI_PREFIX)).send().await.map_err(|e| {
        tracing::warn!("module {}: {e}", module.id());
        ApiError(StatusCode::BAD_GATEWAY, format!("The {} module cannot be reached", module.id()))
    })?;
    pass_on(&module, upstream).await
}

/// The module's answer as the web app gets it, or the module's own words about why not.
async fn pass_on(module: &super::Module, upstream: reqwest::Response) -> Result<Response, ApiError> {
    let status = upstream.status();
    if !status.is_success() {
        // Say what the module said (it names the problem: not a video, ffmpeg failed), but never its token or paths.
        let message = upstream.text().await.unwrap_or_default();
        let (code, message) = if status == StatusCode::UNAUTHORIZED {
            (StatusCode::BAD_GATEWAY, format!("The {} module refused the master's token", module.id()))
        } else {
            (status, message.chars().take(300).collect())
        };
        return Err(ApiError(code, message));
    }
    let kept: Vec<(header::HeaderName, HeaderValue)> = [header::CONTENT_TYPE, header::CACHE_CONTROL]
        .into_iter()
        .filter_map(|name| upstream.headers().get(&name).cloned().map(|v| (name, v)))
        .collect();
    let mut resp = Response::new(Body::from_stream(upstream.bytes_stream()));
    resp.headers_mut().extend(kept);
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::safe_rest;

    #[test]
    fn only_short_plain_names_are_passed_on() {
        for ok in ["hls/master.m3u8", "hls/720p/index.m3u8", "hls/480p/seg0.ts", "hls/subtitles/12.vtt", "a-b_c.d"] {
            assert!(safe_rest(ok), "{ok}");
        }
        for bad in [
            "",
            "/",
            "..",
            "../probe",
            "hls/../master.m3u8",
            ".hidden",
            "hls//index.m3u8",
            "hls/index.m3u8/",
            "a/b/c/d/e",
            "a b",
            "seg1.ts?x=1",
            "hls\\index.m3u8",
            "hls/%2e%2e",
            "é",
        ] {
            assert!(!safe_rest(bad), "{bad:?}");
        }
    }
}
