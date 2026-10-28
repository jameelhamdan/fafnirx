//! What a mirror is made of, and the checks on it, for the API that adds one (the API checks the name).

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

impl MirrorSpec {
    pub fn normalize(&mut self) -> Result<()> {
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
    fn a_named_remote_replaces_the_backend() {
        let mut m = spec();
        m.remote = "vault:fafnirx".into();
        m.normalize().unwrap();
        assert!(m.remote_type.is_empty() && m.options.is_empty());
    }
}
