//! The configuration file: what an operator decides (settings, devices, mirrors, sources) lives in one JSON file
//! that can be edited by hand, kept in version control, or mounted into the container, so that a master can be
//! set up again from it. The admin web app changes the same data, and the file follows it:
//!
//! * the file is read at start, and the database is brought in line with it. A file that cannot be applied stops
//!   the master, so a mistake is found at once; an edit made while the master runs waits for the next start;
//! * a change made in the web app (or a new local folder found in `files/`) is written to the file.
//!
//! The database keeps what is only ever a consequence of running (files, versions, run results, device
//! activity), and the file keeps the decisions. A section the file does not have (`"mirrors"` left out) is not
//! managed: the database keeps what it has, and the file gets the section the next time it is written. Credentials
//! are in the file in the clear, as they are in the database, so it is written readable by its owner only.
//!
//! Environment:
//!   FAFNIRX_CONFIG          the file (default `<FAFNIRX_DATA>/fafnirx.json`)
//!   FAFNIRX_RCLONE_CONFIG   the rclone config file that mirrors with a `remote` use (default `rclone.conf` next to it)
//!   FAFNIRX_CONFIG_POLL_SECS  how often the database is looked at for changes to write to the file (default 2)

mod database;

use crate::integrations::api::{hours_to_secs, parse_folder_ids, parse_service_account, SOURCE_PREFIX};
use crate::platform::db::{self, Db};
use crate::platform::store::Store;
use anyhow::{anyhow, bail, Context, Result};
use fafnirx_common::valid_device_name;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify};

pub const VERSION: u32 = 1;

fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("FAFNIRX_DATA").unwrap_or_else(|_| "data".into()))
}

/// The configuration file.
pub fn path() -> PathBuf {
    std::env::var_os("FAFNIRX_CONFIG").map(PathBuf::from).unwrap_or_else(|| data_dir().join("fafnirx.json"))
}

/// The rclone config file that mirrors with a `remote` read.
pub fn rclone_config() -> PathBuf {
    std::env::var_os("FAFNIRX_RCLONE_CONFIG").map(PathBuf::from).unwrap_or_else(|| path().with_file_name("rclone.conf"))
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Settings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<DeviceEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirrors: Option<Vec<MirrorEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<SourceEntry>>,
}

fn version() -> u32 {
    VERSION
}

/// A setting the file leaves out keeps its value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Versions kept per file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_count: Option<i64>,
    /// Days a deleted file and the versions beyond the count are kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<i64>,
    /// Seconds without news from a device before it counts as offline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offline_after_secs: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeviceEntry {
    pub name: String,
    /// `device` (a client with a token) or `local` (a folder on the master).
    #[serde(default = "device_kind")]
    pub kind: String,
    /// Hex SHA-256 of the device's token: all the master keeps of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_hash: Option<String>,
    /// A token written by hand. It is replaced by its hash as soon as the file is read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub revoked: bool,
}

