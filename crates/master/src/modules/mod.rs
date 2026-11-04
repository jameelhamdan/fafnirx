//! Modules: features that live in a container of their own (see docs/modules.md). Each announces itself to the master (`POST
//! /api/plugins/register`, with the shared secret `FAFNIRX_PLUGIN_SECRET`) and keeps doing so; the master asks it for its manifest
//! and uses what the manifest says it can do. A module that is not set up, does not answer or speaks another contract version simply is not there: the master
//! works without it, and the web app does not offer what it would have added.
//!
//! The rest of the master never names a module. It asks for a capability (`modules::poster`), or passes the web app's
//! request on (`proxy`).

mod proxy;

pub use proxy::{proxy, ui};

use anyhow::{bail, Context, Result};
use fafnirx_fetch_api::{Checked, FetchEntry, FetchGet, FetchListing, FetchRequest, Fetched, FETCH_CHECK_PATH, FETCH_GET_PATH, FETCH_LIST_PATH};
use fafnirx_module_api::{bearer_matches, capability, core_satisfied, valid_id, Manifest, Probe, CONTRACT, MANIFEST_PATH};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;

/// How often the plugins that have not answered yet are asked, and the silent ones are forgotten.
const TICK: Duration = Duration::from_secs(2);
/// A plugin announces itself every 30 s (`fafnirx-module-kit`); one that has been silent this long is forgotten.
const FORGET_AFTER: Duration = Duration::from_secs(90);
/// Every this many ticks all the modules are asked for their manifests again, so that one that starts late or stops is noticed.
const REFRESH_EVERY: u32 = 15;
/// What `probe` and `poster` wait for: a film on a slow disk takes a while.
const DERIVE_TIMEOUT: Duration = Duration::from_secs(120);
/// What the queue page waits for.
const STATUS_TIMEOUT: Duration = Duration::from_secs(3);
/// The largest answer to a request for a poster.
const POSTER_LIMIT: usize = 32 << 20;

/// One module, as reached from here.
pub struct Module {
    id: String,
    url: String,
    token: String,
    client: reqwest::Client,
    /// What the module said last time it was asked; none while it does not answer (or answers wrongly).
    manifest: RwLock<Option<Manifest>>,
    /// Whether its absence has been written to the log.
    reported_down: AtomicBool,
    /// Why the last attempt to reach it failed; none while it answers.
    last_error: Mutex<Option<String>>,
    /// Polls in a row that failed while it was up: one is forgiven (a busy module answers late).
    misses: AtomicU32,
    /// When it last announced itself.
    seen: Mutex<Instant>,
}

