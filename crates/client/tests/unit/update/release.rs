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
