//! Self-update, from one of two places:
//!
//! * By default the project's GitHub releases (`release`): the newest version, checked against the checksum published with it.
//! * With `update_public_key` in the config, the master: it hosts builds you signed with `fafnirx-sign`, and the signing key
//!   never lives there, so an update is only installed if its ed25519 signature verifies. For private or modified builds.
//!
//! Only the program file is replaced. The config, the token in the keychain and the state directory (with the
//! installation id) stay, so the updated client is the same device and goes on where the old one stopped.

pub mod purge;
pub mod release;

use crate::config::Config;
use crate::net::Client;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use fafnirx_common::pb::{UpdateInfo, UpdateInfoRequest};
use fafnirx_common::update_message;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn parse(v: &str) -> Vec<u64> {
    v.trim().split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// True if `remote` is a strictly higher version than `current` (so a replayed
/// older release is never installed).
pub fn newer(remote: &str, current: &str) -> bool {
    let (a, b) = (parse(remote), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

pub fn verify(public_key_b64: &str, message: &str, signature: &[u8]) -> Result<()> {
    let key = base64::engine::general_purpose::STANDARD
        .decode(public_key_b64.trim())
        .context("update_public_key is not valid base64")?;
    let key: [u8; 32] = key.try_into().map_err(|_| anyhow!("update_public_key must be 32 bytes"))?;
    let key = VerifyingKey::from_bytes(&key).context("invalid update_public_key")?;
    let sig = Signature::from_slice(signature).map_err(|_| anyhow!("malformed signature"))?;
    key.verify(message.as_bytes(), &sig).map_err(|_| anyhow!("signature does not verify"))
}

/// The version an update would install, if one is available; nothing is downloaded. From the master the offer must carry a
/// valid signature to count, the same as for installing it.
pub async fn check(client: Option<&mut Client>, cfg: &Config) -> Result<Option<String>> {
    let platform = platform();
    if let Some(public_key) = cfg.update_public_key.as_deref() {
        let client = client.context("builds you signed are offered by the master: this needs its device token")?;
        return Ok(offer(client, public_key).await?.map(|info| info.version));
    }
    let found = tokio::task::spawn_blocking(move || {
        crate::update::release::latest(&crate::update::release::agent(), &platform, VERSION)
    })
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
    crate::service::elevate::run("dpkg", &["-i", &package.to_string_lossy()])
        .with_context(|| format!("the package is checked and kept at {}", package.display()))
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
        let found = tokio::task::spawn_blocking(move || {
            crate::update::release::latest_deb(&crate::update::release::agent(), arch, VERSION)
        })
        .await
        .context("the check stopped")??;
        if let Some(found) = found {
            tracing::info!("version {} is available; this install is updated from the tray's Update entry or with `fafnirx update`", found.version);
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
pub async fn check_and_apply(client: &mut Client, cfg: &Config, work_dir: &Path, interactive: bool) -> Result<bool> {
    if installed_by_dpkg() {
        if cfg.update_public_key.is_some() {
            bail!("a package install is updated with its package manager: the master only hosts builds for the plain executable");
        }
        return from_package(work_dir, interactive).await;
    }
    if let Some(public_key) = cfg.update_public_key.as_deref() {
        return from_master(client, public_key, work_dir).await;
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

/// The update the master offers, if it is newer than this client and its signature verifies.
async fn offer(client: &mut Client, public_key: &str) -> Result<Option<UpdateInfo>> {
    let platform = platform();
    let info = client.get_update_info(UpdateInfoRequest { platform: platform.clone() }).await?.into_inner();
    if !info.available || !newer(&info.version, VERSION) {
        return Ok(None);
    }
    let sha_hex = hex::encode(&info.sha256);
    verify(public_key, &update_message(&platform, &info.version, &sha_hex), &info.signature)
        .context("refusing update")?;
    Ok(Some(info))
}

/// Installs the update the master offers.
async fn from_master(client: &mut Client, public_key: &str, work_dir: &Path) -> Result<bool> {
    let Some(info) = offer(client, public_key).await? else { return Ok(false) };
    std::fs::create_dir_all(work_dir)?;
    let tmp = work_dir.join("update.new");
    let mut file = std::fs::File::create(&tmp)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut stream = client.download_update(UpdateInfoRequest { platform: platform() }).await?.into_inner();
    while let Some(chunk) = stream.message().await? {
        hasher.update(&chunk.data);
        size += chunk.data.len() as u64;
        if size > info.size {
            let _ = std::fs::remove_file(&tmp);
            bail!("the update is larger than announced");
        }
        file.write_all(&chunk.data)?;
    }
    file.sync_all()?;
    drop(file);
    if size != info.size || hasher.finalize().as_slice() != info.sha256.as_slice() {
        let _ = std::fs::remove_file(&tmp);
        bail!("downloaded update does not match the signed hash");
    }
    install(&tmp, &info.version)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn version_ordering() {
        assert!(newer("0.2.0", "0.1.9"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "0.2.0")); // replayed older release
        assert!(newer("0.1.1", "0.1"));
    }

    #[test]
    fn signature_binds_platform_version_and_bytes() {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let pk = base64::engine::general_purpose::STANDARD.encode(sk.verifying_key().to_bytes());
        let msg = update_message("windows-x86_64", "0.2.0", &"ab".repeat(32));
        let sig = sk.sign(msg.as_bytes()).to_bytes();
        verify(&pk, &msg, &sig).unwrap();
        for tampered in [
            update_message("linux-x86_64", "0.2.0", &"ab".repeat(32)),
            update_message("windows-x86_64", "9.9.9", &"ab".repeat(32)),
            update_message("windows-x86_64", "0.2.0", &"cd".repeat(32)),
        ] {
            assert!(verify(&pk, &tampered, &sig).is_err());
        }
        let other = SigningKey::from_bytes(&[8u8; 32]);
        let other_pk = base64::engine::general_purpose::STANDARD.encode(other.verifying_key().to_bytes());
        assert!(verify(&other_pk, &msg, &sig).is_err());
    }
}
