//! The HTTP API (JSON under `/api`) and the web app that uses it. Separate from the device gRPC
//! API: devices reach only their own uploads and their user's shared folders, everything here (browsing,
//! downloading, deleting, configuring) needs a login, and what a user can reach depends on their role (see `access`).
//!
//! The routes are split by cost: `light` routes are quick JSON calls, `heavy` routes move file contents
//! (downloads, uploads, zip) and have no body size limit.

pub mod access;
mod assets;
pub mod auth;
mod events;
mod plugins;
pub mod users;

#[cfg(test)]
mod tests;

pub use events::listen;

use crate::platform::db::Db;
use crate::platform::store::Store;
use crate::{devices, files, integrations, library, monitoring};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use postgres::Client;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::{broadcast, Semaphore};

pub struct AppState {
    pub db: Db,
    pub store: Arc<Store>,
    pub data_dir: PathBuf,
    /// Changes announced by `NOTIFY`, as JSON text, for the live event streams.
    pub events: broadcast::Sender<String>,
    pub login_failures: Mutex<HashMap<String, Vec<Instant>>>,
    /// The proxies whose word about a client's address is believed (`FAFNIRX_TRUSTED_PROXY`).
    pub trusted: crate::platform::proxy::Trusted,
    /// Upload sessions that are being filled.
    pub uploads: Mutex<HashMap<String, crate::files::transfer::Upload>>,
    /// At most this many files are streamed at once, so that transfers cannot starve everything else.
    pub streams: Arc<Semaphore>,
    /// The modules that are set up (see `modules`).
    pub modules: Arc<crate::modules::Registry>,
    /// The plugins in `<data>/plugins`, which the master runs as modules.
    pub plugins: Arc<crate::modules::Plugins>,
}

pub type Shared = Arc<AppState>;

impl AppState {
    pub fn new(db: Db, store: Arc<Store>, data_dir: PathBuf) -> Result<Self, anyhow::Error> {
        let modules = Arc::new(crate::modules::Registry::from_env());
        Ok(Self {
            plugins: Arc::new(crate::modules::Plugins::new(&data_dir, modules.clone())),
            db,
            store,
            data_dir,
            events: broadcast::channel(256).0,
            login_failures: Mutex::default(),
            trusted: crate::platform::proxy::Trusted::from_env()?,
            uploads: Mutex::default(),
            streams: Arc::new(Semaphore::new(32)),
            modules,
        })
    }
}

/// An error a handler reports: the status and a message that is safe to show.
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub String);

impl ApiError {
    pub fn bad(msg: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self(StatusCode::NOT_FOUND, msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self(StatusCode::CONFLICT, msg.into())
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.1)
    }
}

impl std::error::Error for ApiError {}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        match e.downcast::<ApiError>() {
            Ok(api) => api,
            Err(e) => {
                tracing::error!("web api: {e:#}");
                Self(StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        }
    }
}

