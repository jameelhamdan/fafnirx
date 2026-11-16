//! The device API is served on the web port too: a device and a browser use one address, behind one TLS proxy.

use super::*;
use crate::devices::grpc::{routes, Service};
use fafnirx_common::pb::{collector_client::CollectorClient, IdentifyRequest};

#[test]
fn devices_and_the_web_app_share_one_port() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        let (_, made) = api.json(Method::POST, "/api/devices", json!({"name": "laptop"})).await;
        let token = made["token"].as_str().unwrap().to_string();

        let (events, _) = tokio::sync::broadcast::channel(8);
        let service = Arc::new(Service { db: api.db.clone(), store: api.store.clone(), trusted: Default::default(), events });
        let app = api.app.clone().merge(routes(service)).into_make_service_with_connect_info::<SocketAddr>();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        // The web app answers on it, over HTTP/1.1.
        let web = reqwest::get(format!("http://{address}/healthz")).await.unwrap();
        assert_eq!(web.text().await.unwrap(), "ok");

        // A device speaks gRPC to the same address (HTTP/2 without TLS), with its token.
        let mut client = CollectorClient::connect(format!("http://{address}")).await.unwrap();
        let mut asking = tonic::Request::new(IdentifyRequest {});
        asking.metadata_mut().insert("authorization", format!("Bearer {token}").parse().unwrap());
        assert!(client.identify(asking).await.is_ok());

        // Without a token it is refused as on the dedicated port: the web app's login does not stand in front of it.
        let refused = client.identify(tonic::Request::new(IdentifyRequest {})).await.unwrap_err();
        assert_eq!(refused.code(), tonic::Code::Unauthenticated);
    });
}
