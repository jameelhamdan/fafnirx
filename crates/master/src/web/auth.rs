//! Admin login: one password, a signed cookie, and a limit on wrong guesses.

use crate::web::{ApiError, Done, Res, Shared};
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue};
use axum::response::{AppendHeaders, IntoResponse};
use axum::Json;
use fafnirx_common::unix_now;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};

const COOKIE: &str = "fx_session";
const SESSION_SECS: i64 = 7 * 86400;
const LOGIN_WINDOW: Duration = Duration::from_secs(60);
const LOGIN_MAX: usize = 5;

/// The key that signs cookies: made once and kept in the data directory, so that logins survive a restart.
pub fn load_secret(data_dir: &Path) -> anyhow::Result<Vec<u8>> {
    let path = data_dir.join("web_secret");
    if let Some(key) = std::fs::read_to_string(&path).ok().and_then(|t| hex::decode(t.trim()).ok()) {
        if key.len() >= 32 {
            return Ok(key);
        }
    }
    let key: [u8; 32] = crate::platform::db::random_bytes();
    std::fs::create_dir_all(data_dir)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    std::io::Write::write_all(&mut options.open(&path)?, hex::encode(key).as_bytes())?;
    // `mode` only applies to a file that is created: a key file made earlier may be more open.
    #[cfg(unix)]
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    Ok(key.to_vec())
}

/// The key cookies are signed with. It changes with the password.
pub fn session_key(secret: &[u8], password: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("any key length works");
    mac.update(password.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn sign(secret: &[u8], expires: i64) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("any key length works");
    mac.update(expires.to_string().as_bytes());
    format!("{expires}.{}", hex::encode(mac.finalize().into_bytes()))
}

fn valid(secret: &[u8], value: &str) -> bool {
    let Some((expires, _)) = value.split_once('.') else { return false };
    let Ok(expires) = expires.parse::<i64>() else { return false };
    expires > unix_now() && constant_time_eq(sign(secret, expires).as_bytes(), value.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn is_logged_in(state: &Shared, headers: &HeaderMap) -> bool {
    let Some(cookies) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) else { return false };
    cookies
        .split(';')
        .filter_map(|c| c.trim().split_once('='))
        .any(|(name, value)| name == COOKIE && valid(&state.secret, value))
}

#[derive(Deserialize)]
pub struct Login {
    password: String,
}

fn cookie_header(value: &str, max_age: i64, secure: bool) -> (HeaderName, HeaderValue) {
    let secure = if secure { "; Secure" } else { "" };
    let text = format!("{COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}");
    (header::SET_COOKIE, HeaderValue::from_str(&text).expect("cookie is ASCII"))
}

/// Whether the browser reached us over https (a TLS proxy says so in a header).
pub fn is_secure(headers: &HeaderMap) -> bool {
    headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

pub async fn login(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<Login>,
) -> Result<impl IntoResponse, ApiError> {
    // The connection itself: forwarding headers are not believed, anyone who can reach the port could forge them.
    let ip = peer.ip().to_string();
    {
        let mut failures = state.login_failures.lock().unwrap_or_else(|e| e.into_inner());
        // Forget addresses that have been quiet, so the table cannot grow without bound.
        failures.retain(|_, times| {
            times.retain(|t| t.elapsed() <= LOGIN_WINDOW);
            !times.is_empty()
        });
        let recent = failures.entry(ip.clone()).or_default();
        if recent.len() >= LOGIN_MAX {
            return Err(ApiError(axum::http::StatusCode::TOO_MANY_REQUESTS, "Too many attempts, wait a minute".into()));
        }
        if !constant_time_eq(&Sha256::digest(body.password.as_bytes()), &Sha256::digest(state.password.as_bytes())) {
            recent.push(Instant::now());
            return Err(ApiError(axum::http::StatusCode::UNAUTHORIZED, "Wrong password".into()));
        }
        failures.remove(&ip);
    }
    let cookie = cookie_header(&sign(&state.secret, unix_now() + SESSION_SECS), SESSION_SECS, is_secure(&headers));
    Ok((AppendHeaders([cookie]), Json(Done { ok: true })))
}

pub async fn logout(headers: HeaderMap) -> impl IntoResponse {
    (AppendHeaders([cookie_header("", 0, is_secure(&headers))]), Json(Done { ok: true }))
}

#[derive(Serialize)]
pub struct Session {
    admin: bool,
    /// Shown in the top bar; only told to a logged-in admin.
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'static str>,
    /// Where the map gets its pictures from (`{z}/{x}/{y}` stand for the tile); only told to a logged-in admin.
    #[serde(skip_serializing_if = "Option::is_none")]
    map_tiles: Option<String>,
}

pub async fn session(State(state): State<Shared>, headers: HeaderMap) -> Res<Session> {
    let admin = is_logged_in(&state, &headers);
    Ok(Json(Session {
        admin,
        version: admin.then_some(env!("CARGO_PKG_VERSION")),
        map_tiles: admin.then(|| state.map_tiles.clone()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_verify_only_with_the_right_key_and_before_they_expire() {
        let key = b"0123456789abcdef0123456789abcdef";
        let good = sign(key, unix_now() + 100);
        assert!(valid(key, &good));
        assert!(!valid(b"another key another key another!", &good));
        assert!(!valid(key, &sign(key, unix_now() - 1)), "expired");
        assert!(!valid(key, "9999999999.00"), "forged");
        assert!(!valid(key, "garbage"));
    }

    #[test]
    fn a_new_password_ends_every_session() {
        let secret = b"0123456789abcdef0123456789abcdef";
        let (old, new) = (session_key(secret, "one"), session_key(secret, "two"));
        assert!(!valid(&new, &sign(&old, unix_now() + 100)));
        assert!(valid(&old, &sign(&old, unix_now() + 100)));
    }
}
