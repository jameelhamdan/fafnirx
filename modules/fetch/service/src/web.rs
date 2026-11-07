//! The web: a list of addresses of files, and/or an RSS or Atom feed whose entries link to files, read over plain HTTP(S).
//!
//! A source offers files: each address (or each entry of the feed) is one file, named after the last part of its address.
//! The marker is made of what the server says of the file (its entity tag, modification time and length), so a file that
//! changes on the server is fetched again as a new version, and one that does not change is not.
//!
//! A listing from a feed is never complete (a feed shows its latest entries, not everything), and neither is one in which an
//! address could not be reached: the master then deletes nothing for what is missing from it.

use anyhow::{bail, ensure, Context, Result};
use fafnirx_fetch_api::{FetchEntry, FetchListing};
use fafnirx_module_kit::http;
use fafnirx_module_kit::settings::{is_web_address, list as trimmed_list, pattern, scaled, text};
use fafnirx_module_kit::xml::{attribute, unescape};
use fafnirx_module_kit::{sanitize_name, Scratch};
use futures_util::StreamExt;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

const MAX_ADDRESSES: usize = 200;
/// The largest feed.
const FEED_LIMIT: usize = 5 << 20;
/// Entries of a feed that are offered, newest first as the feed has them.
const MAX_FEED_ENTRIES: usize = 500;
/// A listing takes at most this long in all: the master waits for it.
const LIST_BUDGET: Duration = Duration::from_secs(10 * 60);
/// Addresses looked at at the same time.
const LOOKUPS: usize = 8;
/// The largest file that is fetched unless the source says less.
const DEFAULT_MAX_BYTES: u64 = 100 << 30;

#[derive(Debug, Clone)]
pub struct Config {
    /// Addresses of files.
    pub urls: Vec<String>,
    /// An RSS or Atom feed whose entries link to files.
    pub feed: Option<String>,
    /// Only files whose name matches, and not those that match `exclude`.
    pub include: Option<Regex>,
    pub exclude: Option<Regex>,
    pub min_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
}

impl Config {
    /// The settings as the master sent them (this module has checked them already, `settings::check`).
    pub fn parse(v: &Value) -> Result<Self> {
        let urls = trimmed_list(v, "urls");
        let feed = Some(text(v, "feed")).filter(|f| !f.is_empty()).map(String::from);
        ensure!(!urls.is_empty() || feed.is_some(), "the source has neither addresses nor a feed");
        ensure!(urls.len() <= MAX_ADDRESSES, "at most {MAX_ADDRESSES} addresses");
        for u in urls.iter().chain(feed.iter()) {
            ensure!(is_web_address(u), "{u:?} is not a web address");
        }
        Ok(Self {
            urls,
            feed,
            include: pattern(v, "include", "include", false)?,
            exclude: pattern(v, "exclude", "exclude", false)?,
            min_bytes: scaled(v, "min_mb", 1 << 20)?,
            max_bytes: scaled(v, "max_gb", 1 << 30)?,
        })
    }

    fn wants(&self, name: &str, length: Option<u64>) -> bool {
        self.include.as_ref().is_none_or(|r| r.is_match(name)) && !self.exclude.as_ref().is_some_and(|r| r.is_match(name)) && length.is_none_or(|l| self.min_bytes.is_none_or(|m| l >= m) && self.max_bytes.is_none_or(|m| l <= m))
    }
}

static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<(item|entry)\b.*?</(item|entry)>").unwrap());
static ENCLOSURE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<enclosure\b[^>]*>").unwrap());
static LINK_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<link\b[^>]*>").unwrap());
static LINK_TEXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<link\b[^>]*>(.*?)</link>").unwrap());

/// The address each entry of a feed (RSS `item`, Atom `entry`) points to: its enclosure when it has one (that is the file the
/// feed means), else its link.
pub fn feed_addresses(xml: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for block in ITEM.find_iter(xml).map(|m| m.as_str()) {
        let enclosure = ENCLOSURE
            .find_iter(block)
            .filter_map(|m| attribute(m.as_str(), "url"))
            .map(|u| unescape(&u))
            .find(|u| is_web_address(u));
        let link = || {
            LINK_TAG
                .find_iter(block)
                .filter_map(|m| attribute(m.as_str(), "href"))
                .map(|h| unescape(&h))
                .chain(LINK_TEXT.captures_iter(block).map(|c| unescape(&c[1])))
                .find(|l| is_web_address(l))
        };
        if let Some(address) = enclosure.or_else(link).filter(|a| !found.contains(a)) {
            found.push(address);
        }
        if found.len() >= MAX_FEED_ENTRIES {
            break;
        }
    }
    found
}

fn http() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| http::client(concat!("fafnirx-fetch/", env!("CARGO_PKG_VERSION"))));
    &CLIENT
}

/// What is at a web address, at most `FEED_LIMIT` bytes of it.
async fn read_feed(url: &str) -> Result<Vec<u8>> {
    http::read_limited(http(), url, FEED_LIMIT, Duration::from_secs(60)).await
}

