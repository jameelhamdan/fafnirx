//! Adaptive video streaming, for whoever can see the file: the master puts its login in front of the separate
//! `fafnirx-stream` service and passes on its playlists and segments. The browser talks to the master only, so there
//! is one address, one login and nothing to open in a firewall.
//!
//! Set `FAFNIRX_STREAM_URL` (for example `http://stream:8090`) and `FAFNIRX_STREAM_TOKEN` (the token the service
//! was given). Without them streaming is off, and the web app does not offer it.

use crate::web::access::Access;
use crate::web::{ApiError, Shared};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Extension;
use std::time::Duration;

/// How the service is reached. A segment that has to be made first can take a while on a slow machine, hence the long
/// timeout.
pub struct Backend {
    url: String,
    token: String,
    client: reqwest::Client,
}

impl Backend {
    pub fn new(url: &str, token: &str) -> Self {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(120)).build().expect("a client");
        Self { url: url.trim_end_matches('/').to_string(), token: token.to_string(), client }
    }

    pub fn from_env() -> Option<Self> {
        let url = std::env::var("FAFNIRX_STREAM_URL").ok().filter(|u| !u.is_empty())?;
        let token = std::env::var("FAFNIRX_STREAM_TOKEN").ok().filter(|t| !t.is_empty());
        let Some(token) = token else {
            tracing::warn!("FAFNIRX_STREAM_URL is set but FAFNIRX_STREAM_TOKEN is not: streaming stays off");
            return None;
        };
        tracing::info!("video streaming through {url}");
        Some(Self::new(&url, &token))
    }
}

/// `master.m3u8`, `720p/index.m3u8`, `720p/seg3.ts`, `subtitles.json` or `subtitles/2.vtt`: the only things the service
/// serves, and nothing else is passed on. The parts are short names, so the address cannot lead anywhere else.
pub fn valid_resource(rest: &str) -> bool {
    let ok = |part: &str| {
        !part.is_empty()
            && part.len() <= 32
            && part != ".."
            && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
    };
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        ["master.m3u8" | "subtitles.json"] => true,
        ["subtitles", track] => track
            .strip_suffix(".vtt")
            .is_some_and(|n| (1..=4).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit())),
        [quality, "index.m3u8"] => ok(quality),
        [quality, segment] => ok(quality) && ok(segment) && segment.starts_with("seg") && segment.ends_with(".ts"),
        _ => false,
    }
}

/// What the service needs to know about a file: where it is in the file tree and which content it holds.
fn locate(c: &mut postgres::Client, who: &Access, id: i64) -> anyhow::Result<(String, String)> {
    let row = c
        .query_opt(
            "SELECT d.folder, f.path, v.root_hash FROM files f
             JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id
             WHERE f.id = $1 AND f.deleted_at IS NULL",
            &[&id],
        )?
        .ok_or_else(|| ApiError::not_found("No such file"))?;
    let (folder, path): (String, String) = (row.get(0), row.get(1));
    // A file of someone else's device does not exist for this user.
    who.check_see(&folder).map_err(|_| ApiError::not_found("No such file"))?;
    let root: Vec<u8> = row.get(2);
    // The master files are at the root of the file tree, the files of a device below the folder of the device.
    let located = if folder == crate::platform::store::MASTER_FOLDER { path } else { format!("{folder}/{path}") };
    Ok((located, hex::encode(root)))
}

pub async fn proxy(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path((id, rest)): Path<(i64, String)>,
) -> Result<Response, ApiError> {
    let backend = state.stream.as_ref().ok_or_else(|| ApiError::not_found("Streaming is not set up"))?;
    if !valid_resource(&rest) {
        return Err(ApiError::not_found("No such stream"));
    }
    let (p, h) = crate::web::run(&state, move |c| locate(c, &who, id)).await?;
    let upstream = backend
        .client
        .get(format!("{}/hls/{rest}", backend.url))
        .query(&[("p", p.as_str()), ("h", h.as_str())])
        .bearer_auth(&backend.token)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("streaming service: {e}");
            ApiError(StatusCode::BAD_GATEWAY, "The streaming service cannot be reached".into())
        })?;

    let status = upstream.status();
    if !status.is_success() {
        // Say what the service said (it names the problem: not a video, ffmpeg failed), but never its token or paths.
        let message = upstream.text().await.unwrap_or_default();
        let code = if status == StatusCode::UNAUTHORIZED { StatusCode::BAD_GATEWAY } else { status };
        let message = if status == StatusCode::UNAUTHORIZED {
            "The streaming service refused the master's token".to_string()
        } else {
            message.chars().take(300).collect()
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
    use super::valid_resource;

    #[test]
    fn only_playlists_and_segments_are_passed_on() {
        for ok in [
            "master.m3u8",
            "720p/index.m3u8",
            "480p/seg0.ts",
            "240p/seg12.ts",
            "subtitles.json",
            "subtitles/0.vtt",
            "subtitles/12.vtt",
        ] {
            assert!(valid_resource(ok), "{ok}");
        }
        for bad in [
            "",
            "/",
            "..",
            "../master.m3u8",
            "720p/../index.m3u8",
            "720p/seg1.ts/x",
            "720p/other.m3u8",
            "720p/seg1.mp4",
            "720p//index.m3u8",
            "a b/index.m3u8",
            "720p/seg1.ts?x=1",
            "health",
            "hls/master.m3u8",
            "720p\\index.m3u8",
            "subtitles/.vtt",
            "subtitles/x.vtt",
            "subtitles/0.srt",
            "subtitles/../0.vtt",
            "subtitles/12345.vtt",
            "subtitles.json/x",
        ] {
            assert!(!valid_resource(bad), "{bad:?}");
        }
    }
}
