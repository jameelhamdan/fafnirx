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
