use super::*;
use axum::http::{header, HeaderMap, StatusCode};
use axum::routing::post;
use axum::Json;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// A stand-in for the master: keeps what it was told, and refuses a wrong secret.
async fn master(secret: &'static str) -> (String, Arc<Mutex<Vec<Value>>>) {
    let told = Arc::new(Mutex::new(Vec::new()));
    let seen = told.clone();
    let app = Router::new().route(
        "/api/plugins/register",
        post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let seen = seen.clone();
            async move {
                let given = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
                if given != Some(&format!("Bearer {secret}")) {
                    return (StatusCode::UNAUTHORIZED, "the plugin's secret is not this master's");
                }
                seen.lock().unwrap().push(body);
                (StatusCode::OK, "ok")
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, told)
}

#[tokio::test]
async fn the_master_is_told_the_id_and_the_address_with_the_secret_and_a_refusal_says_why() {
    let (base, told) = master("shared-secret-0123456789").await;
    let client = reqwest::Client::new();
    let to = Announce { master: format!("{base}/"), address: "http://video:8090".into() };
    assert_eq!(announce_once(&client, "video", "shared-secret-0123456789", &to).await, Ok(()));
    assert_eq!(*told.lock().unwrap(), [json!({ "id": "video", "address": "http://video:8090" })]);
    let refused = announce_once(&client, "video", "another-secret-0123456789", &to).await.unwrap_err();
    assert!(refused.contains("401") && refused.contains("not this master's"), "{refused}");
    let nobody = Announce { master: "http://127.0.0.1:1".into(), address: "http://video:8090".into() };
    assert!(announce_once(&client, "video", "x", &nobody).await.unwrap_err().contains("cannot be reached"));
}
