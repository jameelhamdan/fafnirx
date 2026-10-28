//! Updates from the project's GitHub releases: the default way a client updates itself. The release workflow attaches the
//! client for every platform as a plain executable (`fafnirx-<version>-<platform>`, `.exe` on Windows) with a `.sha256`
//! beside it, and the client takes the newest one it does not have yet.
//!
//! The download is checked against that checksum, so a damaged one is never installed. The checksum comes from the same
//! release, so this protects against a bad download, not against someone who can publish releases of the project; for that
//! sign your own builds and set `update_public_key` (see `update`).

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

/// The page of the project's newest release, where the installers and the plain executables are (the tray links to it).
pub fn releases_page() -> String {
    format!("{}/releases/latest", env!("CARGO_PKG_REPOSITORY").trim_end_matches('/'))
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

fn latest_with(
    agent: &ureq::Agent,
    url: &str,
    current: &str,
    pick: impl Fn(&Value) -> Option<Found>,
) -> Result<Option<Found>> {
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
mod tests {
    use super::*;
    use serde_json::json;

    fn release() -> Value {
        json!({
            "tag_name": "v3.1.0",
            "assets": [
                {"name": "fafnirx-3.1.0-linux-x86_64.tar.gz", "browser_download_url": "https://x/tar", "size": 9},
                {"name": "fafnirx-3.1.0-linux-x86_64", "browser_download_url": "https://x/bin", "size": 7},
                {"name": "fafnirx-3.1.0-linux-x86_64.sha256", "browser_download_url": "https://x/sum", "size": 90},
                {"name": "fafnirx-3.1.0-windows-x86_64.exe", "browser_download_url": "https://x/exe", "size": 5},
            ]
        })
    }

    #[test]
    fn the_executable_and_its_checksum_are_found_for_the_platform() {
        let f = pick(&release(), "linux-x86_64").unwrap();
        assert_eq!(
            (f.version.as_str(), f.binary_url.as_str(), f.checksum_url.as_str(), f.size),
            ("3.1.0", "https://x/bin", "https://x/sum", 7)
        );
        assert!(pick(&release(), "windows-x86_64").is_none(), "no checksum, so nothing is installed");
        assert!(pick(&release(), "macos-aarch64").is_none());
        assert!(pick(&json!({"message": "Not Found"}), "linux-x86_64").is_none());
    }

    #[test]
    fn the_debian_package_and_its_checksum_are_found_for_the_architecture() {
        let release = json!({
            "tag_name": "v3.1.0",
            "assets": [
                {"name": "fafnirx_3.1.0-1_amd64.deb", "browser_download_url": "https://x/deb", "size": 11},
                {"name": "fafnirx_3.1.0-1_amd64.deb.sha256", "browser_download_url": "https://x/deb.sum", "size": 90},
                {"name": "fafnirx_3.1.0-1_arm64.deb", "browser_download_url": "https://x/arm", "size": 12},
            ]
        });
        let found = pick_deb(&release, "amd64").unwrap();
        assert_eq!(
            (found.version.as_str(), found.binary_url.as_str(), found.checksum_url.as_str()),
            ("3.1.0", "https://x/deb", "https://x/deb.sum")
        );
        assert!(pick_deb(&release, "arm64").is_none(), "without its checksum nothing is installed");
        assert_eq!(deb_name("3.1.0", "amd64"), "fafnirx_3.1.0-1_amd64.deb");
    }

    #[test]
    fn names_follow_the_release_workflow() {
        assert_eq!(asset_name("3.1.0", "windows-x86_64"), "fafnirx-3.1.0-windows-x86_64.exe");
        assert_eq!(asset_name("3.1.0", "macos-aarch64"), "fafnirx-3.1.0-macos-aarch64");
    }

    #[test]
    fn a_checksum_file_is_read_like_sha256sum_writes_it() {
        let hash = "ab".repeat(32);
        assert_eq!(parse_checksum(&format!("{hash}  fafnirx-3.1.0-linux-x86_64\n")).unwrap(), vec![0xab; 32]);
        assert!(parse_checksum("not a hash").is_none() && parse_checksum("").is_none());
        assert!(parse_checksum(&"ab".repeat(16)).is_none(), "too short");
    }

    /// The newest release for `platform` as the server at `url` says it, if newer than `current`.
    fn latest_at(url: &str, platform: &str, current: &str) -> Result<Option<Found>> {
        latest_with(&agent(), url, current, |release| pick(release, platform))
    }

    /// Answers one request with `body` as JSON, and returns the address.
    fn serve_once(body: String) -> String {
        use std::io::{BufRead, BufReader};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/releases/latest", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 2 {
                line.clear();
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(body.as_bytes()).unwrap();
        });
        url
    }

    #[test]
    fn a_check_finds_a_newer_release_and_downloads_nothing() {
        let body = json!({
            "tag_name": "v9.9.9",
            "assets": [
                {"name": "fafnirx-9.9.9-test-x86_64", "browser_download_url": "http://127.0.0.1:1/bin", "size": 7},
                {"name": "fafnirx-9.9.9-test-x86_64.sha256", "browser_download_url": "http://127.0.0.1:1/sum", "size": 90},
            ]
        })
        .to_string();
        let found = latest_at(&serve_once(body.clone()), "test-x86_64", "3.2.0").unwrap().expect("it is newer");
        assert_eq!(found.version, "9.9.9");
        // The same release is not an update for a client that has it, or a newer one.
        assert!(latest_at(&serve_once(body.clone()), "test-x86_64", "9.9.9").unwrap().is_none());
        assert!(latest_at(&serve_once(body.clone()), "test-x86_64", "10.0.0").unwrap().is_none());
        // Nor is a release with no build for this platform.
        assert!(latest_at(&serve_once(body), "other-arm", "3.2.0").unwrap().is_none());
    }

    #[test]
    fn the_default_address_is_this_projects_releases() {
        assert!(api_url().starts_with("https://api.github.com/repos/") && api_url().ends_with("/releases/latest"));
    }
}
