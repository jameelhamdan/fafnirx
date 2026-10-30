//! Plugins: modules the master runs itself. A plugin is a bundle in `<data>/plugins/<id>/` (`plugin.json`, the module's
//! program, its web app script, whatever tools it carries). The master starts the program on a loopback port with a token
//! of its own, restarts it when it stops, and registers it like any other module, so everything past that point is the
//! same as for a module somebody runs as a container.
//!
//! Bundles come from the project's GitHub releases (`fafnirx-plugin-<id>-<version>-<platform>.zip` with a `.sha256` beside
//! it, for the release of this very version of the master) or are unpacked into the folder by hand (offline installs).
//! Only the server administrator changes them: `FAFNIRX_PLUGINS=video` in the master's environment installs what is missing,
//! deleting the folder removes one. The web app shows their state and nothing more.
//! A module that is also set up with `FAFNIRX_MODULE_<ID>_URL` is not started: what the administrator names wins.

use super::{Module, Registry};
use anyhow::{anyhow, bail, Context, Result};
use fafnirx_module_api::{valid_id, PluginFile, PLUGIN_FILE};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};

/// The largest bundle that is downloaded, and the most that is unpacked from it.
const MAX_DOWNLOAD: u64 = 400 << 20;
const MAX_UNPACKED: u64 = 1500 << 20;
/// How long a plugin that stopped waits before it is started again, at most (it doubles from a second).
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// A plugin that ran this long before it stopped starts over with the short wait.
const STABLE: Duration = Duration::from_secs(60);