impl Module {
    fn new(id: &str, url: &str, token: &str) -> Self {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(120)).build().expect("a client");
        Self {
            id: id.to_string(),
            url: url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client,
            manifest: RwLock::new(None),
            reported_down: AtomicBool::new(false),
            last_error: Mutex::new(None),
            misses: AtomicU32::new(0),
            seen: Mutex::new(Instant::now()),
        }
    }

    fn announced(&self) {
        *self.seen.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }

    fn silent_for(&self) -> Duration {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).elapsed()
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn manifest(&self) -> Option<Manifest> {
        self.manifest.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn can(&self, capability: &str) -> bool {
        self.manifest().is_some_and(|m| m.has(capability))
    }

    /// A request to the module, with the token.
    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client.get(format!("{}{path}", self.url)).bearer_auth(&self.token)
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client.post(format!("{}{path}", self.url)).bearer_auth(&self.token)
    }

    async fn fetch_manifest(&self) -> Result<Manifest> {
        let reply = self.get(MANIFEST_PATH).timeout(Duration::from_secs(5)).send().await.context("cannot be reached")?;
        if reply.status() == reqwest::StatusCode::UNAUTHORIZED {
            bail!("refused the token");
        }
        let manifest: Manifest = reply.error_for_status()?.json().await.context("sent no manifest")?;
        if manifest.id != self.id {
            bail!("calls itself {:?}, not {:?}", manifest.id, self.id);
        }
        if manifest.contract != CONTRACT {
            bail!("speaks contract {}, this master speaks {CONTRACT}", manifest.contract);
        }
        if !core_satisfied(&manifest.core, env!("CARGO_PKG_VERSION")) {
            bail!("needs a master of version {} or newer, this is {}", manifest.core, env!("CARGO_PKG_VERSION"));
        }
        Ok(manifest)
    }

    /// Asks for the manifest and keeps the answer, and says in the log when the module comes or goes (once, not every
    /// half minute).
    async fn refresh(&self) {
        let found = self.fetch_manifest().await;
        let was_up = self.manifest().is_some();
        if found.is_err() && was_up && self.misses.fetch_add(1, Ordering::Relaxed) == 0 {
            return;
        }
        if found.is_ok() {
            self.misses.store(0, Ordering::Relaxed);
        }
        *self.last_error.lock().unwrap_or_else(|e| e.into_inner()) = found.as_ref().err().map(|e| format!("{e:#}"));
        match &found {
            Ok(m) => {
                if !was_up {
                    tracing::info!("module {} {} is up at {}", self.id, m.version, self.url);
                }
                self.reported_down.store(false, Ordering::Relaxed);
            }
            Err(e) => {
                if !self.reported_down.swap(true, Ordering::Relaxed) {
                    let state = if was_up { "is down" } else { "is not available" };
                    tracing::warn!("module {} {state}: {e:#}", self.id);
                }
            }
        }
        *self.manifest.write().unwrap_or_else(|e| e.into_inner()) = found.ok();
    }

    /// What the module is working on (its `GET /status`); none when it has no such report or does not answer.
    pub async fn status(&self) -> Option<serde_json::Value> {
        if !self.can(capability::STATUS) {
            return None;
        }
        let reply = self.get("/status").timeout(STATUS_TIMEOUT).send().await.ok()?;
        reply.error_for_status().ok()?.json().await.ok()
    }
}

/// One module for the administrator's page.
#[derive(Serialize)]
pub struct Status {
    pub id: String,
    pub version: Option<String>,
    /// `up`, `setup` (running, but `problem` says what it is missing), `down` (`problem` says why it is not reached) or
    /// `missing` (listed in `FAFNIRX_PLUGINS`, but it has never announced itself or has stopped).
    pub state: &'static str,
    pub problem: Option<String>,
    /// Whether `FAFNIRX_PLUGINS` lists it.
    pub expected: bool,
}

/// Why a plugin's announcement was not taken.
#[derive(Debug, PartialEq)]
pub enum Refusal {
    /// The secret is missing or wrong, or there is none to compare with.
    Secret(&'static str),
    /// The id or the address cannot be used.
    Invalid(String),
}

/// The modules that have announced themselves, and which of them answer. A module that is down is asked again, so one that starts
/// late is noticed; one that stops announcing itself is forgotten.
#[derive(Default)]
pub struct Registry {
    /// `FAFNIRX_PLUGIN_SECRET`: what a plugin proves it may join with, and the bearer token the master calls it with. None: no
    /// plugin may join.
    secret: Option<String>,
    /// `FAFNIRX_PLUGINS`: the ids the administrator says must be running. A plugin that is not connected, or not working, is
    /// reported as a problem instead of being silently absent. Empty: nothing is required.
    expected: Vec<String>,
    modules: RwLock<Vec<Arc<Module>>>,
}

impl Registry {
    pub fn from_env() -> Self {
        let registry = Self::with_secret(std::env::var("FAFNIRX_PLUGIN_SECRET").ok().filter(|s| !s.is_empty()));
        registry.expecting(&std::env::var("FAFNIRX_PLUGINS").unwrap_or_default())
    }

    /// Sets the plugins that must be running from a comma separated list (`video,fetch`). Ids that cannot be one are ignored, with a warning.
    pub fn expecting(mut self, list: &str) -> Self {
        for id in list.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) {
            if !valid_id(&id) {
                tracing::warn!("FAFNIRX_PLUGINS: {id:?} cannot be a plugin id and is ignored");
            } else if !self.expected.contains(&id) {
                self.expected.push(id);
            }
        }
        self
    }

