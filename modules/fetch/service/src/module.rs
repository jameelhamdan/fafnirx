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
#[path = "../tests/unit/module.rs"]
mod tests;