/// The name of this machine in the names of bundles (`linux-x86_64`, `linux-aarch64`), as for the client's builds.
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Where the API of the release of this version of the master is. `FAFNIRX_PLUGIN_RELEASE_URL` replaces it (a fork, or a
/// test).
fn release_url() -> String {
    if cfg!(test) {
        return "http://127.0.0.1:9/release".into(); // the tests never go to the internet
    }
    if let Ok(url) = std::env::var("FAFNIRX_PLUGIN_RELEASE_URL") {
        return url;
    }
    let repo = env!("CARGO_PKG_REPOSITORY").trim_end_matches('/');
    let path = repo.strip_prefix("https://github.com/").unwrap_or(repo);
    format!("https://api.github.com/repos/{path}/releases/tags/v{}", env!("CARGO_PKG_VERSION"))
}

/// A bundle in a release.
#[derive(Debug, PartialEq)]
struct Offer {
    id: String,
    version: String,
    url: String,
    checksum_url: String,
}

/// The bundles for this platform that a release (as its API tells it) carries.
fn offers(release: &Value, platform: &str) -> Vec<Offer> {
    let Some(version) = release.get("tag_name").and_then(Value::as_str).map(|t| t.trim_start_matches('v')) else {
        return Vec::new();
    };
    let assets: Vec<(&str, &str)> = release
        .get("assets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| Some((a.get("name")?.as_str()?, a.get("browser_download_url")?.as_str()?)))
        .collect();
    let url_of = |name: &str| assets.iter().find(|(n, _)| *n == name).map(|(_, u)| u.to_string());
    let suffix = format!("-{version}-{platform}.zip");
    assets
        .iter()
        .filter_map(|(name, _)| {
            let id = name.strip_prefix("fafnirx-plugin-")?.strip_suffix(&suffix)?;
            Some(Offer {
                id: id.to_string(),
                version: version.to_string(),
                url: url_of(name)?,
                checksum_url: url_of(&format!("{name}.sha256"))?,
            })
        })
        .filter(|o| valid_id(&o.id))
        .collect()
}

/// The checksum a `.sha256` file holds (`<hex>  <name>`).
fn checksum_of(text: &str) -> Option<String> {
    let hex = text.split_whitespace().next()?.to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

/// The variables of the master every program needs to run at all (and, on Windows, to use the network).
const BASICS: [&str; 12] = [
    "PATH",
    "HOME",
    "LANG",
    "TZ",
    "RUST_LOG",
    "TMPDIR",
    "SYSTEMROOT",
    "WINDIR",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "PATHEXT",
];

/// What a plugin is given to run with: its own settings, then the ones the master decides. `passed` are the master's own
/// variables; only the ones for this plugin (`FAFNIRX_<ID>_…`) and the plain basics go on to it.
fn child_env(
    file: &PluginFile,
    dir: &Path,
    data: &Path,
    token: &str,
    listen: &str,
    passed: impl Iterator<Item = (String, String)>,
) -> BTreeMap<String, String> {
    let mine = format!("FAFNIRX_{}_", file.id.to_ascii_uppercase());
    let mut env: BTreeMap<String, String> =
        passed.filter(|(k, _)| k.starts_with(&mine) || BASICS.contains(&k.to_ascii_uppercase().as_str())).collect();
    let dir_text = dir.to_string_lossy();
    for (k, v) in &file.env {
        env.insert(k.clone(), v.replace("{dir}", &dir_text));
    }
    env.insert("FAFNIRX_MODULE_TOKEN".into(), token.into());
    env.insert("FAFNIRX_MODULE_LISTEN".into(), listen.into());
    env.insert("FAFNIRX_DATA".into(), data.to_string_lossy().into_owned());
    env.insert("FAFNIRX_MODULE_EXIT_WITH_STDIN".into(), "1".into());
    env
}

/// A plugin that is installed.
struct Installed {
    file: PluginFile,
    dir: PathBuf,
}

/// What the master knows of a plugin it started.
struct Started {
    child: Option<Child>,
    /// Why it is not running, when it should be.
    problem: Option<String>,
    at: Instant,
    failures: u32,
    retry_at: Instant,
}

/// One plugin for the administrator's page.
#[derive(Serialize)]
pub struct Status {
    pub id: String,
    pub version: Option<String>,
    /// `up`, `setup` (running, but `problem` says what it is missing), `starting` (not answering yet), `broken` (not running,
    /// see `problem`) or `external` (set up with an address, not run here).
    pub state: &'static str,
    pub problem: Option<String>,
}

pub struct Plugins {
    dir: PathBuf,
    data: PathBuf,
    registry: Arc<Registry>,
    platform: String,
    started: Mutex<BTreeMap<String, Started>>,
    /// Why a plugin that is in the folder cannot run here (its files, or another module of the same id).
    problems: Mutex<BTreeMap<String, String>>,
    /// One installation at a time.
    installing: tokio::sync::Mutex<()>,
    /// Plugins being installed or removed: `sync` leaves them alone until that is done.
    busy: Mutex<std::collections::BTreeSet<String>>,
    http: reqwest::Client,
}

impl Plugins {
    pub fn new(data: &Path, registry: Arc<Registry>) -> Self {
        Self {
            dir: data.join("plugins"),
            data: data.to_path_buf(),
            registry,
            platform: platform(),
            started: Mutex::default(),
            problems: Mutex::default(),
            installing: tokio::sync::Mutex::new(()),
            busy: Mutex::default(),
            http: reqwest::Client::builder()
                .user_agent(concat!("fafnirx/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(Duration::from_secs(15))
                .build()
                .expect("a client"),
        }
    }

    /// The plugins that are in the folder and could run here. The others are written down in `problems`.
    fn scan(&self) -> Vec<Installed> {
        let mut found = Vec::new();
        let mut problems = BTreeMap::new();
        for entry in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if id.starts_with('.') || !entry.path().is_dir() {
                continue;
            }
            match read_plugin(&entry.path(), &id, &self.platform) {
                Ok(file) => found.push(Installed { file, dir: entry.path() }),
                Err(e) => {
                    problems.insert(id, format!("{e:#}"));
                }
            }
        }
        *self.problems.lock().unwrap_or_else(|e| e.into_inner()) = problems;
        found
    }

    /// Starts what is installed and not running, stops what is running and not installed, and notices what stopped.
    /// Called every few seconds.
    pub fn sync(&self) {
        let installed = self.scan();
        let busy = self.busy.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut started = self.started.lock().unwrap_or_else(|e| e.into_inner());
        let gone: Vec<String> = started
            .keys()
            .filter(|id| !busy.contains(*id) && !installed.iter().any(|i| &i.file.id == *id))
            .cloned()
            .collect();
        for id in gone {
            started.remove(&id); // the child is killed when it is dropped
            self.registry.remove(&id);
            tracing::info!("plugin {id} was removed and is stopped");
        }
        for plugin in installed {
            let id = plugin.file.id.clone();
            if busy.contains(&id) {
                continue;
            }
            if self.registry.is_external(&id) {
                self.problems.lock().unwrap_or_else(|e| e.into_inner()).insert(
                    id.clone(),
                    format!("FAFNIRX_MODULE_{}_URL is set, so that module is used instead", id.to_ascii_uppercase()),
                );
                continue;
            }
            let now = Instant::now();
            let run = started.entry(id.clone()).or_insert_with(|| Started {
                child: None,
                problem: None,
                at: now,
                failures: 0,
                retry_at: now,
            });
            if let Some(child) = run.child.as_mut() {
                match child.try_wait() {
                    Ok(None) => continue,
                    Ok(Some(status)) => {
                        run.failures = if run.at.elapsed() > STABLE { 1 } else { run.failures + 1 };
                        run.retry_at = now + backoff(run.failures);
                        run.problem = Some(format!("it stopped ({status})"));
                        self.registry.remove(&id); // not up while it is not running
                        tracing::warn!(
                            "plugin {id} stopped ({status}); it is started again in {:?}",
                            backoff(run.failures)
                        );
                        run.child = None;
                    }
                    Err(e) => {
                        tracing::warn!("plugin {id}: {e}");
                        continue;
                    }
                }
            }
            if run.child.is_none() && now >= run.retry_at {
                match self.spawn(&plugin) {
                    Ok(child) => {
                        run.child = Some(child);
                        run.at = now;
                        run.problem = None;
                    }
                    Err(e) => {
                        run.failures += 1;
                        run.retry_at = now + backoff(run.failures);
                        run.problem = Some(format!("{e:#}"));
                        tracing::warn!("plugin {id} cannot be started: {e:#}");
                    }
                }
            }
        }
    }

    fn spawn(&self, plugin: &Installed) -> Result<Child> {
        let port = {
            let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).context("find a free port")?;
            probe.local_addr()?.port()
        };
        let listen = format!("127.0.0.1:{port}");
        let mut raw = [0u8; 24];
        getrandom::getrandom(&mut raw).map_err(|e| anyhow!("no randomness: {e}"))?;
        let token = hex::encode(raw);
        let env = child_env(&plugin.file, &plugin.dir, &self.data, &token, &listen, std::env::vars());
        let program = plugin.dir.join(&plugin.file.exec);
        let child = Command::new(&program)
            .current_dir(&plugin.dir)
            .env_clear()
            .envs(&env)
            // A pipe the module reads: when it closes the master is gone, and the module goes with it.
            .stdin(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("start {}", program.display()))?;
        self.registry.set(Module::new(&plugin.file.id, &format!("http://{listen}"), &token, true));
        tracing::info!("plugin {} {} started on {listen}", plugin.file.id, plugin.file.version);
        Ok(child)
    }

    fn mark_busy(&self, id: &str) -> Busy<'_> {
        self.busy.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string());
        Busy { plugins: self, id: id.to_string() }
    }

    /// Stops a plugin, if it runs, and waits until it has (its files cannot be replaced while it runs).
    async fn stop(&self, id: &str) {
        let run = self.started.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        if let Some(mut child) = run.and_then(|r| r.child) {
            let _ = child.kill().await;
        }
        self.registry.remove(id);
    }

    /// What the administrator sees: the plugins in the folder, and the modules set up by address.
    pub fn statuses(&self) -> Vec<Status> {
        let installed = self.scan_names();
        let mut problems = self.problems.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for (id, run) in self.started.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            if let Some(problem) = &run.problem {
                problems.insert(id.clone(), problem.clone());
            }
        }
        let mut out: Vec<Status> = installed
            .into_iter()
            .map(|(id, version)| {
                let running = self.registry.up(&id).filter(|m| m.managed);
                let missing = running.as_ref().and_then(|m| m.manifest()).map(|m| m.setup).unwrap_or_default();
                let problem = if missing.is_empty() { problems.get(&id).cloned() } else { Some(missing.join("; ")) };
                let state = if running.is_some() {
                    if missing.is_empty() {
                        "up"
                    } else {
                        "setup"
                    }
                } else if self.registry.is_external(&id) {
                    "external"
                } else if problem.is_some() {
                    "broken"
                } else {
                    "starting"
                };
                Status { id, version, state, problem }
            })
            .collect();
        for module in self.registry.all().into_iter().filter(|m| !m.managed) {
            let manifest = module.manifest();
            let missing = manifest.as_ref().map(|m| m.setup.join("; ")).filter(|m| !m.is_empty());
            out.push(Status {
                id: module.id().to_string(),
                version: manifest.as_ref().map(|m| m.version.clone()),
                state: match (&manifest, &missing) {
                    (None, _) => "external",
                    (Some(_), Some(_)) => "setup",
                    _ => "up",
                },
                problem: missing,
            });
        }
        out
    }

    /// The folders of the plugin folder, with the version each says it is (when it can be read).
    fn scan_names(&self) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if id.starts_with('.') || !entry.path().is_dir() {
                continue;
            }
            let version = read_file(&entry.path()).ok().map(|f| f.version);
            out.push((id, version));
        }
        out.sort();
        out
    }

    /// The bundles the release of this version carries for this platform.
    async fn available(&self) -> Result<Vec<Offer>> {
        let reply = self
            .http
            .get(release_url())
            .header("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .context("cannot reach the releases")?;
        if reply.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("there is no release of version {} with plugins", env!("CARGO_PKG_VERSION"));
        }
        let release: Value = reply.error_for_status()?.json().await.context("the release is not readable")?;
        Ok(offers(&release, &self.platform))
    }

    /// Downloads the plugin from the release, checks it against the checksum, and puts it in place. It starts within a few
    /// seconds; one that was installed is replaced (and restarted).
    pub async fn install(&self, id: &str) -> Result<String> {
        if !valid_id(id) {
            bail!("{id:?} cannot be a plugin");
        }
        let _one_at_a_time = self.installing.lock().await;
        let _busy = self.mark_busy(id);
        let offer = self
            .available()
            .await?
            .into_iter()
            .find(|o| o.id == id)
            .with_context(|| format!("the release has no {id} plugin for {}", self.platform))?;
        std::fs::create_dir_all(&self.dir)?;
        let archive = self.dir.join(format!(".{id}.zip"));
        let result = async {
            let want = self.checksum(&offer.checksum_url).await?;
            let got = self.download(&offer.url, &archive).await?;
            if got != want {
                bail!("the download does not match its checksum: it is damaged, try again");
            }
            let (dir, platform, name) = (self.dir.clone(), self.platform.clone(), id.to_string());
            let archive = archive.clone();
            tokio::task::spawn_blocking(move || stage(&archive, &dir, &name, &platform)).await?
        }
        .await;
        let _ = std::fs::remove_file(&archive);
        let (staging, version) = result?;
        self.stop(id).await;
        let (dir, name) = (self.dir.clone(), id.to_string());
        tokio::task::spawn_blocking(move || swap(&dir, &name, &staging)).await??;
        self.sync();
        Ok(version)
    }

    async fn checksum(&self, url: &str) -> Result<String> {
        let text = self.http.get(url).timeout(Duration::from_secs(20)).send().await?.error_for_status()?.text().await?;
        checksum_of(&text).context("the checksum file is not readable")
    }

    /// Streams `url` into `to`, and gives the SHA-256 of what came.
    async fn download(&self, url: &str, to: &Path) -> Result<String> {
        use futures_util::StreamExt;
        let reply = self.http.get(url).send().await?.error_for_status()?;
        if reply.content_length().is_some_and(|n| n > MAX_DOWNLOAD) {
            bail!("the download is larger than the {} MB allowed", MAX_DOWNLOAD >> 20);
        }
        let mut file = std::fs::File::create(to)?;
        let (mut hash, mut total) = (Sha256::new(), 0u64);
        let mut body = reply.bytes_stream();
        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            total += chunk.len() as u64;
            if total > MAX_DOWNLOAD {
                bail!("the download is larger than the {} MB allowed", MAX_DOWNLOAD >> 20);
            }
            hash.update(&chunk);
            file.write_all(&chunk)?;
        }
        Ok(hex::encode(hash.finalize()))
    }

    /// Installs the plugins `FAFNIRX_PLUGINS` names that are not in the folder, or are of another version than this master,
    /// and tries again every minute until they are. The one way to install: the web app only shows what is there.
    pub async fn ensure(self: Arc<Self>) {
        let wanted = wanted(&std::env::var("FAFNIRX_PLUGINS").unwrap_or_default());
        loop {
            let have = self.scan_names();
            let missing: Vec<&String> = wanted
                .iter()
                .filter(|id| !self.registry.is_external(id))
                .filter(|id| !have.iter().any(|(h, v)| h == *id && v.as_deref() == Some(env!("CARGO_PKG_VERSION"))))
                .collect();
            if missing.is_empty() {
                return;
            }
            for id in missing {
                match self.install(id).await {
                    Ok(version) => tracing::info!("installed the {id} plugin {version}"),
                    Err(e) => tracing::warn!("cannot install the {id} plugin, trying again in a minute: {e:#}"),
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    /// Clears what an installation that was cut short left behind.
    pub fn tidy(&self) {
        for entry in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                let path = entry.path();
                let _ = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
            }
        }
    }
}

/// Marks a plugin as being installed or removed for as long as it lives.
struct Busy<'a> {
    plugins: &'a Plugins,
    id: String,
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.plugins.busy.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.id);
    }
}

