use crate::config::Config;
use anyhow::{Context, Result};
use fafnirx_common::pb::collector_client::CollectorClient;
use fafnirx_common::MAX_MESSAGE;
use std::time::Duration;
use tonic::metadata::MetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::{Request, Status};

#[derive(Clone)]
pub struct Auth(MetadataValue<tonic::metadata::Ascii>);

impl tonic::service::Interceptor for Auth {
    fn call(&mut self, mut req: Request<()>) -> Result<Request<()>, Status> {
        req.metadata_mut().insert("authorization", self.0.clone());
        Ok(req)
    }
}

pub type Client = CollectorClient<InterceptedService<Channel, Auth>>;

/// Builds a client. The connection is established lazily, so this works while
/// the server is unreachable.
pub fn client(cfg: &Config, token: &str) -> Result<Client> {
    let mut endpoint = Endpoint::from_shared(cfg.server.clone()).context("invalid server URL")?;
    if cfg.server.starts_with("https://") {
        // Behind a TLS proxy (a Cloudflare tunnel, a reverse proxy) the certificate is a public one.
        endpoint = endpoint.tls_config(ClientTlsConfig::new().with_enabled_roots())?;
    }
    let channel = endpoint
        .connect_timeout(Duration::from_secs(10))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .http2_keep_alive_interval(Duration::from_secs(30))
        .keep_alive_while_idle(true)
        .connect_lazy();
    let value = format!("Bearer {token}").parse().context("token contains invalid characters")?;
    Ok(CollectorClient::with_interceptor(channel, Auth(value))
        .max_decoding_message_size(MAX_MESSAGE)
        .max_encoding_message_size(MAX_MESSAGE))
}
