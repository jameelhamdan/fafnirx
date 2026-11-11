//! The modules the master asks for derived content, and who may use them.

use super::*;

/// A stand-in for a module: answers only with the right token, says what it was asked for, and has a report that only
/// the master may read. `contract` and `core` are what its manifest says.
fn stand_in_module(contract: u32, core: &'static str) -> String {
    use axum::extract::{Path, Query};
    use axum::http::HeaderMap;
    let allowed = |h: &HeaderMap| h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer module-token-0123456789");
    let service = Router::new()
        .route(
            "/manifest",
            axum::routing::get(move |h: HeaderMap| async move {
                if !allowed(&h) {
                    return (StatusCode::UNAUTHORIZED, "no".to_string()).into_response();
                }
                axum::Json(json!({"id": "video", "version": "1.0.0", "contract": contract, "core": core, "capabilities": ["status"]})).into_response()
            }),
        )
        .route(
            "/status",
            axum::routing::get(move |h: HeaderMap| async move {
                if !allowed(&h) {
                    return (StatusCode::UNAUTHORIZED, "no".to_string()).into_response();
                }
                axum::Json(json!({"active": [
                    {"kind": "segment", "path": "users/admin/videos/trip/clip.mkv"},
                    {"kind": "segment", "path": "users/bob/pc/secret.mkv"}
                ], "failed": []}))
                .into_response()
            }),
        )
        .route(
            "/files/{*rest}",
            axum::routing::get(
                move |Path(rest): Path<String>, Query(q): Query<std::collections::HashMap<String, String>>, h: HeaderMap| async move {
                    if !allowed(&h) {
                        return (StatusCode::UNAUTHORIZED, "no".to_string()).into_response();
                    }
                    if rest.ends_with("seg9.ts") {
                        return (StatusCode::NOT_FOUND, "no such segment".to_string()).into_response();
                    }
                    let body = format!("#EXTM3U {rest} p={} h={}", q["p"], q["h"]);
                    ([(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")], body).into_response()
                },
            ),
        );
    // On a runtime of its own, so that it can be started outside the test's.
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

fn registry_of(url: &str, token: &str) -> Arc<crate::modules::Registry> {
    let registry = crate::modules::Registry::with_secret(Some(token.into()));
    registry.register(Some(&format!("Bearer {token}")), "video", url).unwrap();
    Arc::new(registry)
}

#[test]
fn a_module_is_for_whoever_can_see_the_file_and_only_when_it_is_set_up() {
    // Not set up: nothing to offer, and nothing is passed on.
    let mut api = Api::new();
    block(async {
        assert_eq!(
            api.get("/api/m/video/AAAAAAAAAAAAAAAAAAAAAA/hls/master.m3u8").await.0,
            StatusCode::UNAUTHORIZED,
            "the login comes first"
        );
        api.cookie = login(&api).await;
        assert_eq!(api.get("/api/session").await.1["modules"], json!({}));
        assert_eq!(api.get("/api/m/video/AAAAAAAAAAAAAAAAAAAAAA/hls/master.m3u8").await.0, StatusCode::NOT_FOUND);
    });

    let registry = registry_of(
        &stand_in_module(fafnirx_module_api::CONTRACT, fafnirx_module_api::VERSION),
        "module-token-0123456789",
    );
    let mut api = Api::with_modules(registry.clone());
    block(async {
        api.cookie = login(&api).await;
        assert_eq!(api.get("/api/session").await.1["modules"], json!({}), "not up before it has answered");
        assert_eq!(api.get("/api/m/video/AAAAAAAAAAAAAAAAAAAAAA/hls/master.m3u8").await.0, StatusCode::NOT_FOUND);

        registry.refresh_all().await;
        let session = api.get("/api/session").await.1;
        assert_eq!(session["modules"]["video"]["capabilities"], json!(["status"]));
        api.json(Method::POST, "/api/folders", json!({"name": "videos"})).await;
        api.upload("admin/videos", "trip/clip.mkv", b"video bytes").await;
        let id: uuid::Uuid = crate::platform::db::with(&api.db, |c| Ok(c.query_one("SELECT id FROM files WHERE path = 'trip/clip.mkv'", &[])?.get(0)))
            .await
            .unwrap();
        let id = crate::platform::id::Id(id);

        let (status, headers, body) = api.send(Method::GET, &format!("/api/m/video/{id}/hls/720p/index.m3u8"), Body::empty(), &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "application/vnd.apple.mpegurl");
        let text = String::from_utf8(body).unwrap();
        assert!(
            text.contains("hls/720p/index.m3u8 p=users/admin/videos/trip/clip.mkv h="),
            "the module is told which file and content: {text}"
        );
        assert!(!text.contains("module-token-0123456789"));

        let (status, error) = api.get(&format!("/api/m/video/{id}/hls/720p/seg9.ts")).await;
        assert_eq!(
            (status, error["error"].as_str()),
            (StatusCode::NOT_FOUND, Some("no such segment")),
            "its answer is passed on"
        );
        for bad in ["../status", "hls/../status", ".hidden", "a/b/c/d/e"] {
            assert_eq!(api.get(&format!("/api/m/video/{id}/{bad}")).await.0, StatusCode::NOT_FOUND, "{bad}");
        }
        // Only what is under /files of the module is reachable: asking for its report or manifest asks for /files/status
        // and /files/manifest, so what only the master may read is never sent.
        for private in ["status", "manifest"] {
            let uri = format!("/api/m/video/{id}/{private}");
            let (status, _, body) = api.send(Method::GET, &uri, Body::empty(), &[]).await;
            let text = String::from_utf8(body).unwrap();
            assert!(
                status == StatusCode::OK && text.starts_with(&format!("#EXTM3U {private} p=")) && !text.contains("bob"),
                "{private}: {text}"
            );
        }
        assert_eq!(api.get(&format!("/api/m/nothing/{id}/hls/master.m3u8")).await.0, StatusCode::NOT_FOUND);
        assert_eq!(
            api.get("/api/m/video/AAAAAAAAAAAAAAAAAAAAAQ/hls/master.m3u8").await.0,
            StatusCode::NOT_FOUND,
            "no such file"
        );

        // The queue page lists what the module is doing like any other work, without the tasks of files the user cannot see.
        let tasks = |queue: &serde_json::Value| {
            queue["jobs"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|j| j["plugin"] == "video" && j["status"] != "failed")
                .count()
        };
        assert_eq!(tasks(&api.get("/api/queue").await.1), 2, "an administrator sees every task");
        let bob = add_user(&mut api, "bob", "normal").await;
        api.cookie = bob;
        assert_eq!(tasks(&api.get("/api/queue").await.1), 1, "bob sees only the task of his own file");
        api.cookie = login(&api).await;

        api.json(
            Method::POST,
            "/api/files/delete",
            json!({"items": [{"device": "admin/videos", "path": "trip/clip.mkv"}]}),
        )
        .await;
        assert_eq!(
            api.get(&format!("/api/m/video/{id}/hls/master.m3u8")).await.0,
            StatusCode::NOT_FOUND,
            "a deleted file is not streamed"
        );
    });
}

#[test]
fn a_module_that_refuses_the_token_or_speaks_another_contract_or_needs_a_newer_master_is_not_used() {
    let other_contract = registry_of(
        &stand_in_module(fafnirx_module_api::CONTRACT + 1, fafnirx_module_api::VERSION),
        "module-token-0123456789",
    );
    let too_new = registry_of(&stand_in_module(fafnirx_module_api::CONTRACT, "999.0.0"), "module-token-0123456789");
    let wrong_token = registry_of(
        &stand_in_module(fafnirx_module_api::CONTRACT, fafnirx_module_api::VERSION),
        "not-the-token-0123456789",
    );
    block(async {
        other_contract.refresh_all().await;
        assert!(other_contract.active().is_empty(), "another contract version");

        too_new.refresh_all().await;
        assert!(too_new.active().is_empty(), "it needs a newer master than this one");

        wrong_token.refresh_all().await;
        assert!(wrong_token.active().is_empty(), "the token is refused");

        let nobody_home = registry_of("http://127.0.0.1:1", "module-token-0123456789");
        nobody_home.refresh_all().await;
        assert!(nobody_home.active().is_empty(), "nothing answers");
    });
}
