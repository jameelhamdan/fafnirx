//! TOML configuration and the per-OS locations of config, state and secrets.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub path: PathBuf,
    /// Top-level directory of this folder inside the device namespace.
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// `http://host:7443` on a trusted network, `https://...` through a TLS proxy such as a Cloudflare tunnel.
    pub server: String,
    /// Device token. Normally absent: `enroll` keeps it in the OS keychain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// The client looks for new releases now and then and installs them. Off: it stays on its version until `fafnirx update`
    /// is run by hand.
    #[serde(default = "yes")]
    pub auto_update: bool,
    /// Base64 ed25519 public key. With it the client updates from the master, to builds signed with the matching key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_public_key: Option<String>,
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
    #[serde(default, rename = "folders")]
    pub folders: Vec<Folder>,
}

fn yes() -> bool {
    true
}
fn default_scan() -> u64 {
    3600
}
fn default_hold() -> u64 {
    crate::guard::DEFAULT_HOLD_HOURS
}

impl Config {
    /// A config with the defaults and no folders yet.
    pub fn new(server: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            token: None,
            auto_update: true,
            update_public_key: None,
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
        let cfg: Config = toml::from_str(&text).with_context(|| format!("parse config {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_atomic(path, &toml::to_string_pretty(self)?)
    }

    pub fn validate(&self) -> Result<()> {
        if self.host().is_none() {
            bail!("server must be an http:// or https:// URL");
        }
        let mut names = std::collections::HashSet::new();
        for f in &self.folders {
            if f.name.is_empty() || f.name.contains(['/', '\\']) || f.name == "." || f.name == ".." {
                bail!("folder name {:?} must be a single path component", f.name);
            }
            if !names.insert(fold(&f.name)) {
                bail!(
                    "two folders are called {:?}: the master keeps them side by side under this name, and names that differ \
                     only in case or accents are the same there. Give one a different name, for example {:?}",
                    f.name,
                    format!("{}-2", f.name)
                );
            }
        }
        Ok(())
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

/// What the master tells folder names apart by: letters in any case and any Unicode form (é as one character or as e and
/// an accent) are the same name, because a file system or a share on the way may not tell them apart.
fn fold(name: &str) -> String {
    fafnirx_common::nfc(name).to_lowercase()
}

/// Hosts that are not reachable from the internet: names without a dot, `.local`/`.lan`/`.internal`
/// names, loopback, link-local and private IPv4 ranges.
fn is_private_host(host: &str) -> bool {
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        return ip.is_private() || ip.is_loopback() || ip.is_link_local();
    }
    !host.contains('.') || [".local", ".lan", ".internal", ".home.arpa"].iter().any(|s| host.ends_with(s))
}

/// Writes through a temporary file so a reader (the daemon) never sees half a config.
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))
}

fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Adds a folder to the config file, keeping everything else in the file (comments included) as it is.
/// The name defaults to the directory's own name, made unique. Returns the folder as stored.
pub fn add_folder_to_file(config_path: &Path, dir: &Path, name: Option<&str>) -> Result<Folder> {
    let cfg = Config::load(config_path).context("run `fafnirx enroll` first")?;
    let path = fafnirx_common::plain_path(
        &std::fs::canonicalize(dir).with_context(|| format!("cannot read {}", dir.display()))?,
    );
    if !path.is_dir() {
        bail!("{} is not a directory", path.display());
    }
    for f in &cfg.folders {
        if same_path(&f.path, &path) {
            bail!("{} is already collected as \"{}\"", path.display(), f.name);
        }
        if path.starts_with(&f.path) || f.path.starts_with(&path) {
            bail!(
                "{} overlaps {} (collected as \"{}\"); collect one of them",
                path.display(),
                f.path.display(),
                f.name
            );
        }
    }
    let name = match name {
        Some(n) => n.to_string(),
        None => {
            let base = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .filter(|n| !n.is_empty())
                .context("this folder has no name of its own; choose one with --name")?;
            let taken: std::collections::HashSet<String> = cfg.folders.iter().map(|f| fold(&f.name)).collect();
            let mut candidate = base.clone();
            let mut n = 2;
            while taken.contains(&fold(&candidate)) {
                candidate = format!("{base}-{n}");
                n += 1;
            }
            candidate
        }
    };
    let folder = Folder { path, name };
    let mut check = cfg;
    check.folders.push(folder.clone());
    check.validate()?;

    let text = std::fs::read_to_string(config_path)?;
    let mut doc: toml_edit::DocumentMut = text.parse().with_context(|| format!("parse {}", config_path.display()))?;
    // `enroll` writes an empty `folders = []`; that has to become a list of tables first.
    if doc.get("folders").and_then(|f| f.as_array()).is_some_and(|a| a.is_empty()) {
        doc.remove("folders");
    }
    let folders = doc
        .entry("folders")
        .or_insert(toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .context("`folders` in the config must be a list of [[folders]] tables")?;
    let mut table = toml_edit::Table::new();
    table["path"] = toml_edit::value(folder.path.to_string_lossy().into_owned());
    table["name"] = toml_edit::value(folder.name.clone());
    folders.push(table);
    write_atomic(config_path, &doc.to_string())?;
    Ok(folder)
}

/// Removes a folder from the config file. Data already uploaded stays on the master.
pub fn remove_folder_from_file(config_path: &Path, name: &str) -> Result<()> {
    Config::load(config_path)?;
    let text = std::fs::read_to_string(config_path)?;
    let mut doc: toml_edit::DocumentMut = text.parse()?;
    let folders = doc
        .get_mut("folders")
        .and_then(|f| f.as_array_of_tables_mut())
        .with_context(|| format!("no folder named \"{name}\""))?;
    let at: Vec<usize> = folders
        .iter()
        .enumerate()
        .filter(|(_, t)| t.get("name").and_then(|n| n.as_str()) == Some(name))
        .map(|(i, _)| i)
        .collect();
    if at.is_empty() {
        bail!("no folder named \"{name}\"");
    }
    for i in at.into_iter().rev() {
        folders.remove(i);
    }
    write_atomic(config_path, &doc.to_string())
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn home() -> PathBuf {
    env_dir("HOME").or_else(|| env_dir("USERPROFILE")).unwrap_or_else(|| PathBuf::from("."))
}

pub fn default_config_path() -> PathBuf {
    let dir = if cfg!(windows) {
        env_dir("APPDATA").unwrap_or_else(|| home().join("AppData/Roaming"))
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else {
        env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
    };
    dir.join("fafnirx").join("config.toml")
}

pub fn default_state_dir(config_path: &Path) -> PathBuf {
    let dir = if cfg!(windows) {
        env_dir("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData/Local"))
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else {
        env_dir("XDG_STATE_HOME").unwrap_or_else(|| home().join(".local/state"))
    };
    // Different config files get different state, so several instances never share a database.
    let key = hex::encode(&fafnirx_common::chunk_hash(config_path.to_string_lossy().as_bytes())[..4]);
    dir.join("fafnirx").join(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_validation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
server = "http://master:7443"
[[folders]]
path = "/data/docs"
name = "docs"
"#,
        )
        .unwrap();
        let cfg = Config::load(&path).unwrap();
        assert!(cfg.watch && cfg.scan_interval_secs == 3600 && cfg.bandwidth_limit_kbps == 0);
        assert!(cfg.token.is_none());
        cfg.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap().folders.len(), 1);
    }

    #[test]
    fn misplaced_or_misspelled_keys_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        // a top-level key written after [[folders]] belongs to the folder table
        std::fs::write(
            &path,
            "server = \"https://x\"
[[folders]]
path = \"a\"
name = \"a\"
watch = false
",
        )
        .unwrap();
        assert!(format!("{:#}", Config::load(&path).unwrap_err()).contains("unknown field"));
    }

    #[test]
    fn folders_are_added_and_removed_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.toml");
        std::fs::write(
            &cfg,
            "# my own notes\nserver = \"http://x:7443\"\nbandwidth_limit_kbps = 500 # keep it polite\n",
        )
        .unwrap();
        for d in ["a/photos", "b/photos", "c", "a/photos/raw"] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        let a = add_folder_to_file(&cfg, &dir.path().join("a/photos"), None).unwrap();
        let b = add_folder_to_file(&cfg, &dir.path().join("b/photos"), None).unwrap();
        let c = add_folder_to_file(&cfg, &dir.path().join("c"), Some("docs")).unwrap();
        assert_eq!(
            (a.name.as_str(), b.name.as_str(), c.name.as_str()),
            ("photos", "photos-2", "docs"),
            "same-named folders get distinct names"
        );

        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("# my own notes") && text.contains("# keep it polite"), "comments survive: {text}");
        let loaded = Config::load(&cfg).unwrap();
        assert_eq!(loaded.folders.len(), 3);
        assert_eq!(loaded.bandwidth_limit_kbps, 500);

        let err = |r: Result<Folder>| format!("{:#}", r.unwrap_err());
        assert!(err(add_folder_to_file(&cfg, &dir.path().join("a/photos"), None)).contains("already collected"));
        assert!(
            err(add_folder_to_file(&cfg, &dir.path().join("a/photos/raw"), None)).contains("overlaps"),
            "a folder inside a collected one"
        );
        assert!(
            err(add_folder_to_file(&cfg, dir.path(), Some("all"))).contains("overlaps"),
            "a folder containing collected ones"
        );
        assert!(err(add_folder_to_file(&cfg, &dir.path().join("missing"), None)).contains("cannot read"));

        // Names that differ only in case or in Unicode form are the same name to the master.
        let same = |name: &str| err(add_folder_to_file(&cfg, &dir.path().join("c"), Some(name)));
        assert!(same("docs").contains("already collected"), "the same folder again");
        std::fs::create_dir_all(dir.path().join("d")).unwrap();
        let clash = |name: &str| err(add_folder_to_file(&cfg, &dir.path().join("d"), Some(name)));
        assert!(clash("DOCS").contains("different name"), "case: {}", clash("DOCS"));
        assert!(clash("photos").contains("\"photos-2\""), "the error suggests a free name");
        add_folder_to_file(&cfg, &dir.path().join("d"), Some("caf\u{e9}")).unwrap();
        std::fs::create_dir_all(dir.path().join("e")).unwrap();
        assert!(
            err(add_folder_to_file(&cfg, &dir.path().join("e"), Some("cafe\u{301}"))).contains("different name"),
            "an accent as its own character"
        );
        remove_folder_from_file(&cfg, "caf\u{e9}").unwrap();

        remove_folder_from_file(&cfg, "photos-2").unwrap();
        let names: Vec<String> = Config::load(&cfg).unwrap().folders.into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["photos", "docs"]);
        assert!(remove_folder_from_file(&cfg, "nope").is_err());
        assert!(std::fs::read_to_string(&cfg).unwrap().contains("# my own notes"));
    }

    #[test]
    fn folders_can_be_added_to_a_config_written_by_enroll() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("config.toml");
        let cfg = Config::new("http://x:7443");
        cfg.save(&cfg_path).unwrap();
        std::fs::create_dir_all(dir.path().join("one")).unwrap();
        add_folder_to_file(&cfg_path, &dir.path().join("one"), None).unwrap();
        assert_eq!(Config::load(&cfg_path).unwrap().folders.len(), 1);
        remove_folder_from_file(&cfg_path, "one").unwrap();
        assert!(Config::load(&cfg_path).unwrap().folders.is_empty());
    }

    #[cfg(feature = "tray")]
    #[test]
    fn dashboard_url_defaults_to_port_8080_on_the_server_host() {
        let mut cfg = Config::new("http://nas.lan:7443");
        assert_eq!(cfg.dashboard_url(), "http://nas.lan:8080");
        cfg.server = "http://10.0.0.5".into();
        assert_eq!(cfg.dashboard_url(), "http://10.0.0.5:8080");
        cfg.web_url = Some("https://fafnirx.example.com".into());
        assert_eq!(cfg.dashboard_url(), "https://fafnirx.example.com");
        cfg.web_url = Some("javascript:alert(1)".into());
        assert_eq!(cfg.dashboard_url(), "http://10.0.0.5:8080", "only http(s) URLs are honoured");
    }

    #[test]
    fn rejects_other_schemes_and_bad_names() {
        let mut cfg = Config::new("ftp://x");
        assert!(cfg.validate().is_err());
        cfg.server = "https://x".into();
        cfg.folders.push(Folder { path: "a".into(), name: "a/b".into() });
        assert!(cfg.validate().is_err());
    }
}
