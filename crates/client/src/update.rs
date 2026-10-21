//! Signed self-update. The orchestrator hosts the binaries but the signing key
//! never lives there: an update is only installed if its ed25519 signature
//! verifies against `update_public_key` from the config.

use crate::config::Config;
use crate::net::Client;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use fafnirx_common::pb::UpdateInfoRequest;
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

/// Downloads and installs an update if one is available. Returns true when
/// the executable was replaced (the caller should exit so it gets restarted).
pub async fn check_and_apply(client: &mut Client, cfg: &Config, work_dir: &Path) -> Result<bool> {
    let Some(public_key) = cfg.update_public_key.as_deref() else {
        tracing::debug!("no update_public_key configured, updates disabled");
        return Ok(false);
    };
    let platform = platform();
    let req = UpdateInfoRequest { platform: platform.clone() };
    let info = client.get_update_info(req.clone()).await?.into_inner();
    if !info.available || !newer(&info.version, VERSION) {
        return Ok(false);
    }
    let sha_hex = hex::encode(&info.sha256);
    verify(public_key, &update_message(&platform, &info.version, &sha_hex), &info.signature)
        .context("refusing update")?;

    std::fs::create_dir_all(work_dir)?;
    let tmp = work_dir.join("update.new");
    let mut file = std::fs::File::create(&tmp)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut stream = client.download_update(req).await?.into_inner();
    while let Some(chunk) = stream.message().await? {
        hasher.update(&chunk.data);
        size += chunk.data.len() as u64;
        file.write_all(&chunk.data)?;
    }
    file.sync_all()?;
    drop(file);
    if size != info.size || hasher.finalize().as_slice() != info.sha256.as_slice() {
        let _ = std::fs::remove_file(&tmp);
        bail!("downloaded update does not match the signed hash");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    self_replace::self_replace(&tmp).context("replace the running executable")?;
    let _ = std::fs::remove_file(&tmp);
    tracing::info!("updated {VERSION} -> {}", info.version);
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
