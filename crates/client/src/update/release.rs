//! Updates from the project's GitHub releases: the default way a client updates itself. The release workflow attaches the
//! client for every platform as a plain executable (`fafnirx-<version>-<platform>`, `.exe` on Windows) with a `.sha256`
//! (and, when the release is signed, a `.sha256.sig`) beside it, and the client takes the newest one it does not have yet.
//!
//! A download that does not match the checksum is never installed. Signing is optional: when the repository has the secret
//! `UPDATE_SIGNING_KEY`, the release workflow signs each checksum file with it (Ed25519) and [`UPDATE_KEY`] is its public
//! half. A release with a `.sig` is installed only if the signature is good; a release without one is checked by its
//! checksum alone, which guards against a bad download, not against someone who can publish releases (they could leave the
//! signature out).

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

/// Where the API of the project's releases is. `FAFNIRX_UPDATE_URL` replaces it (a fork, or a test).
fn api_url() -> String {
    if let Ok(url) = std::env::var("FAFNIRX_UPDATE_URL") {
        return url;
    }
    let repo = env!("CARGO_PKG_REPOSITORY").trim_end_matches('/');
    let path = repo.strip_prefix("https://github.com/").unwrap_or(repo);
    format!("https://api.github.com/repos/{path}/releases/latest")
}

/// The public key the checksums of releases are signed with (raw Ed25519, hex). A fork that publishes its own releases
/// puts its own here.
pub const UPDATE_KEY: &str = "5314ce74397f6f8b0f76992099837ed0c3b5bbff7658386a33a432850308079d";

/// The key updates are checked with. A debug build (the end-to-end tests) can be given another in `FAFNIRX_UPDATE_KEY`; a
/// release build knows only [`UPDATE_KEY`].
fn update_key() -> String {
    match std::env::var("FAFNIRX_UPDATE_KEY") {
        Ok(key) if cfg!(debug_assertions) => key,
        _ => UPDATE_KEY.to_string(),
    }
}

/// The most that is downloaded, whatever the release says it weighs.
const MAX_BYTES: u64 = 200 * 1024 * 1024;

/// The release to install.
#[derive(Debug, PartialEq)]
pub struct Found {
    pub version: String,
    pub binary_url: String,
    pub checksum_url: String,
    /// The checksum's signature, when the release is signed.
    pub signature_url: Option<String>,
    pub size: u64,
}

/// The name of the executable of a version for a platform (`windows-x86_64`, `linux-aarch64`, ...).
pub fn asset_name(version: &str, platform: &str) -> String {
    let exe = if platform.starts_with("windows") { ".exe" } else { "" };
    format!("fafnirx-{version}-{platform}{exe}")
}

/// The name of the Debian package of a version for an architecture (`amd64`, `arm64`).
pub fn deb_name(version: &str, arch: &str) -> String {
    format!("fafnirx_{version}-1_{arch}.deb")
}

/// The Debian architecture of this machine, if there is a package for it.
pub fn deb_arch() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("amd64"),
        "aarch64" => Some("arm64"),
        _ => None,
    }
}

/// The newest release and its files for this platform, out of the answer of the releases API. `None` when it has none for
/// this platform (a release that is still being published, or a platform without a build).
pub fn pick(release: &Value, platform: &str) -> Option<Found> {
    pick_named(release, |version| asset_name(version, platform))
}

/// The same for the Debian package of this architecture.
pub fn pick_deb(release: &Value, arch: &str) -> Option<Found> {
    pick_named(release, |version| deb_name(version, arch))
}

/// The release and the file `name_of` names for its version, with the checksum that goes with it.
fn pick_named(release: &Value, name_of: impl Fn(&str) -> String) -> Option<Found> {
    let version = release.get("tag_name")?.as_str()?.trim_start_matches('v').to_string();
    let wanted = name_of(&version);
    let assets = release.get("assets")?.as_array()?;
    let url_of = |name: &str| {
        assets.iter().find(|a| a.get("name").and_then(Value::as_str) == Some(name)).and_then(|a| {
            Some((
                a.get("browser_download_url")?.as_str()?.to_string(),
                a.get("size").and_then(Value::as_u64).unwrap_or(0),
            ))
        })
    };
    let (binary_url, size) = url_of(&wanted)?;
    let (checksum_url, _) = url_of(&format!("{wanted}.sha256"))?;
    let signature_url = url_of(&format!("{wanted}.sha256.sig")).map(|(url, _)| url);
    Some(Found { version, binary_url, checksum_url, signature_url, size })
}

