//! The HTTP side of a module that reads sources (see `fafnirx_fetch_api::FetchRequest`). The master sends the source's settings with
//! every call, since the module has no database, and tells it where to write: a file under the master's `tmp` folder, which it
//! hashes and commits itself. The module writes nowhere else.
//!
//! The module implements [`Fetcher`]; [`router`] makes the service of it.

use crate::env::Common;
use crate::tasks::Tasks;
use axum::extract::{Path as UrlPath, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use fafnirx_fetch_api::{Checked, FetchEntry, FetchGet, FetchListing, FetchRequest, Fetched, FETCH_CHECK_PATH, FETCH_GET_PATH, FETCH_LIST_PATH};
use fafnirx_module_api::{bearer_matches, capability, Manifest, HEALTH_PATH, MANIFEST_PATH, UI_PREFIX};
use serde_json::{Map, Value};
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// What the module is.
pub struct Info {
    /// The id it announces itself as, and the master has it under.
    pub id: &'static str,
    /// The one kind of source it reads: the capability is `fetch.<kind>`.
    pub kind: &'static str,
    pub version: &'static str,
}

/// Whether the module can read sources now, and what is missing when it cannot (or reads them in spite of a problem).
pub struct Readiness {
    pub ready: bool,
    /// In words for the administrator (the manifest's `setup`).
    pub setup: Vec<String>,
}

impl Readiness {
    pub fn ready() -> Self {
        Self { ready: true, setup: Vec::new() }
    }

    pub fn missing(what: impl Into<String>) -> Self {
        Self { ready: false, setup: vec![what.into()] }
    }
}

/// A module that reads one kind of source.
pub trait Fetcher: Send + Sync + 'static {
    /// The settings of a source, as the module reads them.
    type Config: Send + Sync;

    fn info(&self) -> Info;

    /// Whether it can read sources now. Asked for every manifest and every call, so a module that has to look (run a program) keeps
    /// the answer for a while.
    fn readiness(&self) -> impl Future<Output = Readiness> + Send {
        async { Readiness::ready() }
    }

    /// The settings a user gave, reduced to what this module reads, with the label of the source. An error is a sentence for the user.
    fn check(&self, config: &Value) -> anyhow::Result<(Value, String)>;

    /// The settings stored with a source.
    fn parse(&self, config: &Value) -> anyhow::Result<Self::Config>;

    /// What a listing of this source is called in the list of what is being done.
    fn label(&self, config: &Self::Config) -> String;

    /// What the source offers now.
    fn list(&self, config: &Self::Config) -> impl Future<Output = anyhow::Result<FetchListing>> + Send;

    /// Writes the file of `entry` to `dest`, and gives its size.
    fn get(&self, config: &Self::Config, entry: &FetchEntry, dest: &Path) -> impl Future<Output = anyhow::Result<u64>> + Send;

    /// What the module adds to `GET /status`, beside the tasks.
    fn status(&self) -> Map<String, Value> {
        Map::new()
    }
}

struct App<M> {
    data: PathBuf,
    token: String,
    jobs: usize,
    ui: Option<PathBuf>,
    module: M,
    tasks: Tasks,
    fetches: Semaphore,
    lists: Semaphore,
}

struct Failure(StatusCode, String);

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

fn fail(status: StatusCode, message: impl Into<String>) -> Failure {
    Failure(status, message.into())
}

