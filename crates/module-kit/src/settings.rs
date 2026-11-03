//! The pieces the `check` of a module is made of: reading what a user typed, and saying what is wrong with it in a sentence.

use anyhow::{bail, Context, Result};
use regex::{Regex, RegexBuilder};
use serde_json::Value;

/// The longest web address a source may have.
pub const URL_LEN: usize = 2048;
/// The longest pattern a source may have.
const PATTERN_LEN: usize = 300;

/// The text under `key`, trimmed (empty when there is none).
pub fn text<'a>(config: &'a Value, key: &str) -> &'a str {
    config[key].as_str().unwrap_or_default().trim()
}

/// The texts of a list under `key`, trimmed, without the empty ones.
pub fn list(config: &Value, key: &str) -> Vec<String> {
    config[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Whether `s` is a web address a module reads: `http(s)://`, no spaces or control characters, not absurdly long.
pub fn is_web_address(s: &str) -> bool {
    (s.starts_with("http://") || s.starts_with("https://")) && s.len() <= URL_LEN && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The web address under `key`, or a sentence that says what `label` takes.
pub fn web_address(config: &Value, key: &str, label: &str) -> Result<String> {
    let url = text(config, key);
    if !is_web_address(url) {
        bail!("{label}: a web address starting with http:// or https://");
    }
    Ok(url.to_string())
}

/// The whole number under `key` if there is one (`null` is none), up to `max`.
pub fn whole_number(config: &Value, key: &str, label: &str, max: u64) -> Result<Option<u64>> {
    match config.get(key).filter(|v| !v.is_null()) {
        None => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) if n <= max => Ok(Some(n)),
            _ => bail!("{label}: a whole number up to {max}"),
        },
    }
}

/// The whole number under `key` (`min_mb`, `max_gb`) times `unit`, if there is one.
pub fn scaled(config: &Value, key: &str, unit: u64) -> Result<Option<u64>> {
    match config.get(key).filter(|v| !v.is_null()) {
        None => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| n.checked_mul(unit))
            .map(Some)
            .with_context(|| format!("{key}: a whole number")),
    }
}

/// The pattern under `key` if there is one: not long, and one a regular-expression engine can build within a size limit.
pub fn pattern(config: &Value, key: &str, label: &str, ignore_case: bool) -> Result<Option<Regex>> {
    match text(config, key) {
        "" => Ok(None),
        p if p.len() > PATTERN_LEN => bail!("{label}: the pattern is too long"),
        p => Ok(Some(
            RegexBuilder::new(p)
                .size_limit(1 << 20)
                .case_insensitive(ignore_case)
                .build()
                .with_context(|| format!("{label}: not a valid pattern"))?,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn texts_and_lists_are_trimmed() {
        let c = json!({"a": "  x ", "l": [" 1 ", "", 2, "3"]});
        assert_eq!((text(&c, "a"), text(&c, "nope")), ("x", ""));
        assert_eq!(list(&c, "l"), ["1", "3"]);
        assert!(list(&c, "a").is_empty());
    }

    #[test]
    fn addresses_numbers_and_patterns_are_checked() {
        assert!(is_web_address("https://x.test/a") && !is_web_address("ftp://x") && !is_web_address("https://x/a b"));
        assert!(!is_web_address(&format!("https://x/{}", "a".repeat(URL_LEN))));
        assert!(web_address(&json!({"u": "ftp://x"}), "u", "Link").unwrap_err().to_string().starts_with("Link"));
        let n = json!({"n": 5, "big": 50, "neg": -1, "nul": null});
        assert_eq!(whole_number(&n, "n", "N", 10).unwrap(), Some(5));
        assert_eq!(whole_number(&n, "nul", "N", 10).unwrap(), None);
        assert!(whole_number(&n, "big", "N", 10).is_err() && whole_number(&n, "neg", "N", 10).is_err());
        assert_eq!(scaled(&n, "n", 1 << 20).unwrap(), Some(5 << 20));
        assert!(scaled(&json!({"n": u64::MAX}), "n", 2).is_err());
        assert!(pattern(&json!({"p": "("}), "p", "include", false).unwrap_err().to_string().contains("include"));
        assert!(pattern(&json!({"p": "x".repeat(301)}), "p", "include", false)
            .unwrap_err()
            .to_string()
            .contains("too long"));
        assert!(pattern(&json!({"p": "ab"}), "p", "p", true).unwrap().unwrap().is_match("AB"));
        assert!(pattern(&json!({}), "p", "p", false).unwrap().is_none());
    }
}
