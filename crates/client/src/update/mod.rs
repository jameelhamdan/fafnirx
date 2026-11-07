//! Self-update from the project's GitHub releases (`release`): the newest version, checked against the checksum published
//! with it.
//!
//! Only the program file is replaced. The config, the token in the keychain and the state directory (with the
//! installation id) stay, so the updated client is the same device and goes on where the old one stopped.

pub mod purge;
pub mod release;

use anyhow::{Context, Result};
use std::path::Path;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// A version as people write it: `1.2.3`, or with fewer parts (`1.2` is `1.2.0`).
fn parse(v: &str) -> Option<semver::Version> {
    let mut parts: Vec<&str> = v.trim().split('.').collect();
    while parts.len() < 3 {
        parts.push("0");
    }
    semver::Version::parse(&parts.join(".")).ok()
}

/// True if `remote` is a strictly higher version than `current` (so a replayed
/// older release is never installed). A version that cannot be read is never newer.
pub fn newer(remote: &str, current: &str) -> bool {
    matches!((parse(remote), parse(current)), (Some(a), Some(b)) if a > b)
}

/// The version an update would install, if one is available; nothing is downloaded.
pub async fn check() -> Result<Option<String>> {
    let platform = platform();
    let found = tokio::task::spawn_blocking(move || crate::update::release::latest(&crate::update::release::agent(), &platform, VERSION))
        .await
        .context("the check stopped")??;
    Ok(found.map(|f| f.version))
}

/// Whether the program was installed by dpkg (a `.deb`): it is under `/usr`, which the client cannot write, and the package
/// manager owns it. Only Linux has such an install.
pub fn installed_by_dpkg() -> bool {
    #[cfg(target_os = "linux")]
    {
        let Ok(exe) = std::env::current_exe() else { return false };
        exe.starts_with("/usr/")
            && std::process::Command::new("dpkg")
                .arg("-S")
                .arg(&exe)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
    }
    #[cfg(not(target_os = "linux"))]
    false
}

/// Installs a `.deb` that was downloaded and checked, asking for permission the way the situation allows.
fn install_deb(package: &Path) -> Result<()> {
    crate::service::elevate::run("dpkg", &["-i", &package.to_string_lossy()]).with_context(|| format!("the package is checked and kept at {}", package.display()))
}
#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    Ok(std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?)
}

/// Updates a package install: downloads the `.deb` of the newest release, checks it, and has it installed. The background check
/// does not do this (`interactive` is false): it cannot ask for permission, so it only says that there is a newer version.
async fn from_package(work_dir: &Path, interactive: bool) -> Result<bool> {
    let arch = crate::update::release::deb_arch().context("there is no package for this kind of computer")?;
    if !interactive {
        let found = tokio::task::spawn_blocking(move || crate::update::release::latest_deb(&crate::update::release::agent(), arch, VERSION))
            .await
            .context("the check stopped")??;
        if let Some(found) = found {
            tracing::info!(
                "version {} is available; this install is updated from the tray's Update entry or with `fafnirx update`",
                found.version
            );
        }
        return Ok(false);
    }
    // Installed as root once checked, so nobody else may replace it in between: a private folder, the file read-only.
    let dir = work_dir.join("update");
    std::fs::create_dir_all(&dir)?;
    let package = dir.join("update.deb");
    let work = package.clone();
    #[cfg(unix)]
    set_mode(&dir, 0o700)?;
    let _ = std::fs::remove_file(&package); // one kept from a failed install is read-only
    let version = tokio::task::spawn_blocking(move || crate::update::release::download_deb(arch, VERSION, &work))
        .await
        .context("the download stopped")??;
    let Some(version) = version else { return Ok(false) };
    #[cfg(unix)]
    set_mode(&package, 0o400)?;
    let installed = tokio::task::spawn_blocking({
        let package = package.clone();
        move || install_deb(&package)
    })
    .await
    .context("the install stopped")?;
    if installed.is_ok() {
        let _ = std::fs::remove_file(&package);
        tracing::info!("updated {VERSION} -> {version}");
    }
    installed.map(|()| true)
}

/// Downloads and installs an update if one is available. Returns true when the executable was replaced (the caller
/// should exit so it gets restarted). `interactive` is a person asking (the command, the tray), who can be asked for
/// permission; the background check is not.
pub async fn check_and_apply(work_dir: &Path, interactive: bool) -> Result<bool> {
    if installed_by_dpkg() {
        return from_package(work_dir, interactive).await;
    }
    std::fs::create_dir_all(work_dir)?;
    let tmp = work_dir.join("update.new");
    let (platform, work) = (platform(), tmp.clone());
    let version = tokio::task::spawn_blocking(move || crate::update::release::download(&platform, VERSION, &work))
        .await
        .context("the download stopped")??;
    let Some(version) = version else { return Ok(false) };
    install(&tmp, &version)?;
    Ok(true)
}

/// Puts the downloaded program in the place of the running one.
fn install(tmp: &Path, version: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    let replaced = self_replace::self_replace(tmp).context("replace the running executable");
    let _ = std::fs::remove_file(tmp);
    replaced?;
    tracing::info!("updated {VERSION} -> {version}");
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/update/mod.rs"]
mod tests;
