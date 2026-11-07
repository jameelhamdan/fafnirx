//! TOML configuration and the per-OS locations of config, state and secrets.

pub mod credentials;
mod paths;

pub use paths::*;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where a folder of this computer goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// This computer's own namespace on the master: the folder is only uploaded (a backup), the client never writes to it.
    #[default]
    Computer,
    /// A folder on the master: one of the user's, another user's shared with them, or `master:<path>`. Changes go the way its
    /// mode says. It is not also uploaded to this computer's namespace.
    Folder,
}

/// A local folder and where it goes. Every folder is the same thing: the target and the mode are the only differences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub path: PathBuf,
    /// With the target `computer`: the top-level directory of this folder inside the computer's namespace. With `folder`: the
    /// folder on the master, a synced folder of the user, `<owner>/<name>`, or `master:<path>`.
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub target: Target,
    /// The stable address of the folder on the master, which keeps working when it is renamed there. Set when attaching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Which way it goes, and what happens to a file changed here and on the master (or changed here and deleted there). A
    /// folder that goes to the computer's namespace only ever goes up.
    #[serde(default, skip_serializing_if = "is_default")]
    pub mode: Mode,
    /// With `ask`: how long a file waits for an answer; after that both versions are kept. 0: for ever.
    #[serde(default = "default_ask_timeout", skip_serializing_if = "is_default_ask_timeout")]
    pub ask_timeout_hours: u64,
}

impl Folder {
    /// A folder uploaded to this computer's namespace.
    pub fn computer(path: PathBuf, name: String) -> Self {
        Self {
            path,
            name,
            target: Target::Computer,
            id: None,
            mode: Mode::Up,
            ask_timeout_hours: default_ask_timeout(),
        }
    }

    pub fn is_computer(&self) -> bool {
        self.target == Target::Computer
    }

    /// What the master is asked for: the stable id when there is one, else the name.
    pub fn key(&self) -> &str {
        self.id.as_deref().unwrap_or(&self.name)
    }
}

/// Which way a folder on the master is synced, and who wins when a file changed here and on the master. One setting: the default
/// is what most people want, and the others are for the few who do not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Both ways. A file changed on both sides becomes the master's; this computer's goes aside in `.fafnirx-conflicts`, unsent.
    #[default]
    Both,
    /// What changes here goes to the master; nothing comes back, and nothing is deleted here.
    Up,
    /// Both ways. A file changed on both sides is kept in both versions, this computer's under `name (conflict <computer> <date>).ext`.
    Keep,
    /// Both ways. A person is asked about a file changed on both sides (a question on screen); until then nothing changes.
    Ask,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Both, Mode::Up, Mode::Keep, Mode::Ask];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Both => "both",
            Mode::Up => "up",
            Mode::Keep => "keep",
            Mode::Ask => "ask",
        }
    }

    /// Whether the master's changes are received.
    pub fn pulls(self) -> bool {
        self != Mode::Up
    }

    /// Whether this build can show the question of `ask` at all (it has the tray). The service uses this: it runs without a display.
    pub fn ask_possible() -> bool {
        cfg!(any(target_os = "windows", target_os = "macos", feature = "tray"))
    }

    /// `ask` needs a dialog: a computer with no screen cannot offer it. On Linux that is a session with a display.
    pub fn ask_available() -> bool {
        Self::ask_possible() && (cfg!(any(target_os = "windows", target_os = "macos")) || std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some())
    }

    /// Refuses `ask` where it cannot be answered.
    pub fn check_available(self) -> Result<()> {
        if self == Mode::Ask && !Self::ask_available() {
            bail!("\"ask\" needs a screen to show the question on: it is not available on a computer without one");
        }
        Ok(())
    }
}

impl std::str::FromStr for Mode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Mode::ALL
            .into_iter()
            .find(|m| m.name() == s.to_ascii_lowercase())
            .with_context(|| format!("\"{s}\" is not one of {}", Mode::ALL.map(Mode::name).join(", ")))
    }
}