    pub fn with_secret(secret: Option<String>) -> Self {
        match secret {
            Some(s) if s.len() < 16 => {
                tracing::warn!("FAFNIRX_PLUGIN_SECRET is too short: use at least 16 characters (`openssl rand -hex 24`); plugins stay off");
                Self::default()
            }
            secret => Self { secret, ..Self::default() },
        }
    }

    /// A plugin says it is at `address` (`header` is the `Authorization` it sent). It is listed from now on, and kept while it says
    /// so again at least every [`FORGET_AFTER`]; one that moves to another address replaces its old entry.
    pub fn register(&self, header: Option<&str>, id: &str, address: &str) -> Result<(), Refusal> {
        let Some(secret) = &self.secret else {
            return Err(Refusal::Secret("plugins are off: set FAFNIRX_PLUGIN_SECRET on the master"));
        };
        if !bearer_matches(header, secret) {
            return Err(Refusal::Secret("the plugin's secret is not this master's FAFNIRX_PLUGIN_SECRET"));
        }
        if !valid_id(id) {
            return Err(Refusal::Invalid(format!("{id:?} cannot be a plugin id: lower-case letters and digits, up to 24")));
        }
        if !(address.starts_with("http://") || address.starts_with("https://")) {
            return Err(Refusal::Invalid(format!("{address:?} is not an http or https address")));
        }
        let url = address.trim_end_matches('/');
        let mut modules = self.modules.write().unwrap_or_else(|e| e.into_inner());
        match modules.iter().find(|m| m.id == id) {
            Some(known) if known.url == url => known.announced(),
            _ => {
                tracing::info!("plugin {id} announced itself at {url}");
                modules.retain(|m| m.id != id);
                modules.push(Arc::new(Module::new(id, url, secret)));
            }
        }
        Ok(())
    }

    /// Forgets the plugins that have stopped announcing themselves.
    fn forget_silent(&self) {
        let mut modules = self.modules.write().unwrap_or_else(|e| e.into_inner());
        modules.retain(|m| {
            let keep = m.silent_for() < FORGET_AFTER;
            if !keep {
                tracing::warn!("plugin {} stopped announcing itself and is forgotten", m.id);
            }
            keep
        });
    }

