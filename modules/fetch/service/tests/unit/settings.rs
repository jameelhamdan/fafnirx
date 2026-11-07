use super::*;

#[test]
fn a_web_source_keeps_what_the_module_reads_and_labels_itself() {
    let (v, label) = check(&json!({"urls": [" https://x.test/a.zip ", "", "https://x.test/b.zip"], "include": r"\.zip$", "min_mb": 5, "junk": 1, "acknowledged": true})).unwrap();
    assert_eq!(v["urls"], json!(["https://x.test/a.zip", "https://x.test/b.zip"]));
    assert_eq!((v["include"].as_str(), v["min_mb"].as_u64()), (Some(r"\.zip$"), Some(5)));
    assert!(v.get("junk").is_none() && v.get("acknowledged").is_none() && v.get("feed").is_none());
    assert_eq!(label, "2 × file");
    let (feed, label) = check(&json!({"feed": " https://x.test/rss "})).unwrap();
    assert_eq!((label.as_str(), feed["feed"].as_str()), ("https://x.test/rss", Some("https://x.test/rss")));
    assert!(feed.get("urls").is_none());
}

#[test]
fn what_is_wrong_with_a_web_source_is_said() {
    let bad = |config: Value| check(&config).unwrap_err().to_string();
    assert!(bad(json!({})).contains("at least one"));
    assert!(bad(json!({"urls": ["ftp://x/a"]})).contains("not a web address"));
    assert!(bad(json!({"urls": ["https://x/a b"]})).contains("not a web address"));
    assert!(bad(json!({"urls": vec!["https://x.test/a"; 201]})).contains("at most 200"));
    assert!(bad(json!({"feed": "ftp://x"})).contains("Feed"));
    assert!(bad(json!({"feed": "https://x.test/rss", "include": "("})).contains("include"));
    assert!(bad(json!({"feed": "https://x.test/rss", "exclude": "x".repeat(301)})).contains("too long"));
    assert!(bad(json!({"feed": "https://x.test/rss", "min_mb": -1})).contains("Smallest file"));
    assert!(bad(json!({"feed": "https://x.test/rss", "max_gb": "big"})).contains("Largest file"));
}