fn is_default_ask_timeout(hours: &u64) -> bool {
    *hours == default_ask_timeout()
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// `http://host:7443` on a trusted network, `https://...` through a TLS proxy such as a Cloudflare tunnel.
    pub server: String,
    /// The client looks for new releases now and then and installs them. Off: it stays on its version until `fafnirx update`
    /// is run by hand.
    #[serde(default = "yes")]
    pub auto_update: bool,
    /// Upload limit in KiB/s, 0 for unlimited.
    #[serde(default)]
    pub bandwidth_limit_kbps: u64,
    /// React to file changes with a filesystem watcher.
    #[serde(default = "yes")]
    pub watch: bool,
    /// Full scan interval in seconds, 0 disables periodic scans.
    #[serde(default = "default_scan")]
    pub scan_interval_secs: u64,
    /// How long the deletion of a large share of a folder's files is held back before it is sent to the master.
    #[serde(default = "default_hold")]
    pub delete_hold_hours: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_dir: Option<PathBuf>,
    /// Web UI of the master, opened from the tray menu. Default: port 8080 on the server's host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
    #[serde(default)]
    pub folders: Vec<Folder>,
}

fn yes() -> bool {
    true
}
fn default_scan() -> u64 {
    3600
}
fn default_hold() -> u64 {
    crate::sync::guard::DEFAULT_HOLD_HOURS
}
fn default_ask_timeout() -> u64 {
    24
}

impl Config {
    /// A config with the defaults and no folders yet.
    pub fn new(server: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            auto_update: true,
            bandwidth_limit_kbps: 0,
            watch: true,
            scan_interval_secs: default_scan(),
            delete_hold_hours: default_hold(),
            state_dir: None,
            web_url: None,
            folders: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
        let mut cfg: Config = toml_edit::de::from_str(&text).with_context(|| format!("parse config {}", path.display()))?;
        for f in cfg.folders.iter_mut().filter(|f| f.is_computer()) {
            f.mode = Mode::Up;
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_atomic(path, &toml_edit::ser::to_string_pretty(self)?)
    }

    pub fn validate(&self) -> Result<()> {
        if self.host().is_none() {
            bail!("server must be an http:// or https:// URL");
        }
        let mut computer = std::collections::HashSet::new();
        let mut attached = std::collections::HashSet::new();
        for f in &self.folders {
            if f.is_computer() {
                if f.name.is_empty() || f.name.contains(['/', '\\']) || f.name == "." || f.name == ".." {
                    bail!("folder name {:?} must be a single path component", f.name);
                }
                if !computer.insert(fafnirx_common::fold_path(&f.name)) {
                    bail!(
                        "two folders are called {:?}: the master keeps them side by side under this name, and names that differ \
                         only in case or accents are the same there. Give one a different name, for example {:?}",
                        f.name,
                        format!("{}-2", f.name)
                    );
                }
                continue;
            }
            if let Some(path) = f.name.strip_prefix(MASTER_KEY) {
                if fafnirx_common::clean_remote_path(path).is_err() {
                    bail!("master folder name {:?} must be `master:` and a path in the master files", f.name);
                }
            } else {
                // The name of one's own folder, or `<owner>/<name>` for one another user synced.
                let (owner, name) = f.name.split_once('/').unwrap_or(("a", &f.name));
                if !fafnirx_common::valid_device_name(owner) || !fafnirx_common::valid_device_name(name) {
                    bail!("synced folder name {:?} must be lowercase letters, digits, '.', '_' and '-'", f.name);
                }
            }
            if !attached.insert(f.name.as_str()) {
                bail!(
                    "the folder {:?} is attached twice; a folder of the master is attached to one folder of a computer",
                    f.name
                );
            }
        }
        Ok(())
    }

    /// The folders uploaded to this computer's own namespace.
    pub fn computer_folders(&self) -> impl Iterator<Item = &Folder> {
        self.folders.iter().filter(|f| f.is_computer())
    }

    /// The folders attached to a folder of the master.
    pub fn attached_folders(&self) -> impl Iterator<Item = &Folder> {
        self.folders.iter().filter(|f| !f.is_computer())
    }

    /// Every local directory the client collects or syncs.
    pub fn paths(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.folders.iter().map(|f| (f.name.as_str(), f.path.as_path()))
    }

    /// The host of the server URL, if it is an http(s) URL.
    pub fn host(&self) -> Option<&str> {
        let rest = self.server.strip_prefix("https://").or_else(|| self.server.strip_prefix("http://"))?;
        let host = rest.split(['/', ':']).next().filter(|h| !h.is_empty())?;
        Some(host)
    }

    /// Plain http to a host that is not on a private network: the token and the data cross the internet
    /// unencrypted. Use https (a tunnel or reverse proxy) there.
    pub fn is_plain_over_internet(&self) -> bool {
        self.server.starts_with("http://") && self.host().is_some_and(|h| !is_private_host(h))
    }

    /// Where the master's web UI is, for the tray menu.
    #[cfg(feature = "tray")]
    pub fn dashboard_url(&self) -> String {
        if let Some(u) = self.web_url.as_deref().filter(|u| u.starts_with("http://") || u.starts_with("https://")) {
            return u.to_string();
        }
        let host = self.host().unwrap_or("localhost");
        format!("http://{host}:8080")
    }

    pub fn state_dir(&self, config_path: &Path) -> PathBuf {
        self.state_dir.clone().unwrap_or_else(|| default_state_dir(config_path))
    }
}

/// Hosts that are not reachable from the internet: names without a dot, `.local`/`.lan`/`.internal`
/// names, loopback, link-local and private IPv4 ranges.
fn is_private_host(host: &str) -> bool {
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        return ip.is_private() || ip.is_loopback() || ip.is_link_local();
    }
    !host.contains('.') || [".local", ".lan", ".internal", ".home.arpa"].iter().any(|s| host.ends_with(s))
}

