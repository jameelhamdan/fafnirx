//! How a mirror talks to its storage: the rclone remotes and their environment, the encryption layer of a backup, the restore kit and
//! the running of rclone itself.

use super::{MirrorKind, MirrorRow, REMOTE};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// Where a mirror's uploads go, as rclone writes it: the remote of the config file, or the one made from the options.
pub(super) fn base_of(m: &MirrorRow) -> String {
    backend_base(&m.config.remote, &m.config.remote_path)
}

/// The rclone path of a storage given as a remote of the config file, or as a backend with options.
pub fn backend_base(remote: &str, remote_path: &str) -> String {
    if remote.is_empty() {
        format!("{REMOTE}:{}", remote_path.trim_end_matches('/'))
    } else {
        remote.trim_end_matches('/').to_string()
    }
}

/// rclone configuration for the mirror, as environment variables. `config_file` is the rclone config file that a
/// mirror with a `remote` reads; the others get `empty_config`.
pub fn rclone_env(m: &MirrorRow, empty_config: &Path, config_file: &Path) -> Result<Vec<(String, String)>> {
    backend_env(&m.config.remote, &m.config.remote_type, &m.config.options, empty_config, config_file)
}

/// The same for any storage given as `remote`, or as `remote_type` with JSON `options` (a mirror or a source).
pub fn backend_env(remote: &str, remote_type: &str, options: &str, empty_config: &Path, config_file: &Path) -> Result<Vec<(String, String)>> {
    if !remote.is_empty() {
        if !config_file.is_file() {
            bail!(
                "remote {remote:?} needs the rclone config file {}, which does not exist (set FAFNIRX_RCLONE_CONFIG)",
                config_file.display()
            );
        }
        return Ok(vec![("RCLONE_CONFIG".to_string(), config_file.to_string_lossy().into_owned())]);
    }
    if remote_type.is_empty() || !remote_type.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("invalid remote type {remote_type:?}");
    }
    let opts: BTreeMap<String, String> = serde_json::from_str(options).context("backend options are not valid JSON")?;
    let mut env = vec![
        ("RCLONE_CONFIG".to_string(), empty_config.to_string_lossy().into_owned()),
        (format!("RCLONE_CONFIG_{REMOTE}_TYPE"), remote_type.to_string()),
    ];
    for (k, v) in opts {
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            bail!("invalid option name {k:?}");
        }
        if v.contains('\0') {
            bail!("invalid value for option {k}");
        }
        env.push((format!("RCLONE_CONFIG_{REMOTE}_{}", k.to_ascii_uppercase()), v));
    }
    Ok(env)
}

/// The name of the encryption layer of a backup, over the storage.
pub(super) const CRYPT: &str = "FXMIRRORCRYPT";

/// The rclone remote a mirror writes to: the storage itself for a replica, the encryption layer for a backup.
pub(super) fn top_of(m: &MirrorRow) -> String {
    if m.kind() == MirrorKind::Backup {
        format!("{CRYPT}:")
    } else {
        base_of(m)
    }
}

/// The environment that stacks encryption over the storage `base`. The two passwords are in the form rclone stores them
/// (`rclone obscure`). Names of files and folders are encrypted too.
pub fn backup_env(base: &str, password: &str, salt: &str) -> Vec<(String, String)> {
    let var = |layer: &str, key: &str, value: &str| (format!("RCLONE_CONFIG_{layer}_{key}"), value.to_string());
    vec![
        var(CRYPT, "TYPE", "crypt"),
        var(CRYPT, "REMOTE", base),
        var(CRYPT, "PASSWORD", password),
        var(CRYPT, "PASSWORD2", salt),
        var(CRYPT, "FILENAME_ENCRYPTION", "standard"),
        var(CRYPT, "DIRECTORY_NAME_ENCRYPTION", "true"),
    ]
}

