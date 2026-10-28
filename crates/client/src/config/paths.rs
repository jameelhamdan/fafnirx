//! Where the config, the state and the files shared with the service are, for a user and for the whole computer.

use std::path::{Path, PathBuf};

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn home() -> PathBuf {
    env_dir("HOME").or_else(|| env_dir("USERPROFILE")).unwrap_or_else(|| PathBuf::from("."))
}

/// Where a system-wide install keeps its files. Its service runs as an administrative account for every user of the computer, so
/// nothing it needs is in a user's profile: `config.toml` and the token (the config folder), the databases (`state`, private to
/// the service) and the files that the tray and the commands of any user share with it (`control`).
pub fn system_data_dir() -> PathBuf {
    if cfg!(windows) {
        env_dir("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")).join("fafnirx")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/fafnirx")
    } else {
        PathBuf::from("/var/lib/fafnirx")
    }
}

/// The config folder of a system-wide install; on Linux it is `/etc/fafnirx`, as usual there.
pub fn system_config_path() -> PathBuf {
    if cfg!(windows) || cfg!(target_os = "macos") {
        system_data_dir().join("config.toml")
    } else {
        PathBuf::from("/etc/fafnirx/config.toml")
    }
}

pub fn system_state_dir() -> PathBuf {
    system_data_dir().join("state")
}

/// Whether this config is the system-wide one, which has no user to keep a token in a keychain for.
pub fn is_system_config(config_path: &Path) -> bool {
    super::same_path(config_path, &system_config_path())
}

/// Where the client and the programs that talk to it meet: the markers (pause, sync now, stop, update, restart), the status
/// file and the log. In a system-wide install it is the one folder every user may write to; the rest of the state is not.
pub fn control_dir(state_dir: &Path) -> PathBuf {
    if super::same_path(state_dir, &system_state_dir()) {
        system_data_dir().join("control")
    } else {
        state_dir.join("control")
    }
}

/// The config of this computer: the system-wide one on Windows, where it was installed on Linux and macOS (and for root, who
/// installs it), else the one of the current user.
pub fn default_config_path() -> PathBuf {
    if cfg!(windows) || system_config_path().exists() || (cfg!(unix) && crate::service::elevate::is_admin()) {
        return system_config_path();
    }
    let dir = if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else {
        env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
    };
    dir.join("fafnirx").join("config.toml")
}

pub fn default_state_dir(config_path: &Path) -> PathBuf {
    if is_system_config(config_path) {
        return system_state_dir();
    }
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
