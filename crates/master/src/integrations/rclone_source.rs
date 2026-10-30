//! A source that pulls files from any storage rclone supports (OneDrive, Dropbox, WebDAV, SFTP, S3, ...).
//!
//! Each run lists the remote recursively and hands the listing to `syncsource`. rclone has no stable file ids, so a
//! file is known by its path: a rename downloads the file again. A file counts as changed when its size or its
//! modification time differs from the last run.

use crate::integrations::syncsource::{self, Download, Item};
use crate::integrations::{mirror, Outcome, SourceRow};
use crate::platform::db::Db;
use crate::platform::store::Store;
use anyhow::{bail, Context, Result};
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
        items.push(Item { marker: format!("{}:{mtime_ns}", l.size), path, size: l.size as u64, mtime_ns });
    }
    Ok((items, skipped))
}

pub async fn run(db: Db, store: Arc<Store>, src: SourceRow) -> Result<Outcome> {
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
    let listing =
        rc.run(&["lsjson", "-R", "--files-only", "--no-mimetype", &base], &[]).await.context("list the remote")?;
    let (items, skipped) = items_of(&listing)?;
    let (rc, base) = (&rc, &base);
    let fetch_store = store.clone();
    syncsource::apply(db, store, src.id, src.device, items, skipped, |item| {
        let store = fetch_store.clone();
        async move {
            let path = store.temp_path();
            let received = async {
                let from = mirror::join(base, &item.path);
                rc.run(&["copyto", &from, &path.to_string_lossy(), "--stats", "0", "--retries", "3"], &[]).await?;
                let target = path.clone();
                let (root, size) =
                    tokio::task::spawn_blocking(move || crate::platform::store::hash_file(&target)).await??;
                if size != item.size {
                    bail!("received {size} bytes, the remote lists {}", item.size);
                }
                Ok(Download { path: path.clone(), root, size })
            }
            .await;
            if received.is_err() {
                let _ = tokio::fs::remove_file(&path).await;
            }
            received
        }
    })
    .await
}

struct RemoveOnDrop(std::path::PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_becomes_items_and_skips_what_cannot_be_kept() {
        let json = r#"[
            {"Path":"a/b.txt","Name":"b.txt","Size":12,"ModTime":"2024-05-01T10:00:00.5Z","IsDir":false},
            {"Path":"a","Name":"a","Size":-1,"ModTime":"2024-05-01T10:00:00Z","IsDir":true},
            {"Path":"unknown","Name":"unknown","Size":-1,"ModTime":"2024-05-01T10:00:00Z","IsDir":false},
            {"Path":"../escape","Name":"escape","Size":1,"ModTime":"2024-05-01T10:00:00Z","IsDir":false}
        ]"#;
        let (items, skipped) = items_of(json).unwrap();
        assert_eq!(skipped, 2, "an unknown size and a path that leaves the folder");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path, "a/b.txt");
        assert_eq!(items[0].marker, "12:1714557600500000000");
        assert!(items_of("not json").is_err());
    }

    #[test]
    fn rfc3339_to_unix_nanoseconds() {
        assert_eq!(parse_time_ns("1970-01-01T00:00:01.5Z"), 1_500_000_000);
        assert_eq!(parse_time_ns("garbage"), 0);
    }
}
