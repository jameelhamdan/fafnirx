use super::*;

#[test]
fn only_plain_names_are_served() {
    for ok in ["index.js", "queue-3fa9.js", "style.css", "a_b-c.js.map"] {
        assert!(plain(ok), "{ok}");
    }
    for bad in ["", ".env", "..", "a/b", "a\\b", "a b", "é.js", &"a".repeat(129)] {
        assert!(!plain(bad), "{bad:?}");
    }
}

#[test]
fn the_manifest_names_what_was_built() {
    assert_eq!(describe(None), None);
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(describe(Some(dir.path())), None, "nothing built");
    std::fs::write(dir.path().join("index.js"), "export default {}").unwrap();
    assert_eq!(describe(Some(dir.path())).unwrap().css, None);
    std::fs::write(dir.path().join("style.css"), "").unwrap();
    let ui = describe(Some(dir.path())).unwrap();
    assert_eq!((ui.entry.as_str(), ui.css.as_deref(), ui.sdk), ("index.js", Some("style.css"), WEB_SDK));
    assert!(ui.strings.is_empty());
    for name in ["strings.es.json", "strings.zh.json", "strings..json", "strings.x y.json", "other.json"] {
        std::fs::write(dir.path().join(name), "{}").unwrap();
    }
    assert_eq!(describe(Some(dir.path())).unwrap().strings, ["es", "zh"], "the languages that have a file");
}

#[tokio::test]
async fn a_built_file_is_served_with_its_type_and_the_rest_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.js"), "export default {}").unwrap();
    let ok = file(Some(dir.path()), "index.js").await.unwrap();
    assert_eq!(ok.headers()[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
    for (given, name) in [(Some(dir.path()), "missing.js"), (Some(dir.path()), "../x"), (None, "index.js")] {
        assert_eq!(file(given, name).await.unwrap_err().0, StatusCode::NOT_FOUND, "{name}");
    }
}
