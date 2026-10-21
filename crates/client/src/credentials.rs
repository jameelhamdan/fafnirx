//! The device token lives in the OS keychain, with a file fallback next to
//! the config. A `token` in the TOML is accepted as a last resort.

use anyhow::{bail, Result};
use std::path::Path;

const SERVICE: &str = "fafnirx";

fn entry(server: &str) -> Result<keyring::Entry> {
    // Headless machines (and tests) can opt out of the keychain.
    if std::env::var("FAFNIRX_TOKEN_STORE").as_deref() == Ok("file") {
        bail!("keychain disabled by FAFNIRX_TOKEN_STORE=file");
    }
    Ok(keyring::Entry::new(SERVICE, server)?)
}

fn fallback_path(config_path: &Path) -> std::path::PathBuf {
    config_path.with_file_name("token")
}

/// Stores the token; returns where it ended up.
pub fn store(server: &str, config_path: &Path, token: &str) -> Result<&'static str> {
    let in_keychain = entry(server).and_then(|e| Ok(e.set_password(token)?)).is_ok()
        && load_keychain(server).as_deref() == Some(token);
    if in_keychain {
        let _ = std::fs::remove_file(fallback_path(config_path));
        return Ok("OS keychain");
    }
    let path = fallback_path(config_path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok("token file")
}

fn load_keychain(server: &str) -> Option<String> {
    entry(server).ok()?.get_password().ok().filter(|t| !t.is_empty())
}

pub fn load(server: &str, config_path: &Path, from_config: Option<&str>) -> Result<String> {
    if let Some(t) = load_keychain(server) {
        return Ok(t);
    }
    if let Ok(t) = std::fs::read_to_string(fallback_path(config_path)) {
        let t = t.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    match from_config {
        Some(t) if !t.is_empty() => Ok(t.to_string()),
        _ => bail!("no device token found; run `fafnirx enroll`"),
    }
}