/// The address of the master as a person types it (`nas.lan`, `nas.lan:7443`, `https://fafnirx.example.com`) as the URL the client
/// uses: without a scheme it is plain http, for a trusted network, and without a port the one the devices use.
pub fn normalize_server(input: &str) -> String {
    let s = input.trim();
    if s.contains("://") {
        return s.trim_end_matches('/').to_string();
    }
    let s = s.trim_end_matches('/');
    let has_port = s
        .rsplit_once(':')
        .is_some_and(|(_, port)| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()));
    if has_port {
        format!("http://{s}")
    } else {
        format!("http://{s}:7443")
    }
}

/// Writes through a temporary file so a reader (the daemon) never sees half a config.
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    fafnirx_common::write_atomic(path, text).with_context(|| format!("write {}", path.display()))
}

/// Changes the config file as a TOML document, so that its comments and layout stay: `change` edits the document, which is
/// then written whole (nothing is written when it fails).
fn edit_file<T>(path: &Path, change: impl FnOnce(&mut toml_edit::DocumentMut) -> Result<T>) -> Result<T> {
    let text = std::fs::read_to_string(path)?;
    let mut doc: toml_edit::DocumentMut = text.parse().with_context(|| format!("parse {}", path.display()))?;
    let out = change(&mut doc)?;
    write_atomic(path, &doc.to_string())?;
    Ok(out)
}

/// The `[[key]]` tables of the document, made when there are none.
fn tables<'a>(doc: &'a mut toml_edit::DocumentMut, key: &str) -> Result<&'a mut toml_edit::ArrayOfTables> {
    // `install` writes an empty `folders = []`; that has to become a list of tables first.
    if doc.get(key).and_then(|f| f.as_array()).is_some_and(|a| a.is_empty()) {
        doc.remove(key);
    }
    doc.entry(key)
        .or_insert(toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .with_context(|| format!("`{key}` in the config must be a list of [[{key}]] tables"))
}

/// The start of the name of a master folder: a subfolder of the master files.
pub const MASTER_KEY: &str = "master:";

/// Whether a `[[folders]]` table is the one called `name`.
fn is_named(table: &toml_edit::Table, name: &str) -> bool {
    table.get("name").and_then(|n| n.as_str()) == Some(name)
}

pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// What to add to the config: the folder, where it goes, and for a folder of the master its stable id and mode.
pub struct NewFolder<'a> {
    pub dir: &'a Path,
    pub target: Target,
    /// With the target `computer`: the name in the computer's namespace (default: the directory's own, made unique). With
    /// `folder`: the folder of the master.
    pub name: Option<&'a str>,
    pub id: Option<&'a str>,
    pub mode: Mode,
}

/// Adds a folder to the config file, keeping everything else in the file (comments included) as it is. A folder of the master is
/// made on this computer if it is not there yet: the usual way to set up another computer is to attach a new, empty folder and let
/// it fill. Returns the folder as stored.
pub fn add_folder_to_file(config_path: &Path, new: NewFolder) -> Result<Folder> {
    let cfg = Config::load(config_path).context("run `fafnirx install --server <address>` first")?;
    let computer = new.target == Target::Computer;
    if !computer {
        std::fs::create_dir_all(new.dir).with_context(|| format!("cannot make {}", new.dir.display()))?;
    }
    let path = free_directory(&cfg, new.dir)?;
    let name = match new.name {
        Some(n) => n.to_string(),
        None if computer => {
            let base = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .filter(|n| !n.is_empty())
                .context("this folder has no name of its own; choose one with --name")?;
            let taken: std::collections::HashSet<String> = cfg.computer_folders().map(|f| fafnirx_common::fold_path(&f.name)).collect();
            let mut candidate = base.clone();
            let mut n = 2;
            while taken.contains(&fafnirx_common::fold_path(&candidate)) {
                candidate = format!("{base}-{n}");
                n += 1;
            }
            candidate
        }
        None => bail!("name the folder of the master to attach"),
    };
    let folder = Folder {
        path,
        name,
        target: new.target,
        id: new.id.map(str::to_string),
        mode: if computer { Mode::Up } else { new.mode },
        ask_timeout_hours: default_ask_timeout(),
    };
    let mut check = cfg;
    check.folders.push(folder.clone());
    check.validate()?;

    edit_file(config_path, |doc| {
        let mut table = toml_edit::Table::new();
        table["path"] = toml_edit::value(folder.path.to_string_lossy().into_owned());
        table["name"] = toml_edit::value(folder.name.clone());
        if !computer {
            table["target"] = toml_edit::value("folder");
        }
        if let Some(id) = &folder.id {
            table["id"] = toml_edit::value(id.clone());
        }
        if !computer && folder.mode != Mode::default() {
            table["mode"] = toml_edit::value(folder.mode.name());
        }
        tables(doc, "folders")?.push(table);
        Ok(())
    })?;
    Ok(folder)
}

/// The directory as the client keeps it, if it can be collected or synced: it exists and is not, contain or lie inside
/// a directory that already is.
fn free_directory(cfg: &Config, dir: &Path) -> Result<PathBuf> {
    let path = fafnirx_common::plain_path(&std::fs::canonicalize(dir).with_context(|| format!("cannot read {}", dir.display()))?);
    if !path.is_dir() {
        bail!("{} is not a directory", path.display());
    }
    for (name, other) in cfg.paths() {
        if same_path(other, &path) {
            bail!("{} is already collected as \"{}\"", path.display(), name);
        }
        if path.starts_with(other) || other.starts_with(&path) {
            bail!("{} overlaps {} (collected as \"{}\"); collect one of them", path.display(), other.display(), name);
        }
    }
    Ok(path)
}

/// Sets the mode of a folder on the master (which way it goes, and what happens to a file changed on both sides), and with `Ask` how
/// long a file waits for an answer. Returns the name of the folder in full.
pub fn set_mode_in_file(config_path: &Path, name: &str, mode: Mode, ask_timeout_hours: Option<u64>) -> Result<String> {
    let full = Config::load(config_path)?.attached_named(name)?.name.clone();
    edit_file(config_path, |doc| {
        let Some(list) = doc.get_mut("folders").and_then(|s| s.as_array_of_tables_mut()) else { return Ok(()) };
        for table in list.iter_mut().filter(|t| is_attached(t, &full)) {
            if mode == Mode::default() {
                table.remove("mode");
            } else {
                table["mode"] = toml_edit::value(mode.name());
            }
            match ask_timeout_hours {
                Some(h) if h != default_ask_timeout() => table["ask_timeout_hours"] = toml_edit::value(h as i64),
                Some(_) => {
                    table.remove("ask_timeout_hours");
                }
                None => {}
            }
        }
        Ok(())
    })?;
    Ok(full)
}

