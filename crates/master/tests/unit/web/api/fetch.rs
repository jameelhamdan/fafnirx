//! Sources that a module reads: the master lists them through the module, fetches what is new into `tmp`, commits it,
//! and deletes only when it is sure.

use super::*;
use crate::integrations::fetch_source;
use crate::repo::links::{Dir, Due};
use std::sync::Mutex;

/// What the stand-in module offers, and how it behaves.
#[derive(Default)]
struct Plan {
    paths: Vec<&'static str>,
    complete: bool,
    /// A path whose file cannot be made.
    broken: Option<&'static str>,
    /// What it was asked to fetch, in order.
    fetched: Vec<String>,
}

fn stand_in_fetch_module(plan: Arc<Mutex<Plan>>) -> String {
    use axum::http::HeaderMap;
    let allowed = |h: &HeaderMap| h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer fetch-token-0123456789");
    let (for_list, for_get) = (plan.clone(), plan);
    let service = Router::new()
        .route(
            "/manifest",
            axum::routing::get(move |h: HeaderMap| async move {
                if !allowed(&h) {
                    return (StatusCode::UNAUTHORIZED, "no".to_string()).into_response();
                }
                let manifest = json!({"id": "fetch", "version": "1.0.0", "contract": fafnirx_module_api::CONTRACT, "core": fafnirx_module_api::VERSION, "capabilities": ["fetch.demo"]});
                axum::Json(manifest).into_response()
            }),
        )
        .route(
            "/fetch/check",
            axum::routing::post(move |h: HeaderMap, axum::Json(req): axum::Json<Value>| async move {
                let urls = req["config"]["urls"].as_array().cloned().unwrap_or_default();
                if !allowed(&h) || req["kind"] != "demo" || urls.iter().any(|u| !u.as_str().is_some_and(|u| u.starts_with("https://"))) {
                    return (StatusCode::BAD_REQUEST, "Addresses: https:// only".to_string()).into_response();
                }
                axum::Json(json!({"config": {"urls": urls}, "label": format!("{} × demo", urls.len())})).into_response()
            }),
        )
        .route(
            "/fetch/list",
            axum::routing::post(move |h: HeaderMap, axum::Json(req): axum::Json<Value>| async move {
                if !allowed(&h) || req["kind"] != "demo" || req["config"]["urls"][0] != "https://t.example/a" {
                    return (StatusCode::BAD_REQUEST, "no".to_string()).into_response();
                }
                let plan = for_list.lock().unwrap();
                let items: Vec<Value> = plan
                    .paths
                    .iter()
                    .map(|p| json!({"path": p, "marker": "m1", "size": 0, "mtime_ns": 1, "reference": {"url": p}}))
                    .collect();
                axum::Json(json!({"items": items, "skipped": 0, "complete": plan.complete})).into_response()
            }),
        )
        .route(
            "/fetch/get",
            axum::routing::post(move |h: HeaderMap, axum::Json(req): axum::Json<Value>| async move {
                if !allowed(&h) {
                    return (StatusCode::UNAUTHORIZED, "no".to_string()).into_response();
                }
                let path = req["entry"]["path"].as_str().unwrap().to_string();
                let mut plan = for_get.lock().unwrap();
                plan.fetched.push(path.clone());
                if plan.broken == Some(path.as_str()) {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "ERROR: Source unavailable".to_string()).into_response();
                }
                let bytes = format!("bytes of {path}");
                std::fs::write(req["dest"].as_str().unwrap(), &bytes).unwrap();
                axum::Json(json!({"size": bytes.len()})).into_response()
            }),
        );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move { axum::serve(tokio::net::TcpListener::from_std(listener).unwrap(), service).await.unwrap() })
    });
    url
}

async fn source_row(api: &Api, name: &str) -> Due {
    let name = name.to_string();
    crate::platform::db::with(&api.db, move |c| {
        let r = c.query_one(
            "SELECT s.id, s.kind, s.config, s.folder_id, d.folder FROM links s JOIN devices d ON d.id = s.folder_id WHERE s.direction = 'in' AND s.name = $1",
            &[&name],
        )?;
        Ok(Due {
            id: r.get(0),
            dir: Dir::In,
            kind: r.get(1),
            name: name.clone(),
            config: r.get(2),
            folder_id: r.get(3),
            folder: r.get(4),
            owner: Some("admin".into()),
        })
    })
    .await
    .unwrap()
}

async fn live(api: &Api, device_id: i64) -> Vec<String> {
    let mut paths: Vec<String> = crate::platform::db::with(&api.db, move |c| Ok(crate::repo::sources::live_paths(c, device_id)?))
        .await
        .unwrap();
    paths.sort();
    paths
}

async fn run_requested(api: &Api, id: i64) -> bool {
    crate::platform::db::with(&api.db, move |c| {
        Ok(c.query_one("SELECT run_requested FROM links WHERE id = $1", &[&id])?.get::<_, i64>(0) == 1)
    })
    .await
    .unwrap()
}

async fn clear_run_request(api: &Api, id: i64) {
    crate::platform::db::with(&api.db, move |c| {
        c.execute("UPDATE links SET run_requested = 0 WHERE id = $1", &[&id])?;
        Ok(())
    })
    .await
    .unwrap();
}

fn demo(extra: Value) -> Value {
    let mut config = json!({"urls": ["https://t.example/a"], "acknowledged": true});
    config.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    json!({"name": "tor", "kind": "demo", "config": config})
}