/// A percent-encoded part of an address as text.
fn decoded(part: &str) -> String {
    let bytes = part.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = part.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The name of the file an address stands for: the last part of its path.
pub fn name_of(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let path = path.split_once("://").map_or(path, |(_, rest)| rest.split_once('/').map_or("", |(_, p)| p));
    let last = path.rsplit('/').find(|p| !p.is_empty()).unwrap_or("");
    let name = sanitize_name(&decoded(last));
    if name == "_" {
        "download".into()
    } else {
        name
    }
}

/// What the server says of the file at an address.
struct Head {
    length: Option<u64>,
    marker: String,
    mtime_ns: Option<i64>,
}

async fn head(url: &str) -> Result<Head> {
    let reply = http()
        .head(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .with_context(|| format!("reach {url}"))?;
    ensure!(reply.status().is_success(), "{url}: {}", reply.status());
    let header = |name: &str| reply.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    let length = reply.content_length().or_else(|| header("content-length").and_then(|l| l.parse().ok()));
    let modified = header("last-modified");
    let mtime_ns = modified
        .as_deref()
        .and_then(|m| time::OffsetDateTime::parse(m, &time::format_description::well_known::Rfc2822).ok())
        .map(|t| (t.unix_timestamp_nanos()) as i64);
    let marker = format!(
        "{}|{}|{}",
        header("etag").unwrap_or_default(),
        modified.unwrap_or_default(),
        length.map(|l| l.to_string()).unwrap_or_default()
    );
    Ok(Head { length, marker, mtime_ns })
}

pub async fn list(cfg: &Config) -> Result<FetchListing> {
    let mut addresses = cfg.urls.clone();
    if let Some(feed) = &cfg.feed {
        let xml = String::from_utf8_lossy(&read_feed(feed).await?).into_owned();
        addresses.extend(feed_addresses(&xml));
    }
    let mut seen = HashSet::new();
    addresses.retain(|a| seen.insert(a.clone()));
    let started = Instant::now();
    let now_ns = time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
    let found: Vec<(String, Result<Head>)> = futures_util::stream::iter(addresses)
        .map(|a| async move {
            let h = if started.elapsed() >= LIST_BUDGET { Err(anyhow::anyhow!("out of time")) } else { head(&a).await };
            (a, h)
        })
        .buffer_unordered(LOOKUPS)
        .collect()
        .await;
    let (mut items, mut skipped, mut names) = (Vec::new(), 0usize, HashSet::new());
    for (address, head) in found {
        match head {
            Ok(h) => {
                let name = name_of(&address);
                if !cfg.wants(&name, h.length) {
                    continue;
                }
                // Two addresses with the same name are two files: the second one is told apart by a short hash of its address.
                let path = if names.insert(name.clone()) {
                    name
                } else {
                    format!("{}-{}", hex::encode(sha2_prefix(&address)), name)
                };
                items.push(
                    FetchEntry::new(path, h.marker)
                        .size(h.length.unwrap_or(0))
                        .mtime_ns(h.mtime_ns.unwrap_or(now_ns))
                        .reference(json!({ "url": address })),
                );
            }
            Err(e) => {
                tracing::info!("{address}: {e:#}");
                skipped += 1;
            }
        }
    }
    items.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(FetchListing::new(items).skipped(skipped).complete(cfg.feed.is_none() && skipped == 0))
}

/// The first bytes of the SHA-256 of an address, to tell two files of the same name apart.
fn sha2_prefix(address: &str) -> [u8; 4] {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(address.as_bytes());
    [digest[0], digest[1], digest[2], digest[3]]
}

/// Downloads the file of `entry` and moves it to `dest`; answers with its size. A server that sends nothing for `idle` is given up on.
pub async fn get(idle: Duration, cfg: &Config, entry: &FetchEntry, dest: &Path) -> Result<u64> {
    let url = entry.reference["url"].as_str().context("the entry has no address")?;
    ensure!(is_web_address(url), "the entry's address is not a web address");
    let limit = cfg.max_bytes.unwrap_or(DEFAULT_MAX_BYTES);
    let mut name = dest.file_name().context("the destination has no name")?.to_os_string();
    name.push(".work");
    let work = Scratch::new_at(dest.with_file_name(name))?;
    let made = work.path().join("file");
    let reply = http()
        .get(url)
        .send()
        .await
        .with_context(|| format!("reach {url}"))?
        .error_for_status()
        .with_context(|| format!("fetch {url}"))?;
    let expected = reply.content_length();
    ensure!(expected.is_none_or(|n| n <= limit), "the file is larger than the {} MB allowed", limit >> 20);
    let mut out = tokio::fs::File::create(&made).await?;
    let (mut body, mut total) = (reply.bytes_stream(), 0u64);
    loop {
        match tokio::time::timeout(idle, body.next()).await {
            Err(_) => bail!("stopped: no data for {} seconds", idle.as_secs()),
            Ok(None) => break,
            Ok(Some(chunk)) => {
                let chunk = chunk.with_context(|| format!("read {url}"))?;
                total += chunk.len() as u64;
                ensure!(total <= limit, "the file is larger than the {} MB allowed", limit >> 20);
                out.write_all(&chunk).await?;
            }
        }
    }
    out.flush().await?;
    drop(out);
    ensure!(
        expected.is_none_or(|n| n == total),
        "the file has {total} bytes, the server said {}",
        expected.unwrap_or(0)
    );
    std::fs::rename(&made, dest).context("move the file into place")?;
    Ok(total)
}

#[cfg(test)]
#[path = "../tests/unit/web.rs"]
mod tests;
