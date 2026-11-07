//! Updates from the project's GitHub releases: the default way a client updates itself. The release workflow attaches the
//! client for every platform as a plain executable (`fafnirx-<version>-<platform>`, `.exe` on Windows) with a `.sha256`
//! beside it, and the client takes the newest one it does not have yet.
//!
//! The download is checked against that checksum, so a damaged one is never installed. The checksum comes from the same
//! release, so this protects against a bad download, not against someone who can publish releases of the project.

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

/// The most that is downloaded, whatever the release says it weighs.
const MAX_BYTES: u64 = 200 * 1024 * 1024;

/// The release to install.
#[derive(Debug, PartialEq)]
pub struct Found {
    pub version: String,
    pub binary_url: String,
    pub checksum_url: String,
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
    Some(Found { version, binary_url, checksum_url, size })
}

/// The hash in a `.sha256` file (`<hex>  <name>`, as `sha256sum` writes it).
pub fn parse_checksum(text: &str) -> Option<Vec<u8>> {
    let hex_text = text.split_whitespace().next()?;
    hex::decode(hex_text).ok().filter(|h| h.len() == 32)
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
/// Downloads a release file to `dest` and checks it against the checksum published with it; a file that does not match is not kept.
fn fetch(agent: &ureq::Agent, found: &Found, dest: &Path) -> Result<()> {
    if found.size > MAX_BYTES {
        bail!("the update is larger than {} MB", MAX_BYTES / 1024 / 1024);
    }
    let checksum = agent.get(&found.checksum_url).call().context("download the checksum")?.into_string()?;
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
