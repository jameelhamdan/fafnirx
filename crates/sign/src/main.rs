//! fafnirx-sign: generates the update signing key and publishes signed client
//! builds into the master's `updates/` directory. Run it on a machine
//! that is not the master: the secret key must never live there.

use anyhow::{bail, Context, Result};
use base64::Engine;
use clap::{Parser, Subcommand};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "fafnirx-sign")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a key pair. The secret goes to <out>, the public key is printed.
    Keygen {
        #[arg(long)]
        out: PathBuf,
    },
    /// Sign a client binary and write it under <out>/<platform>/
    Publish {
        #[arg(long)]
        key: PathBuf,
        /// e.g. windows-x86_64, linux-x86_64, macos-aarch64
        #[arg(long)]
        platform: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        bin: PathBuf,
        /// The master's updates directory (<data>/updates)
        #[arg(long)]
        out: PathBuf,
    },
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn load_key(path: &Path) -> Result<SigningKey> {
    let raw = b64().decode(std::fs::read_to_string(path)?.trim()).context("key file is not base64")?;
    let seed: [u8; 32] = raw.try_into().map_err(|_| anyhow::anyhow!("key must be 32 bytes"))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Keygen { out } => {
            if out.exists() {
                bail!("{} already exists", out.display());
            }
            let key = SigningKey::generate(&mut rand::rngs::OsRng);
            std::fs::write(&out, b64().encode(key.to_bytes()))?;
            println!("secret key written to {}", out.display());
            println!("put this in the client config as update_public_key:");
            println!("update_public_key = \"{}\"", b64().encode(key.verifying_key().to_bytes()));
        }
        Cmd::Publish { key, platform, version, bin, out } => {
            if !platform.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || platform.is_empty() {
                bail!("invalid platform");
            }
            let key = load_key(&key)?;
            let data = std::fs::read(&bin).with_context(|| format!("read {}", bin.display()))?;
            let sha = hex::encode(Sha256::digest(&data));
            let sig = key.sign(fafnirx_common::update_message(&platform, &version, &sha).as_bytes());
            let dir = out.join(&platform);
            std::fs::create_dir_all(&dir)?;
            // The binary first, the signature last: a client only sees a complete release.
            let write = |name: &str, content: &[u8]| -> Result<()> {
                let tmp = dir.join(format!("{name}.tmp"));
                std::fs::write(&tmp, content)?;
                std::fs::rename(&tmp, dir.join(name))?;
                Ok(())
            };
            write("bin", &data)?;
            write("version", version.as_bytes())?;
            write("signature", b64().encode(sig.to_bytes()).as_bytes())?;
            println!("published {platform} {version} ({sha}) to {}", dir.display());
        }
    }
    Ok(())
}