fn device_kind() -> String {
    "device".into()
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MirrorEntry {
    pub name: String,
    /// A remote of the rclone config file, as `name:path`. When set, the three fields below are not used.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remote: String,
    /// An rclone backend (`s3`, `sftp`, `webdav`, `b2`, ...) configured by `options`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remote_type: String,
    /// The bucket and prefix inside the backend.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remote_path: String,
    /// rclone backend options (`access_key_id`, `region`, ...), as rclone's lowercase names.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, Value>,
    #[serde(default = "mirror_hours")]
    pub interval_hours: f64,
    /// Runs of history (replaced and removed files) kept on the mirror.
    #[serde(default = "mirror_keep")]
    pub keep_runs: i64,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn mirror_hours() -> f64 {
    72.0
}
fn mirror_keep() -> i64 {
    3
}
fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceEntry {
    pub name: String,
    #[serde(default = "source_kind")]
    pub kind: String,
    /// Drive folder ids (a pasted folder URL is reduced to its id).
    pub folders: Vec<String>,
    /// The service account key: `client_email` and `private_key`.
    pub service_account: Value,
    #[serde(default = "source_hours")]
    pub interval_hours: f64,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn source_kind() -> String {
    "gdrive".into()
}
fn source_hours() -> f64 {
    24.0
}

/// A mirror's `remote`: the name of a remote of the rclone config, a colon, and a path inside it. The name starts
/// with a letter or digit, so that it can never be taken for an option.
pub fn valid_remote(remote: &str) -> bool {
    let Some((name, _)) = remote.split_once(':') else { return false };
    name.len() <= 64
        && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+' | '@' | ' '))
        && !remote.contains(['\0', '\n', '\r'])
}

/// The name of an rclone backend option: what rclone calls it in lowercase, with `_` for `-`.
pub fn option_name(key: &str) -> Option<String> {
    let key = key.trim().to_lowercase().replace('-', "_");
    let ok = !key.is_empty()
        && key.len() <= 64
        && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    ok.then_some(key)
}

fn name_rule(name: &str) -> Result<()> {
    if !valid_device_name(name) {
        bail!(
            "{name:?} is not a valid name: 1-63 characters, lowercase letters, digits, '.', '_' or '-'; not a reserved Windows name"
        );
    }
    Ok(())
}

fn api<T>(r: std::result::Result<T, crate::web::ApiError>) -> Result<T> {
    r.map_err(|e| anyhow!(e.1))
}

fn no_duplicates<'a>(what: &str, names: impl Iterator<Item = &'a String>) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for n in names {
        if !seen.insert(n.as_str()) {
            bail!("{what} {n:?} is listed twice");
        }
    }
    Ok(())
}

impl Settings {
    fn check(&self) -> Result<()> {
        let range = |name: &str, v: Option<i64>, lo: i64, hi: i64| match v {
            Some(v) if !(lo..=hi).contains(&v) => bail!("{name} must be between {lo} and {hi}"),
            _ => Ok(()),
        };
        range("version_count", self.version_count, 1, 1000)?;
        range("retention_days", self.retention_days, 1, 3650)?;
        range("offline_after_secs", self.offline_after_secs, 5, 86_400)
    }
}

impl DeviceEntry {
    fn normalize(&mut self) -> Result<()> {
        name_rule(&self.name)?;
        match self.kind.as_str() {
            "device" => {
                if let Some(token) = self.token.take().filter(|t| !t.is_empty()) {
                    self.token_hash = Some(db::hash_token(&token));
                }
                let hash = self.token_hash.take().unwrap_or_default().to_lowercase();
                if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                    bail!("a device needs a token_hash (the hex SHA-256 of its token), or a token to make one from");
                }
                self.token_hash = Some(hash);
            }
            "local" => {
                // A local folder has no token and nothing to revoke.
                self.token = None;
                self.token_hash = None;
                self.revoked = false;
            }
            other => bail!("kind {other:?} is not device or local"),
        }
        Ok(())
    }
}

impl MirrorEntry {
    pub fn normalize(&mut self) -> Result<()> {
        name_rule(&self.name)?;
        self.remote = self.remote.trim().to_string();
        if !self.remote.is_empty() {
            if !valid_remote(&self.remote) {
                bail!("remote {:?} must be a remote of the rclone config as name:path", self.remote);
            }
            self.remote_type.clear();
            self.remote_path.clear();
            self.options.clear();
        } else {
            self.remote_type = self.remote_type.trim().to_lowercase();
            if self.remote_type.is_empty()
                || self.remote_type.len() > 32
                || !self.remote_type.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            {
                bail!("set remote (a remote of the rclone config), or remote_type (an rclone backend such as s3, sftp, webdav, b2)");
            }
            self.remote_path = self.remote_path.trim().to_string();
            let mut options = BTreeMap::new();
            for (key, value) in std::mem::take(&mut self.options) {
                let name =
                    option_name(&key).ok_or_else(|| anyhow!("option {key:?}: use lowercase letters, digits and _"))?;
                let text = match value {
                    Value::String(s) => s,
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => bail!("option {key:?} must be a string, a number or true/false"),
                };
                if text.contains('\0') {
                    bail!("option {key:?} has an invalid value");
                }
                options.insert(name, Value::String(text));
            }
            self.options = options;
        }
        self.interval_hours = api(hours_to_secs(self.interval_hours))? as f64 / 3600.0;
        if !(1..=100).contains(&self.keep_runs) {
            bail!("keep_runs must be between 1 and 100");
        }
        Ok(())
    }
}

