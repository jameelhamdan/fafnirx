//! Logging in: users with passwords, sessions kept in the database, and a limit on wrong guesses.
//!
//! The cookie holds a random token; the database holds only its hash and the user it belongs to. A session therefore ends
//! the moment the row goes: when the user logs out, is disabled, or gets a new password.

use super::access::{Access, Role};
use crate::repo::{folders, sessions, settings, users};
use crate::web::{done, run, ApiError, App, Done, Res};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{AppendHeaders, IntoResponse};
use axum::{Extension, Json};
use fafnirx_common::{unix_now, LockExt};
use fafnirx_db::client::GenericClient;
use fafnirx_db::queries as sql;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

const COOKIE: &str = "fx_session";
const SESSION_SECS: i64 = 7 * 86400;
const LOGIN_WINDOW: Duration = Duration::from_secs(60);
const LOGIN_MAX: usize = 5;
/// How long each check of a user name waits once more than `LOGIN_MAX` guesses were made at it within the window.
const SLOW_LOGIN: Duration = Duration::from_secs(1);
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
    let salt = SaltString::encode_b64(&crate::platform::db::random_bytes::<16>()).map_err(|e| anyhow::anyhow!("salt: {e}"))?;
    let hash = argon()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hash password: {e}"))?;
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
pub fn set_password(conn: &mut impl GenericClient, user_id: i64, password: &str, keep: Option<&str>) -> anyhow::Result<()> {
    let hash = hash_password(password)?;
    sql::users::set_password_hash().bind(conn, &hash, &user_id).map(drop)?;
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
        anyhow::bail!("FAFNIRX_ADMIN_PASSWORD must be set: it is the password of the user \"admin\" until you change it");
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
    let folders = folders::usable(conn, user_id)?.into_iter().map(|f| (f.key, f.can_write)).collect();
    Ok(Some(Access { user_id, name, role, folders }))
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
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

/// The lock that lets one password check of a user name run at a time. Gates nobody waits on are dropped.
fn gate(state: &App, username: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut gates = state.login_gates.locked();
    gates.retain(|_, g| Arc::strong_count(g) > 1);
    gates.entry(username.to_string()).or_default().clone()
}

fn wrong() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "Wrong username or password".into())
}

pub async fn login(State(state): State<App>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Json(body): Json<Login>) -> Result<impl IntoResponse, ApiError> {
    // The connection itself, unless it comes from a proxy the master was told to trust (`FAFNIRX_TRUSTED_PROXY`): then the
    // address the proxy saw. Otherwise forwarding headers are not believed, anyone who can reach the port could forge them.
    // Guesses are limited per address and per user name at that address: a blocked address cannot try more, and nobody can
    // lock someone else out by guessing their name. Many addresses trying one name are slowed instead (see `gate`).
    let username = body.username.trim().to_lowercase();
    let forwarded = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let ip = state.trusted.client_ip(peer.ip(), forwarded).to_string();
    let keys = [ip.clone(), format!("user:{username}@{ip}")];
    let name_key = format!("user:{username}");
    // The attempt counts from here on, before the password is checked: guesses sent at the same time cannot all slip in
    // before the first of them is counted. A right password takes its attempt back.
    let attempt = Instant::now();
    let slow = {
        let mut failures = state.login_failures.locked();
        // Forget keys that have been quiet, so the table cannot grow without bound.
        failures.retain(|_, times| {
            times.retain(|t| t.elapsed() <= LOGIN_WINDOW);
            !times.is_empty()
        });
        if keys.iter().any(|k| failures.get(k).is_some_and(|times| times.len() >= LOGIN_MAX)) {
            return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "Too many attempts, wait a minute".into()));
        }
        for key in keys.iter().chain([&name_key]) {
            failures.entry(key.clone()).or_default().push(attempt);
        }
        failures.get(&name_key).is_some_and(|times| times.len() > LOGIN_MAX)
    };
    let gate = gate(&state, &username);
    let _turn = gate.lock().await;
    if slow {
        // A name that many addresses are guessing at: one check a second, so it can still log in but cannot be guessed fast.
        tokio::time::sleep(SLOW_LOGIN).await;
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
        return Err(wrong());
    };
    {
        let mut failures = state.login_failures.locked();
        for key in &keys {
            failures.remove(key);
        }
        if let Some(times) = failures.get_mut(&name_key) {
            times.retain(|t| *t != attempt);
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

pub async fn logout(State(state): State<App>, headers: HeaderMap) -> Result<impl IntoResponse, ApiError> {
    if let Some(token) = cookie_token(&headers) {
        let stored = token_hash(&token);
        run(&state, move |c| Ok(sql::sessions::delete_session().bind(c, &stored).map(drop)?)).await?;
    }
    Ok((AppendHeaders([cookie_header("", 0, is_secure(&headers))]), Json(Done { ok: true })))
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Who {
    name: String,
    #[cfg_attr(test, ts(type = "'admin' | 'normal' | 'readonly'"))]
    role: &'static str,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Session {
    /// Who is logged in; none when nobody is.
    user: Option<Who>,
    /// Shown in the top bar; only told to a logged-in user.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    version: Option<&'static str>,
    /// The modules that are up, by id; only told to a logged-in user.
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    modules: std::collections::BTreeMap<String, fafnirx_module_api::Manifest>,
    /// Whether the web app offers the photo pages (administrator setting); only told to a logged-in user.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    photos: Option<bool>,
}

pub async fn session(State(state): State<App>, access: Option<Extension<Access>>) -> Res<Session> {
    let access = access.map(|Extension(a)| a);
    let logged_in = access.is_some();
    let photos = if logged_in { Some(run(&state, |c| Ok(settings::photos_on(c))).await?) } else { None };
    Ok(Json(Session {
        photos,
        user: access.map(|a| Who { name: a.name, role: a.role.as_str() }),
        version: logged_in.then_some(env!("CARGO_PKG_VERSION")),
        modules: if logged_in { state.modules.active() } else { Default::default() },
    }))
}

#[derive(Deserialize)]
pub struct ChangePassword {
    current: String,
    new: String,
}

/// Users change their own password; the session they do it in stays.
pub async fn change_password(State(state): State<App>, Extension(access): Extension<Access>, headers: HeaderMap, Json(body): Json<ChangePassword>) -> Res<Done> {
    if body.new.chars().count() < MIN_PASSWORD {
        return Err(ApiError::bad(format!("The password needs at least {MIN_PASSWORD} characters")));
    }
    let keep = cookie_token(&headers).map(|t| token_hash(&t));
    let id = access.user_id;
    let hash: String = run(&state, move |c| Ok(sql::users::password_hash_of().bind(c, &id).one()?)).await?;
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
#[path = "../../tests/unit/web/auth.rs"]
mod tests;