/// Brings the names in the config up to date with the master: a folder attached by its id that was renamed there is called by its
/// new name here too (the state and the connection follow the id, so nothing is lost). `listed` is (name, id) as the master says.
/// Returns the (old, new) names that changed.
pub fn follow_renames_in_file(config_path: &Path, listed: &[(String, String)]) -> Result<Vec<(String, String)>> {
    let cfg = Config::load(config_path)?;
    let changes: Vec<(String, String)> = cfg
        .attached_folders()
        .filter_map(|s| {
            let id = s.id.as_deref()?;
            let (name, _) = listed.iter().find(|(_, listed_id)| listed_id == id)?;
            (*name != s.name).then(|| (s.name.clone(), name.clone()))
        })
        .collect();
    if changes.is_empty() {
        return Ok(changes);
    }
    edit_file(config_path, |doc| {
        let Some(list) = doc.get_mut("folders").and_then(|s| s.as_array_of_tables_mut()) else { return Ok(()) };
        for (old, new) in &changes {
            for table in list.iter_mut().filter(|t| is_attached(t, old)) {
                table["name"] = toml_edit::value(new.clone());
            }
        }
        Ok(())
    })?;
    Ok(changes)
}

/// Whether `name`, as a person types it, means the folder of the master `full` (`<owner>/<name>`): in full, or by its name alone.
pub fn names_synced(full: &str, name: &str) -> bool {
    full == name || (!name.contains('/') && !full.starts_with(MASTER_KEY) && full.rsplit('/').next() == Some(name))
}

impl Config {
    /// The folder a person means by `name`: one of the master (see `attached_named`) or one collected here.
    pub fn folder_named(&self, name: &str) -> Option<&Folder> {
        self.attached_folders()
            .find(|f| names_synced(&f.name, name))
            .or_else(|| self.computer_folders().find(|f| f.name == name))
    }

    /// The folder of the master a person means by `name`: in full, or by its name alone when only one has it.
    pub fn attached_named(&self, name: &str) -> Result<&Folder> {
        let found: Vec<&Folder> = self.attached_folders().filter(|s| names_synced(&s.name, name)).collect();
        match found.as_slice() {
            [one] => Ok(one),
            [] => bail!("no synced folder named \"{name}\" is attached"),
            many => bail!(
                "\"{name}\" could be any of {}: name one in full",
                many.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

/// Whether a `[[folders]]` table is the folder of the master called `name`.
fn is_attached(table: &toml_edit::Table, name: &str) -> bool {
    is_named(table, name) && table.get("target").and_then(|t| t.as_str()) == Some("folder")
}

/// Stops syncing a folder on this computer: a folder of the master (named in full, or by its name alone when that is not
/// ambiguous) or one collected here. The files stay in the local folder and on the master. Returns the name in full.
pub fn remove_folder_from_file(config_path: &Path, name: &str) -> Result<String> {
    let cfg = Config::load(config_path)?;
    let attached = cfg.attached_folders().any(|s| names_synced(&s.name, name));
    let full = if attached { cfg.attached_named(name)?.name.clone() } else { name.to_string() };
    edit_file(config_path, |doc| {
        let folders = doc
            .get_mut("folders")
            .and_then(|f| f.as_array_of_tables_mut())
            .with_context(|| format!("no folder named \"{name}\""))?;
        let before = folders.len();
        folders.retain(|t| !(is_named(t, &full) && (t.get("target").is_some() == attached)));
        if folders.len() == before {
            bail!("no folder named \"{name}\"");
        }
        Ok(())
    })?;
    Ok(full)
}

#[cfg(test)]
#[path = "../../tests/unit/config/mod.rs"]
mod tests;
