//! A source that pulls files from any storage rclone supports (OneDrive, Dropbox, WebDAV, SFTP, S3, ...).
//!
//! Each run lists the remote recursively and hands the listing to `syncsource`. rclone has no stable file ids, so a
//! file is known by its path: a rename downloads the file again. A file counts as changed when its size or its
//! modification time differs from the last run.

use crate::integrations::syncsource::{self, Item, Listing, Rules};
use crate::integrations::{mirror, SourceRun};
use crate::platform::db::Db;
use crate::platform::store::{RemoveOnDrop, Store};
use crate::repo::links::Due;
use anyhow::{ensure, Context, Result};
use fafnirx_common::clean_remote_path;
use serde::Deserialize;
use std::sync::Arc;

/// What a source needs to reach its storage: the same as a mirror's destination.
#[derive(Deserialize)]
pub struct Config {
    /// A remote of the rclone config file as `name:path`; when set, the rest is not used.
    #[serde(default)]
    pub remote: String,
    #[serde(default)]
    pub remote_type: String,
    /// JSON object of rclone options.
    #[serde(default)]
    pub options: String,
    #[serde(default)]
    pub remote_path: String,
}

#[derive(Deserialize)]
struct Listed {
    #[serde(rename = "Path")]
    path: String,
    #[serde(rename = "Size")]
    size: i64,
    #[serde(default, rename = "ModTime")]
    mod_time: String,
    #[serde(default, rename = "IsDir")]
    is_dir: bool,
}

fn parse_time_ns(s: &str) -> i64 {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map(|t| t.unix_timestamp_nanos() as i64)
        .unwrap_or(0)
}

/// The files of a `lsjson` listing as items, and how many entries were skipped (unusable names or unknown sizes).
fn items_of(listing: &str) -> Result<(Vec<Item>, usize)> {
    let listed: Vec<Listed> = serde_json::from_str(listing).context("rclone listing is not valid JSON")?;
    let (mut items, mut skipped) = (Vec::new(), 0);
    for l in listed.into_iter().filter(|l| !l.is_dir) {
        let (Ok(path), true) = (clean_remote_path(&l.path), l.size >= 0) else {
            skipped += 1;
            continue;
        };
        let mtime_ns = parse_time_ns(&l.mod_time);
        items.push(Item { marker: format!("{}:{mtime_ns}", l.size), path, key: None, size: l.size as u64, mtime_ns });
    }
    Ok((items, skipped))
}

pub async fn run(db: Db, store: Arc<Store>, src: Due) -> Result<SourceRun> {
    let cfg: Config = serde_json::from_str(&src.config).context("source configuration is invalid")?;
    let empty_config = store.temp_path();
    std::fs::write(&empty_config, b"")?;
    let _cleanup = RemoveOnDrop(empty_config.clone());
    let rc = mirror::Rclone {
        bin: mirror::rclone_bin(),
        env: mirror::backend_env(
            &cfg.remote,
            &cfg.remote_type,
            &cfg.options,
            &empty_config,
            &crate::integrations::spec::rclone_config(),
        )?,
    };
    let base = mirror::backend_base(&cfg.remote, &cfg.remote_path);
    let listing = rc
        .run(&["lsjson", "-R", "--files-only", "--no-mimetype", &base], &[])
        .await
        .context("list the remote")?;
    let (items, skipped) = items_of(&listing)?;
    let (rc, base) = (&rc, &base);
    let fetch_store = store.clone();
    let source = syncsource::Source { db, store, id: src.id, device: syncsource::device_of(&src)? };
    syncsource::apply(source, Listing { items, skipped }, Rules::ALL, |item| {
        let store = fetch_store.clone();
        async move {
            let from = mirror::join(base, &item.path);
            syncsource::receive(
                &store,
                |path| async move { rc.run(&["copyto", &from, &path.to_string_lossy(), "--stats", "0", "--retries", "3"], &[]).await },
                |_, size| {
                    ensure!(size == item.size, "received {size} bytes, the remote lists {}", item.size);
                    Ok(())
                },
            )
            .await
        }
    })
    .await
}

#[cfg(test)]
#[path = "../../tests/unit/integrations/rclone_source.rs"]
mod tests;