impl SourceEntry {
    fn normalize(&mut self) -> Result<()> {
        name_rule(&self.name)?;
        if !valid_device_name(&format!("{SOURCE_PREFIX}{}", self.name)) {
            bail!("the name is too long for a source");
        }
        if self.kind != "gdrive" {
            bail!("kind {:?} is not gdrive", self.kind);
        }
        self.folders = api(parse_folder_ids(&self.folders.join("\n")))?;
        self.service_account = api(parse_service_account(&self.service_account.to_string()))?;
        self.interval_hours = api(hours_to_secs(self.interval_hours))? as f64 / 3600.0;
        Ok(())
    }
}

impl Config {
    /// Reads and checks a file's text. What comes back is in its normal form (sorted, with the defaults filled in
    /// and the same spelling the database gives back), so that two configurations compare equal when they mean the
    /// same.
    pub fn parse(text: &str) -> Result<Config> {
        let mut config: Config = serde_json::from_str(text).context("not a valid configuration")?;
        config.normalize()?;
        Ok(config)
    }

    fn normalize(&mut self) -> Result<()> {
        if self.version != VERSION {
            bail!("version {} is not known to this master (it reads version {VERSION})", self.version);
        }
        if let Some(s) = &self.settings {
            s.check().context("settings")?;
        }
        if let Some(devices) = &mut self.devices {
            for (i, d) in devices.iter_mut().enumerate() {
                d.normalize().with_context(|| format!("devices[{i}] {:?}", d.name))?;
            }
            devices.sort_by(|a, b| a.name.cmp(&b.name));
            no_duplicates("device", devices.iter().map(|d| &d.name))?;
        }
        if let Some(mirrors) = &mut self.mirrors {
            for (i, m) in mirrors.iter_mut().enumerate() {
                m.normalize().with_context(|| format!("mirrors[{i}] {:?}", m.name))?;
            }
            mirrors.sort_by(|a, b| a.name.cmp(&b.name));
            no_duplicates("mirror", mirrors.iter().map(|m| &m.name))?;
        }
        if let Some(sources) = &mut self.sources {
            for (i, s) in sources.iter_mut().enumerate() {
                s.normalize().with_context(|| format!("sources[{i}] {:?}", s.name))?;
            }
            sources.sort_by(|a, b| a.name.cmp(&b.name));
            no_duplicates("source", sources.iter().map(|s| &s.name))?;
        }
        Ok(())
    }

    fn to_text(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("a configuration is JSON");
        text.push('\n');
        text
    }
}

/// The file, and the process that writes it when the database changes.
///
/// The file is read once, at start. A file that cannot be applied stops the master, so a mistake is found at once
/// and has to be fixed, and a running master never half-applies an edit. An edit made while the master runs is
/// applied at the next start, and until then the master does not write over it.
pub struct ConfigFile {
    path: PathBuf,
    nudge: Notify,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// Hash of the file as last read or written by this process.
    hash: Option<[u8; 32]>,
    /// What the file said then.
    config: Option<Config>,
    /// A failure to write was already logged.
    write_failed: bool,
    /// The state of the database that was last reported as not written because of an edit waiting for a restart.
    reported: Option<Config>,
}

fn hash_of(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn blank(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_whitespace)
}

