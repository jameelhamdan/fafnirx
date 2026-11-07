use super::*;
use crate::testing::call;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const T: &str = "secret-token-0123456789";

/// A module that reads the kind `echo`: the file it makes holds what the listing named.
struct Echo {
    ready: AtomicBool,
}

impl Fetcher for Echo {
    type Config = String;

    fn info(&self) -> Info {
        Info { id: "echo", kind: "echo", version: "1.2.3" }
    }

    async fn readiness(&self) -> Readiness {
        if self.ready.load(Ordering::Relaxed) {
            Readiness::ready()
        } else {
            Readiness::missing("echo needs its program")
        }
    }

    fn check(&self, config: &Value) -> anyhow::Result<(Value, String)> {
        let text = config["text"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Text: give some"))?;
        Ok((json!({ "text": text.trim() }), format!("echo {}", text.trim())))
    }

    fn parse(&self, config: &Value) -> anyhow::Result<String> {
        self.check(config).map(|(c, _)| c["text"].as_str().unwrap_or_default().to_string())
    }

    fn label(&self, config: &String) -> String {
        config.clone()
    }

    async fn list(&self, config: &String) -> anyhow::Result<FetchListing> {
        Ok(FetchListing::new(vec![FetchEntry::new(format!("{config}.txt"), "1")]))
    }

    async fn get(&self, config: &String, entry: &FetchEntry, dest: &Path) -> anyhow::Result<u64> {
        anyhow::ensure!(config != "fail", "the echo failed\nthe real reason");
        std::fs::write(dest, &entry.path)?;
        Ok(entry.path.len() as u64)
    }

    fn status(&self) -> Map<String, Value> {
        Map::from_iter([("tool".to_string(), json!("v9"))])
    }
}

struct Fixture {
    data: tempfile::TempDir,
    app: Router,
}

fn fixture(ready: bool) -> Fixture {
    let data = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(data.path().join("tmp")).unwrap();
    let common = Common {
        data: data.path().to_path_buf(),
        token: T.into(),
        jobs: 2,
        idle: Duration::from_secs(20),
        ui: None,
        listen: "127.0.0.1:0".parse().unwrap(),
        announce: None,
    };
    Fixture { app: router(&common, Echo { ready: AtomicBool::new(ready) }), data }
}

fn dest(f: &Fixture, name: &str) -> String {
    f.data.path().join("tmp").join(name).to_string_lossy().into_owned()
}

#[tokio::test]
async fn only_the_master_with_the_token_gets_in_and_health_is_open() {
    let f = fixture(true);
    assert_eq!(call(&f.app, "GET", "/health", None, None).await.0, StatusCode::OK);
    for token in [None, Some("wrong-token-0123456789")] {
        let (status, body) = call(&f.app, "GET", "/manifest", token, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("echo module needs"), "{body}");
    }
    for (method, uri) in [
        ("GET", "/status"),
        ("GET", "/ui/index.js"),
        ("POST", FETCH_LIST_PATH),
        ("POST", FETCH_GET_PATH),
        ("POST", FETCH_CHECK_PATH),
    ] {
        assert_eq!(call(&f.app, method, uri, None, None).await.0, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn the_manifest_says_what_it_reads_and_what_is_missing() {
    let f = fixture(true);
    let (status, body) = call(&f.app, "GET", "/manifest", Some(T), None).await;
    assert_eq!(status, StatusCode::OK);
    let m: Manifest = serde_json::from_str(&body).unwrap();
    assert_eq!((m.id.as_str(), m.version.as_str(), m.contract), ("echo", "1.2.3", fafnirx_module_api::CONTRACT));
    assert!(m.has("fetch.echo") && m.has(capability::STATUS) && m.setup.is_empty() && m.ui.is_none());

    let f = fixture(false);
    let m: Manifest = serde_json::from_str(&call(&f.app, "GET", "/manifest", Some(T), None).await.1).unwrap();
    assert!(!m.has("fetch.echo") && m.has(capability::STATUS), "no capability while it cannot read");
    assert_eq!(m.setup, ["echo needs its program"]);
    let (status, body) = call(&f.app, "POST", FETCH_LIST_PATH, Some(T), Some(json!({ "kind": "echo", "config": { "text": "a" } }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn a_kind_it_does_not_read_is_not_found() {
    let f = fixture(true);
    for uri in [FETCH_CHECK_PATH, FETCH_LIST_PATH] {
        let (status, body) = call(&f.app, "POST", uri, Some(T), Some(json!({ "kind": "other", "config": {} }))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert!(body.contains("does not read other sources"), "{body}");
    }
}

#[tokio::test]
async fn settings_are_checked_and_refused_with_a_sentence() {
    let f = fixture(true);
    let (status, body) = call(
        &f.app,
        "POST",
        FETCH_CHECK_PATH,
        Some(T),
        Some(json!({ "kind": "echo", "config": { "text": " hi " } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({ "config": { "text": "hi" }, "label": "echo hi" })
    );
    let (status, body) = call(&f.app, "POST", FETCH_CHECK_PATH, Some(T), Some(json!({ "kind": "echo", "config": {} }))).await;
    assert_eq!((status, body.as_str()), (StatusCode::BAD_REQUEST, "Text: give some"));
}

#[tokio::test]
async fn a_source_is_listed_fetched_into_tmp_and_counted() {
    let f = fixture(true);
    let source = json!({ "kind": "echo", "config": { "text": "a" } });
    let (status, body) = call(&f.app, "POST", FETCH_LIST_PATH, Some(T), Some(source.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let listing: FetchListing = serde_json::from_str(&body).unwrap();
    let get = json!({ "kind": "echo", "config": { "text": "a" }, "entry": listing.items[0], "dest": dest(&f, "x1") });
    let (status, body) = call(&f.app, "POST", FETCH_GET_PATH, Some(T), Some(get)).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, r#"{"size":5}"#));
    assert_eq!(std::fs::read_to_string(f.data.path().join("tmp/x1")).unwrap(), "a.txt");
    let report: Value = serde_json::from_str(&call(&f.app, "GET", "/status", Some(T), None).await.1).unwrap();
    assert_eq!(
        (report["made"].as_u64(), report["slots"].as_u64(), report["tool"].as_str()),
        (Some(2), Some(2), Some("v9")),
        "the listing and the fetch are both counted"
    );
    assert!(report["active"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_failure_answers_with_its_reason_and_is_listed_as_failed() {
    let f = fixture(true);
    let entry = json!({ "path": "a.txt", "marker": "1", "size": 0, "mtime_ns": 0, "reference": null });
    let get = json!({ "kind": "echo", "config": { "text": "fail" }, "entry": entry, "dest": dest(&f, "x2") });
    let (status, body) = call(&f.app, "POST", FETCH_GET_PATH, Some(T), Some(get)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body.contains("the real reason"), "{body}");
    let report: Value = serde_json::from_str(&call(&f.app, "GET", "/status", Some(T), None).await.1).unwrap();
    assert_eq!(report["failed"][0]["error"], "the real reason");
}

#[test]
fn files_are_written_only_directly_in_the_masters_tmp_folder() {
    let data = tempfile::tempdir().unwrap();
    let tmp = data.path().join("tmp");
    std::fs::create_dir_all(tmp.join("inner")).unwrap();
    assert!(destination(data.path(), tmp.join("file").to_str().unwrap()).is_ok());
    for bad in [
        tmp.join("inner/file"),
        data.path().join("file"),
        tmp.join("../file"),
        tmp.join(".."),
        tmp.clone(),
        PathBuf::from("file"),
    ] {
        assert!(destination(data.path(), bad.to_str().unwrap()).is_err(), "{bad:?}");
    }
    assert!(destination(tempfile::tempdir().unwrap().path(), "/anywhere/file").is_err(), "no tmp folder at all");
}
