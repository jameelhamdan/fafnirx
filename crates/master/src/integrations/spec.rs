//! What a mirror or a source keeps its files in, and what a mirror is made of, with the checks on them (the API checks the name).

use crate::integrations::api::hours_to_secs;
use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The rclone config file that mirrors with a `remote` read (`FAFNIRX_RCLONE_CONFIG`, default `rclone.conf` in the
/// data directory).
pub fn rclone_config() -> PathBuf {
    std::env::var_os("FAFNIRX_RCLONE_CONFIG").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var("FAFNIRX_DATA").unwrap_or_else(|_| "data".into())).join("rclone.conf")
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct MirrorSpec {
    pub name: String,
    /// `replica`: the files as they are. `backup`: compressed and encrypted with the two passwords below.
    pub kind: String,
    pub crypt_password: String,
    pub crypt_salt: String,
    /// A remote of the rclone config file, as `name:path`. When set, the three fields below are not used.
    pub remote: String,
    /// An rclone backend (`s3`, `sftp`, `webdav`, `b2`, ...) configured by `options`.
    pub remote_type: String,
    /// The bucket and prefix inside the backend.
    pub remote_path: String,
    /// rclone backend options (`access_key_id`, `region`, ...), as rclone's lowercase names.
    pub options: BTreeMap<String, Value>,
    pub interval_hours: f64,
    /// The folder it holds, like `ana/laptop/Documents` or `_master`; empty for everything (`scope`).
    pub scope: String,
    /// Runs of history (replaced and removed files) kept on the mirror.
    pub keep_runs: i64,
}

/// A mirror's `remote`: the name of a remote of the rclone config, a colon, and a path inside it. The name starts
/// with a letter or digit, so that it can never be taken for an option.
fn valid_remote(remote: &str) -> bool {
    let Some((name, _)) = remote.split_once(':') else { return false };
    name.len() <= 64
        && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+' | '@' | ' '))
        && !remote.contains(['\0', '\n', '\r'])
}

/// The name of an rclone backend option: what rclone calls it in lowercase, with `_` for `-`.
fn option_name(key: &str) -> Option<String> {
    let key = key.trim().to_lowercase().replace('-', "_");
    let ok = !key.is_empty()
        && key.len() <= 64
        && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    ok.then_some(key)
}

/// Checks and cleans where a mirror or a source keeps its files: a remote of the rclone config, or a backend with options.
pub fn normalize_storage(
    remote: &mut String,
    remote_type: &mut String,
    remote_path: &mut String,
    options: &mut BTreeMap<String, Value>,
) -> Result<()> {
    *remote = remote.trim().to_string();
    if !remote.is_empty() {
        if !valid_remote(remote) {
            bail!("remote {remote:?} must be a remote of the rclone config as name:path");
        }
        remote_type.clear();
        remote_path.clear();
        options.clear();
        return Ok(());
    }
    *remote_type = remote_type.trim().to_lowercase();
    if remote_type.is_empty()
        || remote_type.len() > 32
        || !remote_type.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        bail!("set remote (a remote of the rclone config), or remote_type (an rclone backend such as s3, sftp, webdav, b2)");
    }
    *remote_path = remote_path.trim().to_string();
    let mut clean = BTreeMap::new();
    for (key, value) in std::mem::take(options) {
        let name = option_name(&key).ok_or_else(|| anyhow!("option {key:?}: use lowercase letters, digits and _"))?;
        let text = match value {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => bail!("option {key:?} must be a string, a number or true/false"),
        };
        if text.contains('\0') {
            bail!("option {key:?} has an invalid value");
        }
        clean.insert(name, Value::String(text));
    }
    *options = clean;
    Ok(())
}

/// The shortest password of a backup: it protects everything the mirror holds.
const MIN_PASSWORD: usize = 12;

impl MirrorSpec {
    pub fn normalize(&mut self) -> Result<()> {
        self.kind = self.kind.trim().to_lowercase();
        match self.kind.as_str() {
            "replica" => {
                self.crypt_password.clear();
                self.crypt_salt.clear();
            }
            "backup" => {
                for (what, v) in [("password", &self.crypt_password), ("salt", &self.crypt_salt)] {
                    if v.chars().count() < MIN_PASSWORD || v.contains(['\0', '\n', '\r']) {
                        bail!("the encryption {what} needs at least {MIN_PASSWORD} characters on one line");
                    }
                }
                if self.crypt_password == self.crypt_salt {
                    bail!("the encryption password and salt must differ");
                }
            }
            _ => bail!("kind must be replica or backup"),
        }
        normalize_storage(&mut self.remote, &mut self.remote_type, &mut self.remote_path, &mut self.options)?;
        self.scope = crate::integrations::scope::normalize(&self.scope)?;
        self.interval_hours = hours_to_secs(self.interval_hours).map_err(|e| anyhow!(e.1))? as f64 / 3600.0;
        if !(1..=100).contains(&self.keep_runs) {
            bail!("keep_runs must be between 1 and 100");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> MirrorSpec {
        MirrorSpec {
            name: "m".into(),
            kind: "replica".into(),
            crypt_password: "ignored".into(),
            crypt_salt: String::new(),
            remote: String::new(),
            remote_type: "S3".into(),
            remote_path: " bucket/x ".into(),
            options: [("Access-Key".to_string(), Value::from(1))].into(),
            interval_hours: 72.0,
            scope: " /ana/laptop/ ".into(),
            keep_runs: 3,
        }
    }

    #[test]
    fn remotes_are_a_name_and_a_path() {
        assert!(valid_remote("b2crypt:") && valid_remote("my remote:bucket/x"));
        assert!(!valid_remote("no-colon") && !valid_remote("-x:y") && !valid_remote("a:b\nc"));
    }

    #[test]
    fn a_backend_spelled_out_is_normalized() {
        let mut m = spec();
        m.normalize().unwrap();
        assert_eq!((m.remote_type.as_str(), m.remote_path.as_str()), ("s3", "bucket/x"));
        assert_eq!(m.options["access_key"], Value::from("1"));
        assert_eq!(m.scope, "ana/laptop");
        let mut bad = spec();
        bad.keep_runs = 0;
        assert!(bad.normalize().is_err());
        let mut none = spec();
        none.remote_type.clear();
        assert!(none.normalize().is_err());
    }

    #[test]
    fn a_backup_needs_a_key_and_a_replica_has_none() {
        let mut replica = spec();
        replica.normalize().unwrap();
        assert!(replica.crypt_password.is_empty(), "a replica is not encrypted");
        let mut backup = spec();
        backup.kind = "Backup".into();
        assert!(backup.normalize().is_err(), "no key");
        backup.crypt_password = "short".into();
        backup.crypt_salt = "another long salt".into();
        assert!(backup.normalize().is_err(), "too short");
        backup.crypt_password = "a long enough password".into();
        backup.normalize().unwrap();
        backup.crypt_salt = backup.crypt_password.clone();
        assert!(backup.normalize().is_err(), "salt equals password");
        let mut other = spec();
        other.kind = "snapshot".into();
        assert!(other.normalize().is_err());
    }

    #[test]
    fn a_named_remote_replaces_the_backend() {
        let mut m = spec();
        m.remote = "vault:fafnirx".into();
        m.normalize().unwrap();
        assert!(m.remote_type.is_empty() && m.options.is_empty());
    }
}
