//! The fetch module as the kit's [`Fetcher`]: it reads one kind of source, `web` (addresses of files, and feeds that link to files).

use crate::{settings, web};
use anyhow::Result;
use fafnirx_fetch_api::{FetchEntry, FetchListing};
use fafnirx_module_kit::{Fetcher, Info};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// The one kind of source this module reads.
pub const KIND: &str = "web";

pub struct Web {
    /// A download that receives nothing for this long is given up on.
    pub idle: Duration,
}

impl Fetcher for Web {
    type Config = web::Config;

    fn info(&self) -> Info {
        Info { id: "fetch", kind: KIND, version: env!("CARGO_PKG_VERSION") }
    }

    fn check(&self, config: &Value) -> Result<(Value, String)> {
        settings::check(config)
    }

    fn parse(&self, config: &Value) -> Result<web::Config> {
        web::Config::parse(config)
    }

    fn label(&self, config: &web::Config) -> String {
        config.feed.clone().unwrap_or_else(|| "addresses".into())
    }

    async fn list(&self, config: &web::Config) -> Result<FetchListing> {
        web::list(config).await
    }

    async fn get(&self, config: &web::Config, entry: &FetchEntry, dest: &Path) -> Result<u64> {
        web::get(self.idle, config, entry, dest).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use fafnirx_fetch_api::{Checked, Fetched};
    use fafnirx_module_api::{capability, Manifest};
    use fafnirx_module_kit::testing::{Fixture, IDLE};
    use serde_json::json;

    fn fixture() -> Fixture {
        Fixture::new(|_| Web { idle: IDLE })
    }

    /// A small website: a file that never changes, one that does, a feed, and a page that is not there.
    async fn website() -> String {
        use axum::http::HeaderMap;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let version = Arc::new(AtomicUsize::new(1));
        let base_cell = Arc::new(std::sync::OnceLock::<String>::new());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        base_cell.set(base.clone()).unwrap();
        let v = version.clone();
        let b = base_cell.clone();
        let app = Router::new()
            .route("/files/report.txt", get(|| async { ([(header::ETAG, "\"r1\"")], "the report") }))
            .route(
                "/files/live.txt",
                get(move || {
                    let v = v.clone();
                    async move {
                        let n = v.load(Ordering::SeqCst);
                        ([(header::ETAG, format!("\"v{n}\""))], format!("version {n}"))
                    }
                }),
            )
            .route(
                "/bump",
                get(move || {
                    let version = version.clone();
                    async move {
                        version.fetch_add(1, Ordering::SeqCst);
                        "ok"
                    }
                }),
            )
            .route(
                "/feed.xml",
                get(move |_h: HeaderMap| {
                    let base = b.get().cloned().unwrap_or_default();
                    async move { format!(r#"<rss><channel><item><title>R</title><enclosure url="{base}/files/report.txt"/></item><item><title>Gone</title><link>{base}/files/gone.txt</link></item></channel></rss>"#) }
                }),
            );
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        base
    }

    #[tokio::test]
    async fn the_manifest_offers_the_web_and_status() {
        let f = fixture();
        let (status, body) = f.call("GET", "/manifest", None).await;
        assert_eq!(status, StatusCode::OK);
        let m: Manifest = serde_json::from_str(&body).unwrap();
        assert_eq!(m.id, "fetch");
        assert!(m.has("fetch.web") && m.has(capability::STATUS), "{:?}", m.capabilities);
        assert!(m.setup.is_empty());
    }

    #[tokio::test]
    async fn settings_are_checked_and_labelled() {
        let f = fixture();
        let body = json!({"kind": "web", "config": {"urls": [" https://x.test/a.zip "], "junk": 1}});
        let (status, text) = f.call("POST", "/fetch/check", Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{text}");
        let checked: Checked = serde_json::from_str(&text).unwrap();
        assert_eq!(
            (checked.config["urls"][0].as_str(), checked.label.as_str()),
            (Some("https://x.test/a.zip"), "1 × file")
        );
        assert!(checked.config.get("junk").is_none());
        let (status, text) = f.call("POST", "/fetch/check", Some(json!({"kind": "web", "config": {}}))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(text.contains("at least one"), "{text}");
        let unusable = json!({"kind": "web", "config": {"urls": ["ftp://x"]}});
        assert_eq!(f.call("POST", "/fetch/list", Some(unusable)).await.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn addresses_and_a_feed_are_listed_and_each_file_is_fetched_alone() {
        let f = fixture();
        let base = website().await;
        let config = json!({"urls": [format!("{base}/files/live.txt")], "feed": format!("{base}/feed.xml")});
        let request = json!({"kind": "web", "config": config});
        let (status, text) = f.call("POST", "/fetch/list", Some(request.clone())).await;
        assert_eq!(status, StatusCode::OK, "{text}");
        let listing: FetchListing = serde_json::from_str(&text).unwrap();
        assert_eq!(listing.items.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(), ["live.txt", "report.txt"]);
        assert_eq!(listing.skipped, 1, "the feed links to a file that is not there");
        assert!(!listing.complete, "a feed shows its latest entries: what is missing from it is not gone");
        let live = &listing.items[0];
        assert!(live.marker.contains("\"v1\""), "{}", live.marker);

        let dest = f.dest("beef0001");
        let get = json!({"kind": "web", "config": config, "entry": live, "dest": dest});
        let (status, text) = f.call("POST", "/fetch/get", Some(get)).await;
        assert_eq!(status, StatusCode::OK, "{text}");
        assert_eq!(serde_json::from_str::<Fetched>(&text).unwrap().size, 9);
        assert_eq!(std::fs::read(&dest).unwrap(), b"version 1");
        assert_eq!(f.tmp_count(), 1, "only the file is left in tmp");

        // The file changes on the server: its marker changes, so the master fetches it again.
        reqwest::get(format!("{base}/bump")).await.unwrap();
        let (_, again) = f.call("POST", "/fetch/list", Some(request)).await;
        let again: FetchListing = serde_json::from_str(&again).unwrap();
        assert_ne!(again.items[0].marker, live.marker);
        assert_eq!(again.items[1].marker, listing.items[1].marker, "an unchanged file keeps its marker");
    }

    #[tokio::test]
    async fn a_file_that_cannot_be_fetched_fails_and_leaves_nothing_in_tmp() {
        let f = fixture();
        let base = website().await;
        let config = json!({"urls": [format!("{base}/files/report.txt")]});
        let entry = json!({"path": "gone.txt", "marker": "x", "reference": {"url": format!("{base}/files/gone.txt")}});
        let dest = f.dest("ffff0000");
        let get = json!({"kind": "web", "config": config, "entry": entry, "dest": dest});
        let (status, text) = f.call("POST", "/fetch/get", Some(get)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{text}");
        assert!(text.contains("404"), "{text}");
        assert_eq!(f.tmp_count(), 0, "no file and no work folder is left");
    }
}