#[test]
fn a_source_a_module_reads_is_fetched_in_parts_and_deletes_only_what_it_is_sure_of() {
    let plan = Arc::new(Mutex::new(Plan {
        paths: vec!["L/a.mp3", "L/b.mp3", "L/c.mp3"],
        complete: true,
        ..Default::default()
    }));
    let registry = {
        let registry = crate::modules::Registry::with_secret(Some("fetch-token-0123456789".into()));
        registry
            .register(Some("Bearer fetch-token-0123456789"), "fetch", &stand_in_fetch_module(plan.clone()))
            .unwrap();
        Arc::new(registry)
    };
    let mut api = Api::with_modules(registry.clone());
    block(async {
        registry.refresh_all().await;
        crate::modules::run_with_for_tests(registry.clone());
        api.cookie = login(&api).await;

        // What is asked for is checked: the confirmation, the format, a link.
        let mut no_confirmation = demo(json!({}));
        no_confirmation["config"]["acknowledged"] = json!(false);
        assert_eq!(api.json(Method::POST, "/api/sources", no_confirmation).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            api.json(Method::POST, "/api/sources", demo(json!({"urls": ["ftp://x/a"]}))).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            api.json(Method::POST, "/api/sources", demo(json!({"max_per_run": 0}))).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(api.json(Method::POST, "/api/sources", demo(json!({"max_per_run": 2}))).await.0, StatusCode::OK);
        let listed = api.get("/api/sources").await.1;
        assert_eq!(listed[0]["detail"]["kind"], "demo");
        assert_eq!(listed[0]["detail"]["remote"], "1 × demo");
        assert!(!listed.to_string().contains("acknowledged"), "settings are not sent back to the browser");

        let row = source_row(&api, "tor").await;
        let (id, device) = (row.id, row.folder_id.unwrap());

        // Two of the three in the first run (the limit of the source), and a run asked for at once for the third.
        let out = fetch_source::run(api.db.clone(), api.store.clone(), row.clone()).await.unwrap();
        assert_eq!((out.added, out.deferred, out.failed.len()), (2, 1, 0), "{out:?}");
        assert_eq!(live(&api, device).await, ["L/a.mp3", "L/b.mp3"]);
        assert!(run_requested(&api, id).await, "the rest is fetched without waiting for the interval");
        let tmp_left = std::fs::read_dir(api._dir.path().join("tmp")).unwrap().count();
        assert_eq!(tmp_left, 0, "nothing is left in tmp");

        clear_run_request(&api, id).await;
        let out = fetch_source::run(api.db.clone(), api.store.clone(), row.clone()).await.unwrap();
        assert_eq!((out.added, out.unchanged, out.deferred), (1, 2, 0), "{out:?}");
        assert_eq!(live(&api, device).await, ["L/a.mp3", "L/b.mp3", "L/c.mp3"]);
        assert!(!run_requested(&api, id).await, "nothing is waiting");
        assert_eq!(plan.lock().unwrap().fetched, ["L/a.mp3", "L/b.mp3", "L/c.mp3"], "each file was fetched once");

        // A listing that may be cut short deletes nothing; a whole one deletes what is gone.
        {
            let mut p = plan.lock().unwrap();
            p.paths = vec!["L/b.mp3", "L/c.mp3"];
            p.complete = false;
        }
        let out = fetch_source::run(api.db.clone(), api.store.clone(), row.clone()).await.unwrap();
        assert_eq!((out.deleted, out.unchanged), (0, 2), "{out:?}");
        assert_eq!(live(&api, device).await.len(), 3, "a is still there");
        plan.lock().unwrap().complete = true;
        let out = fetch_source::run(api.db.clone(), api.store.clone(), row.clone()).await.unwrap();
        assert_eq!(out.deleted, 1, "{out:?}");
        assert_eq!(live(&api, device).await, ["L/b.mp3", "L/c.mp3"]);

        // A listing with nothing in it is the sign of a source that could not be read: nothing is deleted.
        {
            let mut p = plan.lock().unwrap();
            p.paths = vec![];
            p.complete = true;
        }
        let out = fetch_source::run(api.db.clone(), api.store.clone(), row.clone()).await.unwrap();
        assert_eq!(out.deleted, 0, "{out:?}");
        assert_eq!(live(&api, device).await.len(), 2);

        // A file that cannot be made is a problem of its own: the others still arrive.
        {
            let mut p = plan.lock().unwrap();
            p.paths = vec!["L/b.mp3", "L/c.mp3", "L/d.mp3", "L/e.mp3"];
            p.broken = Some("L/d.mp3");
        }
        let out = fetch_source::run(api.db.clone(), api.store.clone(), row.clone()).await.unwrap();
        assert_eq!((out.added, out.failed.len()), (1, 1), "{out:?}");
        assert!(
            out.failed[0].contains("L/d.mp3") && out.failed[0].contains("Source unavailable"),
            "{:?}",
            out.failed
        );
        assert_eq!(live(&api, device).await, ["L/b.mp3", "L/c.mp3", "L/e.mp3"]);
        assert_eq!(
            std::fs::read_dir(api._dir.path().join("tmp")).unwrap().count(),
            0,
            "the failed file left nothing in tmp"
        );
    });
}

#[test]
fn a_kind_no_module_reads_fails_with_a_message_that_says_what_to_do() {
    // No module of any test provides this kind, whichever module registry the process has.
    block(async {
        let e = crate::modules::fetch_list("nokind", &json!({})).await.unwrap_err();
        let text = format!("{e:#}");
        assert!(text.contains("no plugin") && text.contains("nokind") && text.contains("container"), "{text}");
    });
}
