//! Logging in: users with passwords, sessions kept in the database, and a limit on wrong guesses.
//!
//! The cookie holds a random token; the database holds only its hash and the user it belongs to. A session therefore ends
//! the moment the row goes: when the user logs out, is disabled, or gets a new password.

use super::access::{Access, Role};
use crate::repo::{devices, sessions, users};
use crate::web::{done, run, ApiError, Done, Res, Shared};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{AppendHeaders, IntoResponse};
use axum::{Extension, Json};
use fafnirx_common::unix_now;
use postgres::GenericClient;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const COOKIE: &str = "fx_session";
const SESSION_SECS: i64 = 7 * 86400;
const LOGIN_WINDOW: Duration = Duration::from_secs(60);
const LOGIN_MAX: usize = 5;
pub const MIN_PASSWORD: usize = 8;

fn argon() -> Argon2<'static> {
    // The tests log in all the time: the cheapest parameters keep them quick. Real masters use the defaults.
    #[cfg(test)]
    return Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(1024, 1, 1, None).expect("valid parameters"),
    );
    #[cfg(not(test))]
    Argon2::default()
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt =
        SaltString::encode_b64(&crate::platform::db::random_bytes::<16>()).map_err(|e| anyhow::anyhow!("salt: {e}"))?;
    let hash = argon().hash_password(password.as_bytes(), &salt).map_err(|e| anyhow::anyhow!("hash password: {e}"))?;
    Ok(hash.to_string())
}

/// Whether `password` is the one `hash` was made from. `!` (no password set) never matches.
pub fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|parsed| argon().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// A hash to check against when the user does not exist, so that the answer takes as long either way.
fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password("no such user").expect("hashing works"))
}

/// Hex SHA-256 of a session token, which is all the database keeps of it.
fn token_hash(token: &str) -> String {
    crate::platform::db::hash_token(token)
}

/// Gives the user a password. Every session of the user ends, except `keep` (the one that asked for the change).
pub fn set_password(
    conn: &mut impl GenericClient,
    user_id: i64,
    password: &str,
    keep: Option<&str>,
) -> anyhow::Result<()> {
    let hash = hash_password(password)?;
    users::set_password_hash(conn, user_id, &hash)?;
    sessions::end_others(conn, user_id, keep.unwrap_or(""))?;
    Ok(())
}

/// The default administrator has no password until the master is started with one: `FAFNIRX_ADMIN_PASSWORD` sets it
/// that once. Afterwards the variable is not read, so a password changed in the web app stays.
pub fn bootstrap(conn: &mut impl GenericClient, env_password: &str) -> anyhow::Result<()> {
    let (id, hash) = users::admin_credentials(conn)?;
    if hash != "!" {
        return Ok(());
    }
    if env_password.is_empty() {
        anyhow::bail!(
            "FAFNIRX_ADMIN_PASSWORD must be set: it is the password of the user \"admin\" until you change it"
        );
    }
    set_password(conn, id, env_password, None)?;
    tracing::info!("the password of the user \"admin\" was set from FAFNIRX_ADMIN_PASSWORD");
    Ok(())
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies
        .split(';')
        .filter_map(|c| c.trim().split_once('='))
        .find(|(name, value)| *name == COOKIE && value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|(_, value)| value.to_string())
}

/// The user a session cookie belongs to, if the session is there, has not expired and the user is not disabled.
pub fn user_of_session(conn: &mut impl GenericClient, headers: &HeaderMap) -> anyhow::Result<Option<Access>> {
    let Some(token) = cookie_token(headers) else { return Ok(None) };
    let Some((user_id, name, role)) = sessions::user_of(conn, &token_hash(&token), unix_now())? else {
        return Ok(None);
    };
    let Some(role) = Role::parse(&role) else { return Ok(None) };
    let shared_with_me = devices::shared_folders(conn, user_id)?
        .into_iter()
        .filter(|s| !s.mine)
        .map(|s| (s.folder, s.can_write))
        .collect();
    Ok(Some(Access { user_id, name, role, shared_with_me }))
}

#[derive(Deserialize)]
pub struct Login {
    username: String,
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

fn wrong() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "Wrong username or password".into())
}