/// The hash in a `.sha256` file (`<hex>  <name>`, as `sha256sum` writes it).
pub fn parse_checksum(text: &str) -> Option<Vec<u8>> {
    let hex_text = text.split_whitespace().next()?;
    hex::decode(hex_text).ok().filter(|h| h.len() == 32)
}

/// Whether `signature` (raw, 64 bytes) is the Ed25519 signature of `message` by the public key `key_hex`.
pub fn signed_by(key_hex: &str, message: &[u8], signature: &[u8]) -> bool {
    use ring::signature::{UnparsedPublicKey, ED25519};
    let Ok(key) = hex::decode(key_hex) else { return false };
    UnparsedPublicKey::new(&ED25519, key).verify(message, signature).is_ok()
}

/// Reads a small file of a release (a checksum, a signature), at most `limit` bytes.
fn small(agent: &ureq::Agent, url: &str, limit: u64, what: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    agent
        .get(url)
        .call()
        .with_context(|| format!("download the {what}"))?
        .into_reader()
        .take(limit)
        .read_to_end(&mut out)?;
    Ok(out)
}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .user_agent(&format!("fafnirx/{}", crate::update::VERSION))
        .build()
}

/// The newest release for this platform if it is newer than the running client; nothing is downloaded.
/// Blocking: call it from `spawn_blocking`.
pub fn latest(agent: &ureq::Agent, platform: &str, current: &str) -> Result<Option<Found>> {
    latest_with(agent, &api_url(), current, |release| pick(release, platform))
}

/// The same for the Debian package of this architecture.
pub fn latest_deb(agent: &ureq::Agent, arch: &str, current: &str) -> Result<Option<Found>> {
    latest_with(agent, &api_url(), current, |release| pick_deb(release, arch))
}

fn latest_with(agent: &ureq::Agent, url: &str, current: &str, pick: impl Fn(&Value) -> Option<Found>) -> Result<Option<Found>> {
    let release: Value = agent
        .get(url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .context("ask GitHub for the latest release")?
        .into_json()
        .context("read the release")?;
    let Some(found) = pick(&release) else {
        tracing::debug!("the latest release has no build of the kind asked for");
        return Ok(None);
    };
    Ok(crate::update::newer(&found.version, current).then_some(found))
}

/// The newest release for this platform if it is newer than the running client, downloaded to `dest` and checked.
/// Blocking: call it from `spawn_blocking`.
pub fn download(platform: &str, current: &str, dest: &Path) -> Result<Option<String>> {
    download_found(|agent| latest(agent, platform, current), dest)
}

/// The same for the Debian package of this architecture.
pub fn download_deb(arch: &str, current: &str, dest: &Path) -> Result<Option<String>> {
    download_found(|agent| latest_deb(agent, arch, current), dest)
}

fn download_found(find: impl FnOnce(&ureq::Agent) -> Result<Option<Found>>, dest: &Path) -> Result<Option<String>> {
    let agent = agent();
    let Some(found) = find(&agent)? else { return Ok(None) };
    fetch(&agent, &found, dest)?;
    Ok(Some(found.version))
}

/// Downloads a release file to `dest` and checks it against the checksum published with it, after checking the checksum's
/// signature when the release has one; a file that does not match is not kept.
fn fetch(agent: &ureq::Agent, found: &Found, dest: &Path) -> Result<()> {
    if found.size > MAX_BYTES {
        bail!("the update is larger than {} MB", MAX_BYTES / 1024 / 1024);
    }
    let checksum = small(agent, &found.checksum_url, 4096, "checksum")?;
    if let Some(url) = &found.signature_url {
        let signature = small(agent, url, 1024, "signature")?;
        if !signed_by(&update_key(), &checksum, &signature) {
            bail!("the checksum of the update is not signed with the project's key; nothing was installed");
        }
    }
    let checksum = String::from_utf8_lossy(&checksum);
    let expected = parse_checksum(&checksum).ok_or_else(|| anyhow!("the checksum file is not a SHA-256"))?;

    let response = agent.get(&found.binary_url).call().context("download the update")?;
    let mut reader = response.into_reader().take(MAX_BYTES + 1);
    let mut file = std::fs::File::create(dest)?;
    let (mut hasher, mut size, mut buf) = (Sha256::new(), 0u64, vec![0u8; 64 * 1024]);
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        size += n as u64;
    }
    file.sync_all()?;
    drop(file);
    if size > MAX_BYTES || hasher.finalize().as_slice() != expected.as_slice() {
        let _ = std::fs::remove_file(dest);
        bail!("the downloaded update does not match its checksum");
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/update/release.rs"]
mod tests;
