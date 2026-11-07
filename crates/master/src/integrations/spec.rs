//! What a mirror or a source keeps its files in, and what a mirror is made of, with the checks on them (the API checks the name).

use crate::integrations::api::hours_to_secs;
use crate::repo::links::MirrorKind;
use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The rclone config file that mirrors with a `remote` read (`FAFNIRX_RCLONE_CONFIG`, default `rclone.conf` in the
/// data directory).
pub fn rclone_config() -> PathBuf {
    std::env::var_os("FAFNIRX_RCLONE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("FAFNIRX_DATA").unwrap_or_else(|_| "data".into())).join("rclone.conf"))
}

#[derive(Debug, Clone, PartialEq)]
pub struct MirrorSpec {
    pub name: String,
    /// `replica`: the files as they are. `backup`: encrypted with the two passwords below.
    pub kind: MirrorKind,
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
    /// The folder it holds, like `ana/laptop` or `master:Photos`; empty for everything.
    pub folder: String,
    /// Runs of history (replaced and removed files) kept on the mirror.
    pub keep_runs: i64,
}

/// A mirror's `remote`: the name of a remote of the rclone config, a colon, and a path inside it. The name starts
/// with a letter or digit, so that it can never be taken for an option.
fn valid_remote(remote: &str) -> bool {
    let Some((name, _)) = remote.split_once(':') else { return false };
    name.len() <= 64 && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+' | '@' | ' ')) && !remote.contains(['\0', '\n', '\r'])
}

/// The name of an rclone backend option: what rclone calls it in lowercase, with `_` for `-`.
fn option_name(key: &str) -> Option<String> {
    let key = key.trim().to_lowercase().replace('-', "_");
    let ok = !key.is_empty() && key.len() <= 64 && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    ok.then_some(key)
}

/// Checks and cleans where a mirror or a source keeps its files: a remote of the rclone config, or a backend with options.
pub fn normalize_storage(remote: &mut String, remote_type: &mut String, remote_path: &mut String, options: &mut BTreeMap<String, Value>) -> Result<()> {
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
    if remote_type.is_empty() || remote_type.len() > 32 || !remote_type.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
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
        match self.kind {
            MirrorKind::Replica => {
                self.crypt_password.clear();
                self.crypt_salt.clear();
            }
            MirrorKind::Backup => {
                for (what, v) in [("password", &self.crypt_password), ("salt", &self.crypt_salt)] {
                    if v.chars().count() < MIN_PASSWORD || v.contains(['\0', '\n', '\r']) {
                        bail!("the encryption {what} needs at least {MIN_PASSWORD} characters on one line");
                    }
                }
                if self.crypt_password == self.crypt_salt {
                    bail!("the encryption password and salt must differ");
                }
            }
        }
        normalize_storage(&mut self.remote, &mut self.remote_type, &mut self.remote_path, &mut self.options)?;
        self.folder = self.folder.trim().trim_matches('/').to_string();
        self.interval_hours = hours_to_secs(self.interval_hours).map_err(|e| anyhow!(e.1))? as f64 / 3600.0;
        if !(1..=100).contains(&self.keep_runs) {
            bail!("keep_runs must be between 1 and 100");
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/integrations/spec.rs"]
mod tests;