fn read(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b).filter(|b| !blank(b))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Replaces the file in one step. A file that is a mount of its own (a bind mount of a single file) cannot be
/// replaced, only written to, so that is the fallback.
fn write(path: &Path, data: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let staged = options.open(&tmp).and_then(|mut f| f.write_all(data.as_bytes()).and_then(|()| f.sync_all()));
    if staged.is_ok() && std::fs::rename(&tmp, path).is_ok() {
        return Ok(());
    }
    let _ = std::fs::remove_file(&tmp);
    // Written in place: not atomic, but the file keeps its place and is never more open than its owner.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(data.as_bytes())?;
    #[cfg(unix)]
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    Ok(())
}

impl ConfigFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path, nudge: Notify::new(), state: Mutex::default() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Asks for the file to be brought up to date now, rather than at the next look.
    pub fn nudge(&self) {
        self.nudge.notify_one();
    }

    /// Reads the file and applies it, at start. A file that cannot be applied is an error that stops the master, so
    /// that it never runs on a configuration other than the one it was given. Then the file is written if it lacks
    /// something the database has (the sections it left out, the hash of a token written in the clear).
    pub async fn start(&self, db: &Db, store: &Arc<Store>) -> Result<()> {
        {
            let mut st = self.state.lock().await;
            let bytes = read(&self.path).with_context(|| format!("read {}", self.path.display()))?;
            if let Some(bytes) = bytes {
                let config = async {
                    let text = std::str::from_utf8(&bytes).context("the file is not UTF-8 text")?;
                    let config = Config::parse(text)?;
                    let (c, s) = (config.clone(), store.clone());
                    db::with(db, move |conn| database::apply(conn, &s, &c)).await?;
                    Ok::<_, anyhow::Error>(config)
                }
                .await
                .with_context(|| format!("{} cannot be applied; fix it and start again", self.path.display()))?;
                tracing::info!("configuration read from {}", self.path.display());
                st.hash = Some(hash_of(&bytes));
                st.config = Some(config);
            }
        }
        self.sync(db).await
    }

    /// Writes the file if the database has something it does not say. Never touches a file that someone edited since
    /// it was last read or written: that edit is for the next start.
    pub async fn sync(&self, db: &Db) -> Result<()> {
        let mut st = self.state.lock().await;
        let bytes = read(&self.path).with_context(|| format!("read {}", self.path.display()))?;
        let hash = bytes.as_deref().map(hash_of);
        if hash.is_some() && hash != st.hash {
            // The edit is for the next start; what the web app changes meanwhile is not written, and the restart
            // undoes it. Say so each time there is something new to lose.
            let exported = db::with(db, database::export).await?;
            if st.config.as_ref() != Some(&exported) && st.reported.as_ref() != Some(&exported) {
                tracing::warn!(
                    "{} was edited while the master runs and the web app has changed things since: those changes are not written to it and are lost when the master restarts. Restart the master now to apply the edit",
                    self.path.display()
                );
                st.reported = Some(exported);
            } else if st.reported.is_none() {
                tracing::warn!(
                    "{} was edited while the master runs; restart the master to apply it",
                    self.path.display()
                );
                st.reported = Some(exported);
            }
            return Ok(());
        }
        let exported = db::with(db, database::export).await?;
        if bytes.is_some() && st.config.as_ref() == Some(&exported) {
            return Ok(());
        }
        let text = exported.to_text();
        match write(&self.path, &text) {
            Ok(()) => {
                st.hash = Some(hash_of(text.as_bytes()));
                st.config = Some(exported);
                st.write_failed = false;
                st.reported = None;
            }
            Err(e) if !st.write_failed => {
                st.write_failed = true;
                tracing::warn!("cannot write {} (is it mounted read-only?): {e}", self.path.display());
            }
            Err(_) => {}
        }
        Ok(())
    }

    /// Writes the file whenever the database changes it, for the life of the process.
    pub fn spawn(self: Arc<Self>, db: Db) {
        let poll = Duration::from_secs(
            std::env::var("FAFNIRX_CONFIG_POLL_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(2u64).max(1),
        );
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = self.nudge.notified() => {}
                    () = tokio::time::sleep(poll) => {}
                }
                if let Err(e) = self.sync(&db).await {
                    tracing::error!("configuration file: {e:#}");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests;
