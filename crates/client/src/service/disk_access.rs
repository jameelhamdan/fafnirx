//! macOS lets a background program change files in Documents, Desktop, Downloads, iCloud Drive and on other volumes only
//! when it has Full Disk Access. The client is a service run by launchd, so without it a folder there fails with
//! "Operation not permitted" (a new file may be made, but an existing one cannot be replaced). Nothing asks the user: the
//! client finds out and says so, in the tray, in `status`, and before a folder there is attached.

use std::path::Path;

/// What to do, as one sentence for a message.
pub const STEPS: &str = "open System Settings > Privacy & Security > Full Disk Access, add fafnirx and switch it on, then run `sudo fafnirx restart`";

/// Whether this process has Full Disk Access: `Some` on macOS only. The service is the one whose answer counts, because it
/// is the one that changes the files; it keeps the answer in its status file.
pub fn granted() -> Option<bool> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    // Only a program with Full Disk Access may read the privacy database.
    match std::fs::File::open("/Library/Application Support/com.apple.TCC/TCC.db") {
        Ok(_) => Some(true),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Some(false),
        Err(_) => None,
    }
}

/// Whether the folder is one of those macOS protects: Documents, Desktop and Downloads of a user, iCloud Drive and the cloud
/// folders under Library, and any other volume (external disks and network shares). Written with `/` as macOS does.
pub fn is_protected(path: &Path) -> bool {
    let path = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let parts: Vec<&str> = path.split('/').collect();
    matches!(
        parts.as_slice(),
        ["", "volumes", _, ..] | ["", "users", _, "documents" | "desktop" | "downloads", ..] | ["", "users", _, "library", "mobile documents" | "cloudstorage", ..]
    )
}

/// The first of these folders that the service cannot reach for want of Full Disk Access, given what its status file says.
pub fn first_blocked<'a>(access: Option<bool>, paths: impl IntoIterator<Item = &'a Path>) -> Option<&'a Path> {
    if access != Some(false) {
        return None;
    }
    paths.into_iter().find(|p| is_protected(p))
}

/// The sentence for a folder that is blocked.
pub fn blocked_text(path: &Path) -> String {
    format!(
        "macOS blocks fafnirx from changing files in {} until it has Full Disk Access: {STEPS}.",
        path.display()
    )
}

/// A warning about `dir` when the service cannot reach it, read from its status file: for the commands and the tray, before a
/// folder is attached.
pub fn warning_for(config_path: &Path, dir: &Path) -> Option<String> {
    let cfg = crate::config::Config::load(config_path).ok()?;
    let status = crate::daemon::read_status(&cfg.state_dir(config_path))?;
    first_blocked(status.full_disk_access, [dir]).map(blocked_text)
}

/// Opens the page of System Settings where Full Disk Access is given.
#[cfg(feature = "tray")]
pub fn open_settings() {
    if cfg!(target_os = "macos") {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
            .status();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/service/disk_access.rs"]
mod tests;