pub async fn login(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<Login>,
) -> Result<impl IntoResponse, ApiError> {
    // The connection itself, unless it comes from a proxy the master was told to trust (`FAFNIRX_TRUSTED_PROXY`): then the
    // address the proxy saw. Otherwise forwarding headers are not believed, anyone who can reach the port could forge them.
    // Guesses are counted per address and per user name, so that neither one address trying many names nor many
    // addresses trying one name gets far.
    let username = body.username.trim().to_lowercase();
    let forwarded = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let keys = [state.trusted.client_ip(peer.ip(), forwarded).to_string(), format!("user:{username}")];
    {
        let mut failures = state.login_failures.lock().unwrap_or_else(|e| e.into_inner());
        // Forget keys that have been quiet, so the table cannot grow without bound.
        failures.retain(|_, times| {
            times.retain(|t| t.elapsed() <= LOGIN_WINDOW);
            !times.is_empty()
        });
        if keys.iter().any(|k| failures.get(k).is_some_and(|times| times.len() >= LOGIN_MAX)) {
            return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "Too many attempts, wait a minute".into()));
        }
    }

    let name = username.clone();
    let user = run(&state, move |c| Ok(users::login_credentials(c, &name)?)).await?;
    let (id, hash) = match &user {
        Some((id, hash)) => (Some(*id), hash.clone()),
        None => (None, dummy_hash().to_string()),
    };
    let password = body.password;
    let matched = tokio::task::spawn_blocking(move || verify_password(&hash, &password))
        .await
        .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
    let Some(id) = id.filter(|_| matched) else {
        let mut failures = state.login_failures.lock().unwrap_or_else(|e| e.into_inner());
        for key in keys {
            failures.entry(key).or_default().push(Instant::now());
        }
        return Err(wrong());
    };
    {
        let mut failures = state.login_failures.lock().unwrap_or_else(|e| e.into_inner());
        for key in &keys {
            failures.remove(key);
        }
    }

    let token = hex::encode(crate::platform::db::random_bytes::<32>());
    let stored = token_hash(&token);
    run(&state, move |c| {
        sessions::start(c, &stored, id, unix_now(), SESSION_SECS)?;
        Ok(())
    })
    .await?;
    Ok((AppendHeaders([cookie_header(&token, SESSION_SECS, is_secure(&headers))]), Json(Done { ok: true })))
}

pub async fn logout(State(state): State<Shared>, headers: HeaderMap) -> Result<impl IntoResponse, ApiError> {
    if let Some(token) = cookie_token(&headers) {
        let stored = token_hash(&token);
        run(&state, move |c| Ok(sessions::end(c, &stored)?)).await?;
    }
    Ok((AppendHeaders([cookie_header("", 0, is_secure(&headers))]), Json(Done { ok: true })))
}

#[derive(Serialize)]
pub struct Who {
    name: String,
    role: &'static str,
}

#[derive(Serialize)]
pub struct Session {
    /// Who is logged in; none when nobody is.
    user: Option<Who>,
    /// Shown in the top bar; only told to a logged-in user.
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'static str>,
    /// Where the map gets its pictures from (`{z}/{x}/{y}` stand for the tile); only told to a logged-in user.
    #[serde(skip_serializing_if = "Option::is_none")]
    map_tiles: Option<String>,
    /// Whether videos can be streamed in several qualities (a streaming service is set up); only told to a logged-in user.
    stream: bool,
}

pub async fn session(State(state): State<Shared>, access: Option<Extension<Access>>) -> Res<Session> {
    let access = access.map(|Extension(a)| a);
    let logged_in = access.is_some();
    Ok(Json(Session {
        user: access.map(|a| Who { name: a.name, role: a.role.as_str() }),
        version: logged_in.then_some(env!("CARGO_PKG_VERSION")),
        map_tiles: logged_in.then(|| state.map_tiles.clone()),
        stream: logged_in && state.stream.is_some(),
    }))
}

#[derive(Deserialize)]
pub struct ChangePassword {
    current: String,
    new: String,
}

/// Users change their own password; the session they do it in stays.
pub async fn change_password(
    State(state): State<Shared>,
    Extension(access): Extension<Access>,
    headers: HeaderMap,
    Json(body): Json<ChangePassword>,
) -> Res<Done> {
    if body.new.chars().count() < MIN_PASSWORD {
        return Err(ApiError::bad(format!("The password needs at least {MIN_PASSWORD} characters")));
    }
    let keep = cookie_token(&headers).map(|t| token_hash(&t));
    let id = access.user_id;
    let hash: String = run(&state, move |c| Ok(users::password_hash_of(c, id)?)).await?;
    let current = body.current;
    let matched = tokio::task::spawn_blocking(move || verify_password(&hash, &current))
        .await
        .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
    if !matched {
        return Err(ApiError::bad("The current password is wrong"));
    }
    let new = body.new;
    run(&state, move |c| set_password(c, id, &new, keep.as_deref())).await?;
    done()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_verify_only_against_their_own_hash() {
        let hash = hash_password("correct horse").unwrap();
        assert!(verify_password(&hash, "correct horse"));
        assert!(!verify_password(&hash, "correct horse "));
        assert!(!verify_password("!", "!"), "a user without a password cannot log in");
        assert!(!verify_password("garbage", "garbage"));
        assert_ne!(hash, hash_password("correct horse").unwrap(), "every hash has its own salt");
    }

    #[test]
    fn only_a_well_formed_cookie_is_taken_for_a_token() {
        let mut headers = HeaderMap::new();
        let token = "ab".repeat(32);
        headers.insert(header::COOKIE, HeaderValue::from_str(&format!("x=1; {COOKIE}={token}")).unwrap());
        assert_eq!(cookie_token(&headers).as_deref(), Some(token.as_str()));
        headers.insert(header::COOKIE, HeaderValue::from_str(&format!("{COOKIE}=short")).unwrap());
        assert_eq!(cookie_token(&headers), None);
    }
}
