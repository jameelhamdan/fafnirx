use super::*;

#[test]
fn a_master_must_be_at_least_the_version_a_module_needs() {
    assert!(core_satisfied("3.20.0", "3.20.0") && core_satisfied("3.20.0", "3.21.1") && core_satisfied("3.20.0", "4.0.0"));
    assert!(!core_satisfied("3.20.0", "3.19.9") && !core_satisfied("3.20.1", "3.20.0"));
    assert!(core_satisfied("3.20.0", "3.20.0-rc1"), "a pre-release counts as its number");
    assert!(
        !core_satisfied("x", "3.20.0") && !core_satisfied("3.20.0", ""),
        "what is not a version is not satisfied"
    );
    let m = Manifest::new("video", "1", vec![]);
    assert!(core_satisfied(&m.core, VERSION) && m.contract == CONTRACT);
}

#[test]
fn the_whole_token_is_compared() {
    assert!(bearer_matches(Some("Bearer secret-token"), "secret-token"));
    for bad in [
        None,
        Some(""),
        Some("secret-token"),
        Some("bearer secret-token"),
        Some("Bearer secret-token-and-more"),
        Some("Bearer secret-toke"),
        Some("Bearer secret-tokeN"),
    ] {
        assert!(!bearer_matches(bad, "secret-token"), "{bad:?}");
    }
}

#[test]
fn a_manifest_says_what_it_provides() {
    let m: Manifest = serde_json::from_str(r#"{"id":"video","version":"1.0.0","contract":1,"core":"3.20.0","capabilities":["poster","status"]}"#).unwrap();
    assert!(m.has(capability::POSTER) && m.has(capability::STATUS) && !m.has(capability::PROBE_VIDEO));
    assert!(m.ui.is_none(), "a module without screens says nothing of them");
    let json = format!(r#"{{"id":"video","version":"1","contract":1,"core":"3.20.0","capabilities":[],"ui":{{"entry":"index.js","sdk":{WEB_SDK}}}}}"#);
    let with: Manifest = serde_json::from_str(&json).unwrap();
    assert_eq!(with.ui, Some(Ui { entry: "index.js".into(), css: None, sdk: WEB_SDK, strings: vec![] }));
}

#[test]
fn ids_are_short_plain_words() {
    for ok in ["video", "docs", "a1"] {
        assert!(valid_id(ok), "{ok}");
    }
    for bad in ["", "Video", "my-docs", "docs/x", "..", &"a".repeat(25)] {
        assert!(!valid_id(bad), "{bad:?}");
    }
}