impl From<postgres::Error> for ApiError {
    fn from(e: postgres::Error) -> Self {
        anyhow::Error::from(e).into()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

pub type Res<T> = Result<Json<T>, ApiError>;

/// Runs `f` on a blocking thread with a database connection.
pub async fn run<T, F>(state: &Shared, f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&mut Client) -> anyhow::Result<T> + Send + 'static,
{
    Ok(crate::platform::db::with(&state.db, f).await?)
}

#[derive(Serialize)]
pub struct Done {
    pub ok: bool,
}

pub fn done() -> Res<Done> {
    Ok(Json(Done { ok: true }))
}

/// Runs `f` like [`run`] and answers `{ "ok": true }`: the handlers that change something and return nothing.
pub async fn act<F>(state: &Shared, f: F) -> Res<Done>
where
    F: FnOnce(&mut Client) -> anyhow::Result<()> + Send + 'static,
{
    run(state, f).await?;
    done()
}

/// Quick JSON calls.
fn light() -> Router<Shared> {
    Router::new()
        .route("/api/login", post(auth::login))
        .route("/api/logout", post(auth::logout))
        .route("/api/session", get(auth::session))
        .route("/api/plugins", get(plugins::list))
        .route("/api/me/password", post(auth::change_password))
        .route("/api/users", get(users::list).post(users::create))
        .route("/api/users/:name", axum::routing::patch(users::update))
        .route("/api/dashboard", get(monitoring::api::dashboard))
        .route("/api/alerts/:id/dismiss", post(monitoring::api::dismiss))
        .route("/api/settings", get(devices::api::settings).put(devices::api::save_settings))
        .route("/api/devices", get(devices::api::list).post(devices::api::create))
        .route("/api/devices/:name", get(devices::api::detail))
        .route("/api/devices/:name/token", post(devices::api::rotate_token))
        .route("/api/devices/:name/release", post(devices::api::release))
        .route("/api/devices/:name/revoke", post(devices::api::revoke))
        .route("/api/shared", get(devices::shared::list).post(devices::shared::create))
        .route("/api/shared/leave", post(devices::shared::leave))
        .route("/api/shared/people", get(devices::shared::people))
        .route("/api/shared/:name", delete(devices::shared::delete))
        .route("/api/shared/:name/members", post(devices::shared::add_member))
        .route("/api/shared/:name/members/:user", delete(devices::shared::remove_member))
        .route("/api/files", get(files::browse::list))
        .route("/api/files/info", get(files::browse::info))
        .route("/api/files/delete", post(files::browse::remove))
        .route("/api/files/move", post(files::browse::move_files))
        .route("/api/files/rescan", post(files::browse::rescan))
        .route("/api/files/restore", post(files::browse::restore))
        .route("/api/timeline/buckets", get(library::photos::buckets))
        .route("/api/timeline", get(library::photos::timeline))
        .route("/api/library", get(library::photos::by_ids))
        .route("/api/library/bulk", post(library::photos::bulk))
        .route("/api/library/:id", get(library::photos::detail).patch(library::photos::edit))
        .route("/api/library/:id/folder", get(library::photos::folder))
        .route("/api/tags", get(library::photos::tags))
        .route("/api/tags/:id", delete(library::photos::delete_tag))
        .route("/api/trash", get(library::photos::trash))
        .route("/api/albums", get(library::albums::list).post(library::albums::create))
        .route(
            "/api/albums/:id",
            get(library::albums::detail).patch(library::albums::edit).delete(library::albums::delete),
        )
        .route("/api/albums/:id/files", post(library::albums::add))
        .route("/api/albums/:id/files/remove", post(library::albums::remove))
        .route("/api/sources", get(integrations::api::sources).post(integrations::api::create_source))
        .route("/api/sources/:user/:name", delete(integrations::api::delete_source))
        .route("/api/sources/:user/:name/run", post(integrations::api::run_source))
        .route("/api/sources/:user/:name/toggle", post(integrations::api::toggle_source))
        .route("/api/mirrors", get(integrations::api::mirrors).post(integrations::api::create_mirror))
        .route("/api/mirrors/scope", get(integrations::api::mirror_scope))
        .route("/api/mirrors/:name", delete(integrations::api::delete_mirror))
        .route("/api/mirrors/:name/run", post(integrations::api::run_mirror))
        .route("/api/mirrors/:name/toggle", post(integrations::api::toggle_mirror))
        .route("/api/mirrors/:name/resync", post(integrations::api::resync_mirror))
        .route("/api/mirrors/:name/restore-kit", get(integrations::api::mirror_restore_kit))
        .route("/api/events", get(events::stream))
}

/// Moving file contents.
fn heavy() -> Router<Shared> {
    Router::new()
        .route("/api/files/download", get(files::transfer::download))
        .route("/api/files/thumb", get(files::transfer::thumbnail))
        .route("/api/media/:id/:variant", get(files::transfer::media))
        .route("/api/m/:module/ui/:file", get(crate::modules::ui))
        .route("/api/m/:module/:id/*rest", get(crate::modules::proxy))
        .route("/api/queue", get(monitoring::queue::show))
        .route("/api/queue/pause", post(monitoring::queue::pause))
        .route("/api/queue/retry", post(monitoring::queue::retry))
        .route("/api/queue/cancel", post(monitoring::queue::cancel))
        .route("/api/queue/rebuild", post(monitoring::queue::rebuild))
        .route("/api/files/zip", post(files::transfer::zip))
        .route("/api/uploads", post(files::transfer::start_upload))
        .route(
            "/api/uploads/:id",
            get(files::transfer::upload_status)
                .put(files::transfer::upload_chunk)
                .delete(files::transfer::abort_upload),
        )
        .route("/api/uploads/:id/finish", post(files::transfer::finish_upload))
        .layer(DefaultBodyLimit::disable())
}

/// Every route, behind the login, plus the web app.
pub fn router(state: Shared) -> Router {
    light()
        .merge(heavy())
        .route("/healthz", get(healthz))
        // The fallback comes first: a layer only wraps what the router holds when it is added.
        .fallback(assets::serve)
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}

/// Answers while the process is up; for container health checks and monitoring.
async fn healthz() -> &'static str {
    "ok"
}

/// Headers every response carries. The pages may only load their own scripts and styles, and cannot be put in
/// a frame; what the API sends is only ever data.
async fn security_headers(req: axum::extract::Request, next: Next) -> Response {
    let secure = auth::is_secure(req.headers());
    let page = !req.uri().path().starts_with("/api/");
    let mut resp = next.run(req).await;
    let csp = if page {
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; font-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'"
    } else {
        "frame-ancestors 'none'"
    };
    let mut set = vec![
        ("content-security-policy", csp.to_string()),
        ("x-content-type-options", "nosniff".into()),
        ("x-frame-options", "DENY".into()),
        ("referrer-policy", "same-origin".into()),
        ("permissions-policy", "camera=(), microphone=(), geolocation=(), payment=(), usb=()".into()),
        ("cross-origin-opener-policy", "same-origin".into()),
        ("cross-origin-resource-policy", "same-origin".into()),
    ];
    if secure {
        set.push(("strict-transport-security", "max-age=31536000".into()));
    }
    for (name, value) in set {
        if let Ok(value) = axum::http::HeaderValue::from_str(&value) {
            resp.headers_mut().entry(name).or_insert(value);
        }
    }
    resp
}

/// Everything under `/api` except logging in needs a valid session (and tells the handlers who it is, as an
/// [`access::Access`]); changes also need the marker header (a cross-site form cannot set it).
async fn guard(State(state): State<Shared>, mut req: axum::extract::Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    if path.starts_with("/api/") && path != "/api/login" {
        let headers = req.headers().clone();
        let who = match crate::platform::db::with(&state.db, move |c| auth::user_of_session(c, &headers)).await {
            Ok(who) => who,
            Err(e) => return ApiError::from(e).into_response(),
        };
        match who {
            Some(who) => {
                req.extensions_mut().insert(who);
            }
            // The page asks who is logged in before it knows: nobody is an answer.
            None if path == "/api/session" => return next.run(req).await,
            None => return ApiError(StatusCode::UNAUTHORIZED, "login required".into()).into_response(),
        }
        let changes = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
        // The zip download is a plain form post; the strict same-site cookie already keeps other sites out.
        if changes && path != "/api/files/zip" && req.headers().get("x-requested-with").is_none() {
            return ApiError(StatusCode::FORBIDDEN, "missing x-requested-with header".into()).into_response();
        }
    }
    next.run(req).await
}
