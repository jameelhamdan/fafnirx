//! Modules: features that live in a service of their own (see docs/modules.md). The master knows them only through
//! `FAFNIRX_MODULE_<ID>_URL` and `FAFNIRX_MODULE_<ID>_TOKEN`, asks each for its manifest, and uses what the manifest says it
//! can do. A module that is not set up, does not answer or speaks another contract version simply is not there: the master
//! works without it, and the web app does not offer what it would have added.
//!
//! The rest of the master never names a module. It asks for a capability (`modules::poster`), or passes the web app's
//! request on (`proxy`).

mod plugins;
mod proxy;

pub use plugins::{Plugins, Status as PluginStatus};
pub use proxy::{proxy, ui};

use anyhow::{bail, Context, Result};
use fafnirx_module_api::{capability, Manifest, Probe, CONTRACT, MANIFEST_PATH};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;
use tokio::runtime::Handle;

/// How often the plugins are looked after, and a plugin that has not answered yet is asked.
const TICK: Duration = Duration::from_secs(2);
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
    /// Whether the master runs it (a plugin) rather than somebody else (a container).
    managed: bool,
    /// Polls in a row that failed while it was up: one is forgiven (a busy module answers late).
    misses: AtomicU32,
}

impl Module {
    fn new(id: &str, url: &str, token: &str, managed: bool) -> Self {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(120)).build().expect("a client");
        Self {
            id: id.to_string(),
            url: url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client,
            manifest: RwLock::new(None),
            reported_down: AtomicBool::new(false),
            managed,
            misses: AtomicU32::new(0),
        }
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

    async fn fetch_manifest(&self) -> Result<Manifest> {
        let reply =
            self.get(MANIFEST_PATH).timeout(Duration::from_secs(5)).send().await.context("cannot be reached")?;
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

/// The modules that are set up, and which of them answer. Plugins come and go while the master runs.
#[derive(Default)]
pub struct Registry {
    modules: RwLock<Vec<Arc<Module>>>,
}

impl Registry {
    pub fn from_env() -> Self {
        Self::from_vars(std::env::vars())
    }

    /// The modules `FAFNIRX_MODULE_<ID>_URL` (with its `_TOKEN`) name. An empty address means off.
    pub fn from_vars(vars: impl Iterator<Item = (String, String)>) -> Self {
        let vars: BTreeMap<String, String> = vars.collect();
        let mut modules = Vec::new();
        for (key, url) in &vars {
            let Some(id) = key.strip_prefix("FAFNIRX_MODULE_").and_then(|k| k.strip_suffix("_URL")) else { continue };
            if url.is_empty() || id.is_empty() || !id.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
                continue;
            }
            let id = id.to_ascii_lowercase();
            match vars.get(&format!("FAFNIRX_MODULE_{}_TOKEN", id.to_ascii_uppercase())).filter(|t| !t.is_empty()) {
                Some(token) => modules.push(Arc::new(Module::new(&id, url, token, false))),
                None => tracing::warn!(
                    "FAFNIRX_MODULE_{0}_URL is set but FAFNIRX_MODULE_{0}_TOKEN is not: the {id} module stays off",
                    id.to_ascii_uppercase()
                ),
            }
        }
        Self { modules: RwLock::new(modules) }
    }

    fn all(&self) -> Vec<Arc<Module>> {
        self.modules.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Adds a module, or replaces the one of the same id.
    fn set(&self, module: Module) {
        let mut modules = self.modules.write().unwrap_or_else(|e| e.into_inner());
        modules.retain(|m| m.id != module.id);
        modules.push(Arc::new(module));
    }

    /// Drops the module of this id if the master runs it (a module somebody else runs stays).
    fn remove(&self, id: &str) {
        self.modules.write().unwrap_or_else(|e| e.into_inner()).retain(|m| !(m.id == id && m.managed));
    }

    /// Whether a module of this id is set up by an address.
    fn is_external(&self, id: &str) -> bool {
        self.all().iter().any(|m| m.id == id && !m.managed)
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

    /// Asks the modules for their manifests: all of them, or only the plugins that have not answered yet (they are
    /// still starting, so they are asked more often than the rest).
    async fn refresh(&self, all: bool) {
        for m in self.all() {
            if all || (m.managed && m.manifest().is_none()) {
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

/// Starts the plugins that are installed, and asks the modules for their manifests, now and every half minute. `files` is
/// the live file tree.
pub fn start(registry: Arc<Registry>, plugins: Arc<Plugins>, files: PathBuf) {
    let running = Running { registry: registry.clone(), runtime: Handle::current(), files };
    if RUNNING.set(running).is_err() {
        return;
    }
    plugins.tidy();
    tokio::spawn(plugins.clone().ensure());
    tokio::spawn(async move {
        let mut tick = 0u32;
        loop {
            plugins.sync();
            registry.refresh(tick.is_multiple_of(REFRESH_EVERY)).await;
            tick = tick.wrapping_add(1);
            tokio::time::sleep(TICK).await;
        }
    });
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
        let reply =
            module.get("/probe").query(&[("p", p.as_str()), ("h", hash)]).timeout(DERIVE_TIMEOUT).send().await?;
        if reply.status() == reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE {
            return Ok(Some(Probe::default())); // not a video it can read: that is its answer, not a failure
        }
        Ok(Some(ok(reply).await?.json().await?))
    })
}

/// One frame of a file as a JPEG no larger than `max` pixels; none when no module can make one.
///
/// For the threads of the job queue: it waits for the answer.
pub fn poster(file: &Path, hash: &str, max: u32) -> Result<Option<Vec<u8>>> {
    let Some(run) = RUNNING.get() else { return Ok(None) };
    let Some(module) = run.registry.provider(capability::POSTER) else { return Ok(None) };
    let p = tree_path(&run.files, file)?;
    run.runtime.block_on(async {
        let reply = module
            .get("/poster")
            .query(&[("p", p.as_str()), ("h", hash), ("max", &max.to_string())])
            .timeout(DERIVE_TIMEOUT)
            .send()
            .await?;
        let reply = ok(reply).await?;
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
        Ok(Some(picture))
    })
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

    fn registry(vars: &[(&str, &str)]) -> Registry {
        Registry::from_vars(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())))
    }

    #[test]
    fn modules_are_found_by_their_variables() {
        let r = registry(&[
            ("FAFNIRX_MODULE_VIDEO_URL", "http://video:8090/"),
            ("FAFNIRX_MODULE_VIDEO_TOKEN", "t"),
            ("FAFNIRX_MODULE_DOCS_URL", "http://docs"),
            ("FAFNIRX_MODULE_DOCS_TOKEN", ""),
            ("FAFNIRX_MODULE_OFF_URL", ""),
            ("FAFNIRX_MODULE_OFF_TOKEN", "t"),
            ("FAFNIRX_MODULE_TOKEN", "the service's own"),
            ("FAFNIRX_MODULE_odd_URL", "http://x"),
            ("FAFNIRX_DATA", "/data"),
        ]);
        let all = r.all();
        let ids: Vec<&str> = all.iter().map(|m| m.id()).collect();
        assert_eq!(ids, ["video"], "no token, no address and strange names are left out");
        assert_eq!(all[0].url, "http://video:8090", "the slash at the end goes");
        assert!(r.is_external("video") && !r.is_external("docs"));
        assert!(r.up("video").is_none() && r.active().is_empty(), "a module is not up before it has answered");
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
