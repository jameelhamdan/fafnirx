//! The device token lives in the OS keychain, with a file fallback next to
//! the config. A `token` in the TOML is accepted as a last resort.
//!
//! A system-wide install (its service has no user, and runs for all of them) keeps it in the file only: in the config folder,
//! readable by the service's account and the administrators.

use anyhow::Result;
use std::path::Path;

const SERVICE: &str = "fafnirx";

/// Whether the keychain of the current user is where this config's token lives. Headless machines (and tests) can opt out of it.
fn use_keychain(config_path: &Path) -> bool {
    !crate::config::is_system_config(config_path) && std::env::var("FAFNIRX_TOKEN_STORE").as_deref() != Ok("file")
}

fn entry(server: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, server)?)
}

fn fallback_path(config_path: &Path) -> std::path::PathBuf {
    config_path.with_file_name("token")
}

/// Stores the token; returns where it ended up.
pub fn store(server: &str, config_path: &Path, token: &str) -> Result<&'static str> {
    let in_keychain = use_keychain(config_path)
        && entry(server).and_then(|e| Ok(e.set_password(token)?)).is_ok()
        && load_keychain(server).as_deref() == Some(token);
    if in_keychain {
        let _ = std::fs::remove_file(fallback_path(config_path));
        return Ok("OS keychain");
    }
    let path = fallback_path(config_path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&path)?;
    // `mode` only applies to a file that is created: a token file made earlier may be more open.
    #[cfg(unix)]
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    // On Windows the folder decides who can read it, and the config folder is readable by every user: closed before the
    // token is written.
    crate::service::keep_private(&path)?;
    std::io::Write::write_all(&mut file, token.as_bytes())?;
    Ok("token file")
}

fn load_keychain(server: &str) -> Option<String> {
    entry(server).ok()?.get_password().ok().filter(|t| !t.is_empty())
}

pub fn load(server: &str, config_path: &Path, from_config: Option<&str>) -> Result<String> {
    if use_keychain(config_path) {
        if let Some(t) = load_keychain(server) {
            return Ok(t);
        }
    }
    if let Ok(t) = std::fs::read_to_string(fallback_path(config_path)) {
        let t = t.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    match from_config {
        Some(t) if !t.is_empty() => Ok(t.to_string()),
        _ => anyhow::bail!("no device token found; run `fafnirx enroll`"),
    }
}
