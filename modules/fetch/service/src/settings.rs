//! The settings of a web source as a user gives them (`POST /fetch/check`): checked, reduced to what this module reads (anything
//! else is dropped), and given a label for the Sources page. The confirmation that the user may copy what the source downloads and
//! the limit of files per run are the master's to check.

use anyhow::{bail, Result};
use fafnirx_module_kit::settings::{is_web_address, list, pattern, text, whole_number, URL_LEN};
use serde_json::{json, Map, Value};

const MAX_ADDRESSES: usize = 200;

/// The settings reduced to what the module reads, and the label of the source.
pub fn check(config: &Value) -> Result<(Value, String)> {
    let mut out = Map::new();
    let urls = list(config, "urls");
    let feed = text(config, "feed");
    if urls.is_empty() && feed.is_empty() {
        bail!("Give at least one address of a file, or a feed");
    }
    if urls.len() > MAX_ADDRESSES {
        bail!("Addresses: at most {MAX_ADDRESSES}");
    }
    if let Some(bad) = urls.iter().find(|u| !is_web_address(u)) {
        bail!(
            "Addresses: {:?} is not a web address starting with http:// or https:// (at most {URL_LEN} characters)",
            bad.chars().take(60).collect::<String>()
        );
    }
    if !feed.is_empty() && !is_web_address(feed) {
        bail!("Feed: a web address starting with http:// or https://");
    }
    let label = if feed.is_empty() { format!("{} × file", urls.len()) } else { feed.to_string() };
    if !urls.is_empty() {
        out.insert("urls".into(), json!(urls));
    }
    if !feed.is_empty() {
        out.insert("feed".into(), json!(feed));
    }
    for key in ["include", "exclude"] {
        if pattern(config, key, key, false)?.is_some() {
            out.insert(key.into(), json!(text(config, key)));
        }
    }
    for (key, name, max) in [("min_mb", "Smallest file (MB)", 10_000_000), ("max_gb", "Largest file (GB)", 100_000)] {
        if let Some(n) = whole_number(config, key, name, max)? {
            out.insert(key.into(), json!(n));
        }
    }
    Ok((Value::Object(out), label))
}

#[cfg(test)]
#[path = "../tests/unit/settings.rs"]
mod tests;