/// An rclone config file that restores a backup mirror without fafnirx: `rclone copy fafnirx-backup:files <dir>`.
/// `storage` is the mirror's backend (its type and options), or empty when the mirror names a remote of the user's own
/// config file, which the restore then needs as well.
pub fn restore_config(m: &crate::repo::mirrors::Key, password: &str, salt: &str) -> String {
    let mut out = String::from("# Restore: rclone --config this-file copy fafnirx-backup:files ./restored\n");
    let target = if m.config.remote.is_empty() {
        let opts: BTreeMap<String, String> = serde_json::from_str(&m.config.options).unwrap_or_default();
        out.push_str(&format!("[fafnirx-storage]\ntype = {}\n", m.config.remote_type));
        for (k, v) in opts {
            out.push_str(&format!("{k} = {v}\n"));
        }
        format!("fafnirx-storage:{}", m.config.remote_path.trim_end_matches('/'))
    } else {
        out.push_str(&format!(
            "# The storage is {} in your own rclone.conf: keep that remote in this file.\n",
            m.config.remote
        ));
        m.config.remote.trim_end_matches('/').to_string()
    };
    out.push_str(&format!(
        "[fafnirx-backup]
type = crypt
remote = {target}
password = {password}
password2 = {salt}
filename_encryption = standard
directory_name_encryption = true
"
    ));
    out
}

/// What rclone stores of a password (`rclone obscure`); the plain one never reaches the command line.
pub async fn obscure(bin: &str, password: &str) -> Result<String> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new(bin)
        .args(["obscure", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("run {bin} (is rclone installed?)"))?;
    child.stdin.take().context("rclone obscure has no input")?.write_all(password.as_bytes()).await?;
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        bail!("rclone obscure failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The places on the remote that a run writes to.
pub(super) struct Remotes {
    pub(super) base: String,
    pub(super) root: String,
    pub(super) files: String,
    pub(super) versions: String,
    /// This run's directory under `versions/`: where replaced and removed files go.
    pub(super) run: String,
    pub(super) meta: String,
}

impl Remotes {
    pub(super) fn new(base: &str, stamp: &str) -> Self {
        Self {
            base: base.to_string(),
            root: join(base, ""),
            files: join(base, "files"),
            versions: join(base, "versions"),
            run: join(base, &format!("versions/{stamp}")),
            meta: join(base, "meta"),
        }
    }
}

/// `sub` inside `base`, where `base` is an rclone path such as `FXMIRROR:bucket/prefix` or `name:`.
pub fn join(base: &str, sub: &str) -> String {
    let base = base.trim_end_matches('/');
    match (base.ends_with(':'), sub.is_empty()) {
        (_, true) => base.to_string(),
        (true, false) => format!("{base}{sub}"),
        (false, false) => format!("{base}/{sub}"),
    }
}

/// Names of the version directories to delete so that only the newest `keep` remain.
pub fn expired_versions(mut dirs: Vec<String>, keep: usize) -> Vec<String> {
    dirs.retain(|d| !d.is_empty());
    dirs.sort();
    let excess = dirs.len().saturating_sub(keep.max(1));
    dirs.into_iter().take(excess).collect()
}

/// The rclone binary (`FAFNIRX_RCLONE`, else the one on the path).
pub fn rclone_bin() -> String {
    std::env::var("FAFNIRX_RCLONE").unwrap_or_else(|_| "rclone".into())
}

/// rclone with the environment that configures its storage.
pub struct Rclone {
    pub bin: String,
    pub env: Vec<(String, String)>,
}

impl Rclone {
    /// Runs rclone and returns stdout. Exit codes in `ok_codes` count as success.
    pub async fn run(&self, args: &[&str], ok_codes: &[i32]) -> Result<String> {
        let out = tokio::process::Command::new(&self.bin)
            .args(args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .with_context(|| format!("run {} (is rclone installed?)", self.bin))?;
        let code = out.status.code().unwrap_or(-1);
        if code != 0 && !ok_codes.contains(&code) {
            let err = String::from_utf8_lossy(&out.stderr);
            let tail: String = err.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
            bail!("rclone {} failed (exit {code}): {}", args.first().unwrap_or(&""), tail.trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}