    fn all(&self) -> Vec<Arc<Module>> {
        self.modules.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Every module that is set up, and whether it answers (the administrator's page).
    pub fn plugins(&self) -> Vec<Status> {
        let mut out: Vec<Status> = self
            .all()
            .iter()
            .map(|m| {
                let manifest = m.manifest();
                let missing = manifest.as_ref().map(|x| x.setup.join("; ")).filter(|x| !x.is_empty());
                let (state, problem) = match (&manifest, missing) {
                    (None, _) => ("down", m.last_error.lock().unwrap_or_else(|e| e.into_inner()).clone()),
                    (Some(_), Some(missing)) => ("setup", Some(missing)),
                    (Some(_), None) => ("up", None),
                };
                Status {
                    id: m.id.clone(),
                    version: manifest.map(|x| x.version),
                    state,
                    problem,
                    expected: self.expected.contains(&m.id),
                }
            })
            .collect();
        for id in &self.expected {
            if !out.iter().any(|s| &s.id == id) {
                let problem = if self.secret.is_none() {
                    "FAFNIRX_PLUGIN_SECRET is not set (or too short) on the master, so no plugin can connect".to_string()
                } else {
                    "listed in FAFNIRX_PLUGINS but not connected: its container is not running, or cannot reach the master with this secret".to_string()
                };
                out.push(Status { id: id.clone(), version: None, state: "missing", problem: Some(problem), expected: true });
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// The module of this id, if it is up.
    pub fn up(&self, id: &str) -> Option<Arc<Module>> {
        self.all().into_iter().find(|m| m.id == id && m.manifest().is_some())
    }

    /// The first module that is up and can do this.
    pub fn provider(&self, capability: &str) -> Option<Arc<Module>> {
        self.all().into_iter().find(|m| m.can(capability))
    }

    /// The modules that are up, by id.
    pub fn active(&self) -> BTreeMap<String, Manifest> {
        self.all().iter().filter_map(|m| m.manifest().map(|manifest| (m.id.clone(), manifest))).collect()
    }

    /// Every module that is up, with its report for the queue page.
    pub async fn statuses(&self) -> Vec<(String, serde_json::Value)> {
        let mut out = Vec::new();
        for m in self.all() {
            if let Some(report) = m.status().await {
                out.push((m.id.clone(), report));
            }
        }
        out
    }

    #[cfg(test)]
    pub async fn refresh_all(&self) {
        self.refresh(true).await;
    }

    /// Asks the modules for their manifests: all of them, or only the ones that have not answered yet.
    async fn refresh(&self, all: bool) {
        for m in self.all() {
            if all || m.manifest().is_none() {
                m.refresh().await;
            }
        }
    }
}

/// What the background threads (the job queue) need to reach the modules: the same registry the web API has, a way into the
/// async runtime from a thread of their own, and where the file tree is, to name files the way modules do.
struct Running {
    registry: Arc<Registry>,
    runtime: Handle,
    files: PathBuf,
}

static RUNNING: OnceLock<Running> = OnceLock::new();

/// Asks the modules for their manifests, now and every half minute (and the ones that do not answer every few seconds), and forgets
/// the ones that stop announcing themselves. `files` is the live file tree.
pub fn start(registry: Arc<Registry>, files: PathBuf) {
    let running = Running { registry: registry.clone(), runtime: Handle::current(), files };
    if RUNNING.set(running).is_err() {
        return;
    }
    tokio::spawn(async move {
        let mut tick = 0u32;
        loop {
            registry.forget_silent();
            registry.refresh(tick.is_multiple_of(REFRESH_EVERY)).await;
            tick = tick.wrapping_add(1);
            tokio::time::sleep(TICK).await;
        }
    });
}

/// For tests that need the background-thread side of the modules (`fetch_list`): makes this registry the running one. Only the
/// first call in a process has an effect, so a test that uses it must not rely on other modules being absent or present.
#[cfg(test)]
pub fn run_with_for_tests(registry: Arc<Registry>) {
    let _ = RUNNING.set(Running { registry, runtime: Handle::current(), files: PathBuf::new() });
}

/// A file as modules name it: its place in the file tree, `/`-separated (`p`).
fn tree_path(files: &Path, file: &Path) -> Result<String> {
    let relative = file.strip_prefix(files).context("the file is not in the file tree")?;
    Ok(relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
}

/// Whether a module can make a frame of any file it is given (videos, and pictures the master cannot decode).
pub fn can_poster() -> bool {
    RUNNING.get().is_some_and(|r| r.registry.provider(capability::POSTER).is_some())
}

/// What the video module says about a video file; none when there is no such module.
///
/// For the threads of the job queue: it waits for the answer.
pub fn probe_video(file: &Path, hash: &str) -> Result<Option<Probe>> {
    let Some(run) = RUNNING.get() else { return Ok(None) };
    let Some(module) = run.registry.provider(capability::PROBE_VIDEO) else { return Ok(None) };
    let p = tree_path(&run.files, file)?;
    run.runtime.block_on(async {
        let reply = module.get("/probe").query(&[("p", p.as_str()), ("h", hash)]).timeout(DERIVE_TIMEOUT).send().await?;
        if reply.status() == reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE {
            return Ok(Some(Probe::default())); // not a video it can read: that is its answer, not a failure
        }
        Ok(Some(ok(reply).await?.json().await?))
    })
}

/// One frame of a file as a JPEG no larger than `max` pixels, and where in the video it was taken (milliseconds) when the
/// module looked for the place; none when no module can make a frame. `at_ms` is a place an earlier answer gave.
///
/// For the threads of the job queue: it waits for the answer.
pub fn poster(file: &Path, hash: &str, max: u32, at_ms: Option<i64>) -> Result<Option<(Vec<u8>, Option<i64>)>> {
    let Some(run) = RUNNING.get() else { return Ok(None) };
    let Some(module) = run.registry.provider(capability::POSTER) else { return Ok(None) };
    let p = tree_path(&run.files, file)?;
    run.runtime.block_on(async {
        let mut query = vec![("p", p.clone()), ("h", hash.to_string()), ("max", max.to_string())];
        query.extend(at_ms.map(|ms| ("at_ms", ms.to_string())));
        let reply = module.get("/poster").query(&query).timeout(DERIVE_TIMEOUT).send().await?;
        let reply = ok(reply).await?;
        let taken_at = reply.headers().get("x-poster-ms").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok());
        if reply.content_length().is_some_and(|n| n as usize > POSTER_LIMIT) {
            bail!("the {} module sent a picture that is too large", module.id);
        }
        use futures_util::StreamExt;
        let mut picture = Vec::new();
        let mut body = reply.bytes_stream();
        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            if picture.len() + chunk.len() > POSTER_LIMIT {
                bail!("the {} module sent a picture that is too large", module.id);
            }
            picture.extend_from_slice(&chunk);
        }
        Ok(Some((picture, taken_at)))
    })
}

/// What a module needs to list a source: it may wait for a slow site.
const FETCH_LIST_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// What it may take to make one file. The module gives up on its own when nothing arrives for a while (its idle timeout);
/// this only keeps a stuck connection from living for ever.
const FETCH_GET_TIMEOUT: Duration = Duration::from_secs(24 * 3600);

fn fetcher(kind: &str) -> Result<Arc<Module>> {
    RUNNING
        .get()
        .and_then(|r| r.registry.provider(&fafnirx_fetch_api::capability(kind)))
        .with_context(|| format!("no plugin that can read {kind} sources is connected: start its container (docs/plugins.md), then run the source again"))
}

/// The settings of a new source as the module reads them, and the label of the source; a module refuses them with a sentence
/// for the user, which is passed on as it is.
pub async fn fetch_check(kind: &str, config: &serde_json::Value) -> Result<Checked> {
    let module = fetcher(kind)?;
    let body = FetchRequest { kind: kind.to_string(), config: config.clone() };
    let reply = module.post(FETCH_CHECK_PATH).json(&body).timeout(Duration::from_secs(30)).send().await?;
    if reply.status() == reqwest::StatusCode::BAD_REQUEST {
        bail!("{}", reply.text().await.unwrap_or_default().chars().take(300).collect::<String>());
    }
    Ok(ok(reply).await?.json().await?)
}

/// What a source of this kind offers right now.
pub async fn fetch_list(kind: &str, config: &serde_json::Value) -> Result<FetchListing> {
    let module = fetcher(kind)?;
    let body = FetchRequest { kind: kind.to_string(), config: config.clone() };
    let reply = module.post(FETCH_LIST_PATH).json(&body).timeout(FETCH_LIST_TIMEOUT).send().await?;
    Ok(ok(reply).await?.json().await?)
}

/// Has the module write the file of `entry` to `dest` (under the master's `tmp` folder); answers with its size.
pub async fn fetch_get(kind: &str, config: &serde_json::Value, entry: &FetchEntry, dest: &Path) -> Result<u64> {
    let module = fetcher(kind)?;
    let body = FetchGet {
        kind: kind.to_string(),
        config: config.clone(),
        entry: entry.clone(),
        dest: dest.to_string_lossy().into_owned(),
    };
    let reply = module.post(FETCH_GET_PATH).json(&body).timeout(FETCH_GET_TIMEOUT).send().await?;
    Ok(ok(reply).await?.json::<Fetched>().await?.size)
}

/// The reply, or the module's own words about why not.
async fn ok(reply: reqwest::Response) -> Result<reqwest::Response> {
    if reply.status().is_success() {
        return Ok(reply);
    }
    let status = reply.status();
    let message = reply.text().await.unwrap_or_default();
    bail!("the module answered {status}: {}", message.chars().take(300).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "shared-secret-0123456789";
    const BEARER: &str = "Bearer shared-secret-0123456789";

    #[test]
    fn a_plugin_joins_with_the_secret_and_nothing_else_does() {
        let r = Registry::with_secret(Some(SECRET.into()));
        assert_eq!(r.register(Some(BEARER), "video", "http://video:8090/"), Ok(()));
        assert_eq!(r.all().len(), 1);
        assert_eq!(r.all()[0].url, "http://video:8090", "the slash at the end goes");
        for header in [None, Some(""), Some("Bearer wrong"), Some("shared-secret-0123456789")] {
            assert!(matches!(r.register(header, "docs", "http://docs"), Err(Refusal::Secret(_))), "{header:?}");
        }
        for (id, address) in [("Odd", "http://x"), ("", "http://x"), ("docs", "ftp://x"), ("docs", "docs:8090")] {
            assert!(matches!(r.register(Some(BEARER), id, address), Err(Refusal::Invalid(_))), "{id} {address}");
        }
        assert_eq!(r.all().len(), 1, "only the first was taken");
        assert!(r.up("video").is_none() && r.active().is_empty(), "a module is not up before it has answered");
        let listed = r.plugins();
        assert_eq!((listed[0].id.as_str(), listed[0].state), ("video", "down"));
    }

    #[test]
    fn a_plugin_that_must_run_is_reported_when_it_is_not_connected() {
        let r = Registry::with_secret(Some(SECRET.into())).expecting(" Video, fetch,video,BAD ID,");
        assert_eq!(r.expected, ["video", "fetch"]);
        r.register(Some(BEARER), "video", "http://video:8090").unwrap();
        r.register(Some(BEARER), "docs", "http://docs:1").unwrap();
        let listed: Vec<_> = r.plugins().iter().map(|s| (s.id.clone(), s.state, s.expected)).collect();
        assert_eq!(
            listed,
            [("docs".into(), "down", false), ("fetch".into(), "missing", true), ("video".into(), "down", true)]
        );
        let none = Registry::with_secret(None).expecting("video");
        assert!(none.plugins()[0].problem.as_deref().unwrap().contains("FAFNIRX_PLUGIN_SECRET"));
    }

    #[test]
    fn without_a_secret_or_with_a_short_one_no_plugin_may_join() {
        for secret in [None, Some("short".to_string())] {
            let r = Registry::with_secret(secret);
            assert!(matches!(r.register(Some("Bearer short"), "video", "http://video"), Err(Refusal::Secret(_))));
            assert!(r.all().is_empty());
        }
    }

    #[test]
    fn announcing_again_keeps_the_entry_a_new_address_replaces_it_and_silence_ends_it() {
        let r = Registry::with_secret(Some(SECRET.into()));
        r.register(Some(BEARER), "video", "http://a:1").unwrap();
        let first = r.all()[0].clone();
        r.register(Some(BEARER), "video", "http://a:1").unwrap();
        assert!(Arc::ptr_eq(&first, &r.all()[0]), "the same address is the same plugin");
        r.register(Some(BEARER), "video", "http://b:2").unwrap();
        assert_eq!((r.all().len(), r.all()[0].url.as_str()), (1, "http://b:2"));
        *r.all()[0].seen.lock().unwrap() = Instant::now() - FORGET_AFTER - Duration::from_secs(1);
        r.forget_silent();
        assert!(r.all().is_empty());
    }

    #[test]
    fn files_are_named_by_their_place_in_the_tree() {
        let files = Path::new("/data/files");
        assert_eq!(
            tree_path(files, Path::new("/data/files/users/admin/pc/a b/clip.mp4")).unwrap(),
            "users/admin/pc/a b/clip.mp4"
        );
        assert_eq!(tree_path(files, Path::new("/data/files/clip.mp4")).unwrap(), "clip.mp4");
        assert!(tree_path(files, Path::new("/elsewhere/clip.mp4")).is_err());
    }
}