/// The ids in a `FAFNIRX_PLUGINS` value (comma separated); what cannot be a plugin is left out.
fn wanted(list: &str) -> Vec<String> {
    let mut ids: Vec<String> = list.split(',').map(|s| s.trim().to_string()).filter(|s| valid_id(s)).collect();
    ids.sort();
    ids.dedup();
    ids
}

fn backoff(failures: u32) -> Duration {
    Duration::from_secs(1u64 << failures.saturating_sub(1).min(6)).min(MAX_BACKOFF)
}

fn read_file(dir: &Path) -> Result<PluginFile> {
    let text = std::fs::read_to_string(dir.join(PLUGIN_FILE)).with_context(|| format!("read {PLUGIN_FILE}"))?;
    serde_json::from_str(&text).with_context(|| format!("{PLUGIN_FILE} is not readable"))
}

/// The plugin in `dir`, if it can run here as plugin `id`.
fn read_plugin(dir: &Path, id: &str, platform: &str) -> Result<PluginFile> {
    let file = read_file(dir)?;
    if let Some(problem) = file.problem(id, platform) {
        bail!("{problem}");
    }
    if !dir.join(&file.exec).is_file() {
        bail!("its program {} is missing", file.exec);
    }
    Ok(file)
}

/// Unpacks a bundle next to where it will live and checks it. Gives the folder it is in and its version.
fn stage(archive: &Path, plugins: &Path, id: &str, platform: &str) -> Result<(PathBuf, String)> {
    let staging = plugins.join(format!(".{id}.new"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    match unzip(archive, &staging).and_then(|()| read_plugin(&staging, id, platform)) {
        Ok(file) => Ok((staging, file.version)),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            Err(e)
        }
    }
}

/// Moves a staged bundle in place of the plugin of the same id.
fn swap(plugins: &Path, id: &str, staging: &Path) -> Result<()> {
    let target = plugins.join(id);
    let old = plugins.join(format!(".{id}.old"));
    let _ = std::fs::remove_dir_all(&old);
    if target.exists() {
        std::fs::rename(&target, &old).context("move the installed plugin aside")?;
    }
    if let Err(e) = std::fs::rename(staging, &target) {
        let _ = std::fs::rename(&old, &target);
        return Err(anyhow!("put the plugin in place: {e}"));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

fn unzip(archive: &Path, to: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?).context("the download is not a zip file")?;
    let mut unpacked = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let relative = entry
            .enclosed_name()
            .with_context(|| format!("{:?} leads out of the plugin's folder", entry.name().unwrap_or_default()))?;
        let path = to.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        unpacked += entry.size();
        if unpacked > MAX_UNPACKED {
            bail!("the bundle unpacks to more than the {} MB allowed", MAX_UNPACKED >> 20);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&path)?;
        let size = entry.size();
        std::io::copy(&mut entry.by_ref().take(size), &mut out)?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release() -> Value {
        let asset =
            |name: &str| json!({ "name": name, "browser_download_url": format!("https://example.test/{name}") });
        json!({ "tag_name": "v3.12.0", "assets": [
            asset("fafnirx-plugin-video-3.12.0-linux-x86_64.zip"),
            asset("fafnirx-plugin-video-3.12.0-linux-x86_64.zip.sha256"),
            asset("fafnirx-plugin-video-3.12.0-linux-aarch64.zip"),
            asset("fafnirx-plugin-video-3.12.0-linux-aarch64.zip.sha256"),
            asset("fafnirx-plugin-docs-3.12.0-linux-x86_64.zip"),
            asset("fafnirx-plugin-Bad-3.12.0-linux-x86_64.zip"),
            asset("fafnirx-plugin-Bad-3.12.0-linux-x86_64.zip.sha256"),
            asset("fafnirx-3.12.0-linux-x86_64"),
        ]})
    }

    #[test]
    fn the_wanted_plugins_are_the_valid_ids_of_the_list_once_each() {
        assert_eq!(wanted("video, docs,video,,../x,Bad Id"), vec!["docs", "video"]);
        assert!(wanted("").is_empty());
    }

    #[test]
    fn a_release_offers_the_bundles_for_this_platform_with_their_checksums() {
        let found = offers(&release(), "linux-x86_64");
        assert_eq!(found.len(), 1, "docs has no checksum, Bad is no id, aarch64 is another machine: {found:?}");
        assert_eq!((found[0].id.as_str(), found[0].version.as_str()), ("video", "3.12.0"));
        assert!(found[0].url.ends_with("fafnirx-plugin-video-3.12.0-linux-x86_64.zip"));
        assert!(found[0].checksum_url.ends_with(".zip.sha256"));
        assert_eq!(offers(&release(), "linux-aarch64").len(), 1);
        assert!(offers(&release(), "macos-aarch64").is_empty());
        assert!(offers(&json!({}), "linux-x86_64").is_empty());
    }

    #[test]
    fn a_checksum_file_gives_the_hash() {
        let hex = "ab".repeat(32);
        assert_eq!(checksum_of(&format!("{hex}  fafnirx-plugin.zip\n")), Some(hex.clone()));
        assert_eq!(checksum_of(&hex.to_uppercase()), Some(hex));
        for bad in ["", "abc", &"zz".repeat(32), "<html>not found</html>"] {
            assert_eq!(checksum_of(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_plugin_runs_with_its_own_settings_and_the_masters_decisions() {
        let file = PluginFile {
            id: "video".into(),
            version: "1".into(),
            contract: 1,
            platform: "linux-x86_64".into(),
            exec: "bin/fafnirx-video".into(),
            env: [("FAFNIRX_FFMPEG", "{dir}/bin/ffmpeg"), ("FAFNIRX_MODULE_TOKEN", "from the plugin")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let vars = [
            ("PATH", "/usr/bin"),
            ("FAFNIRX_VIDEO_JOBS", "4"),
            ("FAFNIRX_DOCS_JOBS", "9"),
            ("FAFNIRX_MODULE_VIDEO_TOKEN", "the master's own"),
            ("DATABASE_URL", "postgres://secret"),
            ("FAFNIRX_ADMIN_PASSWORD", "secret"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let env = child_env(&file, Path::new("/data/plugins/video"), Path::new("/data"), "tok", "127.0.0.1:9", vars);
        assert_eq!(env["FAFNIRX_FFMPEG"], "/data/plugins/video/bin/ffmpeg");
        assert_eq!(env["FAFNIRX_VIDEO_JOBS"], "4");
        assert_eq!(env["FAFNIRX_MODULE_TOKEN"], "tok", "the plugin cannot choose its token");
        assert_eq!((env["FAFNIRX_MODULE_LISTEN"].as_str(), env["FAFNIRX_DATA"].as_str()), ("127.0.0.1:9", "/data"));
        assert!(env.contains_key("PATH"));
        for hidden in ["FAFNIRX_DOCS_JOBS", "FAFNIRX_MODULE_VIDEO_TOKEN", "DATABASE_URL", "FAFNIRX_ADMIN_PASSWORD"] {
            assert!(!env.contains_key(hidden), "{hidden} is not for the plugin");
        }
    }

    #[test]
    fn a_plugin_that_keeps_stopping_is_started_less_often() {
        let waits: Vec<u64> = (1..=9).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60, 60]);
    }

    fn bundle(dir: &Path, entries: &[(&str, &str)]) -> PathBuf {
        let path = dir.join("bundle.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, content) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default().unix_permissions(0o755)).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn manifest(id: &str, version: &str) -> String {
        json!({ "id": id, "version": version, "contract": 1, "platform": "linux-x86_64", "exec": "bin/run" })
            .to_string()
    }

    /// Installs a bundle the way `install` does, once it is downloaded.
    fn unpack(archive: &Path, plugins: &Path, id: &str, platform: &str) -> Result<String> {
        let (staging, version) = stage(archive, plugins, id, platform)?;
        swap(plugins, id, &staging)?;
        Ok(version)
    }

    #[test]
    fn a_bundle_is_checked_before_it_replaces_the_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins = tmp.path().join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();

        let good = bundle(tmp.path(), &[(PLUGIN_FILE, &manifest("video", "1.0.0")), ("bin/run", "#!/bin/sh")]);
        assert_eq!(unpack(&good, &plugins, "video", "linux-x86_64").unwrap(), "1.0.0");
        assert!(plugins.join("video/bin/run").is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(plugins.join("video/bin/run")).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "the program can be run");
        }

        // Each of these leaves the installed 1.0.0 as it is, and nothing behind.
        let cases = [
            ("another id", bundle(tmp.path(), &[(PLUGIN_FILE, &manifest("docs", "2.0.0")), ("bin/run", "x")])),
            ("no program", bundle(tmp.path(), &[(PLUGIN_FILE, &manifest("video", "2.0.0"))])),
            ("no plugin.json", bundle(tmp.path(), &[("bin/run", "x")])),
            ("leads out", bundle(tmp.path(), &[(PLUGIN_FILE, &manifest("video", "2.0.0")), ("../evil", "x")])),
        ];
        for (what, archive) in cases {
            assert!(unpack(&archive, &plugins, "video", "linux-x86_64").is_err(), "{what}");
            let on_disk: PluginFile = read_file(&plugins.join("video")).unwrap();
            assert_eq!(on_disk.version, "1.0.0", "{what}");
        }
        assert!(!tmp.path().join("evil").exists());
        let leftovers: Vec<_> = std::fs::read_dir(&plugins).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(leftovers, ["video"], "no staging folder is left");

        // A newer one replaces it.
        let newer = bundle(tmp.path(), &[(PLUGIN_FILE, &manifest("video", "2.0.0")), ("bin/run", "x")]);
        assert_eq!(unpack(&newer, &plugins, "video", "linux-x86_64").unwrap(), "2.0.0");
        assert_eq!(read_file(&plugins.join("video")).unwrap().version, "2.0.0");
    }
}