fn internal(context: &str, e: anyhow::Error) -> Failure {
    tracing::warn!("{context}: {e:#}");
    fail(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
}

/// The service for the master: the manifest, the status, the web part and the three requests, all behind the token; and the open
/// health check.
pub fn router<M: Fetcher>(common: &Common, module: M) -> Router {
    let jobs = common.jobs.max(1);
    let app = Arc::new(App {
        data: common.data.clone(),
        token: common.token.clone(),
        jobs,
        ui: common.ui.clone(),
        module,
        tasks: Tasks::default(),
        fetches: Semaphore::new(jobs),
        lists: Semaphore::new(jobs),
    });
    let for_master = Router::new()
        .route(MANIFEST_PATH, get(manifest::<M>))
        .route("/status", get(status::<M>))
        .route(&format!("{UI_PREFIX}/:file"), get(ui_file::<M>))
        .route(FETCH_CHECK_PATH, post(check::<M>))
        .route(FETCH_LIST_PATH, post(list::<M>))
        .route(FETCH_GET_PATH, post(fetch::<M>))
        .layer(middleware::from_fn_with_state(app.clone(), guard::<M>));
    Router::new().route(HEALTH_PATH, get(|| async { "ok" })).merge(for_master).with_state(app)
}

async fn guard<M: Fetcher>(State(app): State<Arc<App<M>>>, req: Request, next: Next) -> Response {
    let given = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !bearer_matches(given, &app.token) {
        return fail(StatusCode::UNAUTHORIZED, format!("the {} module needs the master's token", app.module.info().id)).into_response();
    }
    next.run(req).await
}

async fn manifest<M: Fetcher>(State(app): State<Arc<App<M>>>) -> Json<Manifest> {
    let info = app.module.info();
    let readiness = app.module.readiness().await;
    // A module that cannot read sources says so: no capability, and a word for the administrator.
    let mut capabilities = Vec::new();
    if readiness.ready {
        capabilities.push(fafnirx_fetch_api::capability(info.kind));
    }
    capabilities.push(capability::STATUS.into());
    Json(
        Manifest::new(info.id, info.version, capabilities)
            .ui(crate::ui::describe(app.ui.as_deref()))
            .setup(readiness.setup),
    )
}

async fn status<M: Fetcher>(State(app): State<Arc<App<M>>>) -> Json<Value> {
    let mut report = serde_json::to_value(app.tasks.report(app.jobs)).unwrap_or_default();
    if let Some(object) = report.as_object_mut() {
        object.extend(app.module.status());
    }
    Json(report)
}

async fn ui_file<M: Fetcher>(State(app): State<Arc<App<M>>>, UrlPath(name): UrlPath<String>) -> Result<Response, Failure> {
    crate::ui::file(app.ui.as_deref(), &name).await.map_err(|(status, message)| fail(status, message))
}

fn not_read(kind: &str) -> Failure {
    fail(StatusCode::NOT_FOUND, format!("this module does not read {kind} sources"))
}

/// The settings of a source, or the answer for a kind this module does not read (or cannot read now).
async fn settings<M: Fetcher>(app: &App<M>, kind: &str, config: &Value) -> Result<M::Config, Failure> {
    if kind != app.module.info().kind || !app.module.readiness().await.ready {
        return Err(not_read(kind));
    }
    app.module.parse(config).map_err(|e| fail(StatusCode::BAD_REQUEST, format!("{e:#}")))
}

/// `dest` if it is a file directly in `<data>/tmp`: the one place the module is allowed to write for the master.
fn destination(data: &Path, dest: &str) -> Result<PathBuf, String> {
    let dest = PathBuf::from(dest);
    let plain_name = dest.file_name().is_some_and(|n| n != "." && n != "..");
    if !plain_name || dest.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("the destination is not a file".into());
    }
    let tmp = std::fs::canonicalize(data.join("tmp")).map_err(|e| format!("the master's tmp folder is not there: {e}"))?;
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    match std::fs::canonicalize(parent) {
        Ok(p) if p == tmp => Ok(dest),
        _ => Err("the destination is not in the master's tmp folder".into()),
    }
}

async fn check<M: Fetcher>(State(app): State<Arc<App<M>>>, Json(req): Json<FetchRequest>) -> Result<Json<Checked>, Failure> {
    if req.kind != app.module.info().kind {
        return Err(not_read(&req.kind));
    }
    let (config, label) = app.module.check(&req.config).map_err(|e| fail(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
    Ok(Json(Checked { config, label }))
}

async fn list<M: Fetcher>(State(app): State<Arc<App<M>>>, Json(req): Json<FetchRequest>) -> Result<Json<FetchListing>, Failure> {
    let cfg = settings(&app, &req.kind, &req.config).await?;
    let task = app.tasks.begin("list", &app.module.label(&cfg));
    let _permit = app.lists.acquire().await.expect("the semaphore is never closed");
    task.started();
    let listed = app.module.list(&cfg).await;
    task.finish(listed.as_ref().err().map(|e| format!("{e:#}")));
    listed.map(Json).map_err(|e| internal("list", e))
}

async fn fetch<M: Fetcher>(State(app): State<Arc<App<M>>>, Json(req): Json<FetchGet>) -> Result<Json<Fetched>, Failure> {
    let dest = destination(&app.data, &req.dest).map_err(|e| fail(StatusCode::BAD_REQUEST, e))?;
    let cfg = settings(&app, &req.kind, &req.config).await?;
    let task = app.tasks.begin("fetch", &req.entry.path);
    let _permit = app.fetches.acquire().await.expect("the semaphore is never closed");
    task.started();
    let made = app.module.get(&cfg, &req.entry, &dest).await;
    task.finish(made.as_ref().err().map(|e| format!("{e:#}")));
    made.map(|size| Json(Fetched { size })).map_err(|e| internal("fetch", e))
}

#[cfg(test)]
#[path = "../tests/unit/fetch.rs"]
mod tests;
