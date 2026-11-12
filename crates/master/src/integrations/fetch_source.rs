//! Sources that a module reads for the master. The module owns the kind: it checks the settings a user gave (`/fetch/check`), lists
//! what the source offers and writes one file at a time to the master's `tmp` folder; everything else (what is new, what left,
//! the commit) is `syncsource`, the same as for rclone. The master knows no kind by name: a source of any kind other than `rclone`
//! is read by the module that lists `fetch.<kind>` among its capabilities, and fails with a message that says so when none runs.
//!
//! A run fetches at most `max_per_run` files and, when more are waiting, asks for another run at once: a long list
//! arrives over several runs, and a restart or a failure loses at most the file being made.

use crate::integrations::syncsource::{self, Item, Listing, Rules};
use crate::integrations::SourceRun;
use crate::modules;
use crate::platform::db::{self, Db};
use crate::platform::store::Store;
use crate::repo::links::Due;
use anyhow::{bail, ensure, Context, Result};
use fafnirx_common::{clean_remote_path, unix_now};
use fafnirx_fetch_api::FetchEntry;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// How many files one run fetches unless the source says otherwise.
const DEFAULT_PER_RUN: u64 = 25;
const MAX_PER_RUN: u64 = 500;

fn per_run(config: &Value) -> Result<u64> {
    match config.get("max_per_run").filter(|v| !v.is_null()) {
        None => Ok(DEFAULT_PER_RUN),
        Some(v) => match v.as_u64() {
            Some(n) if (1..=MAX_PER_RUN).contains(&n) => Ok(n),
            _ => bail!("Files per run: between 1 and {MAX_PER_RUN}"),
        },
    }
}

/// The settings a user sent for a source of `kind`: the confirmation and the limit are the master's to check, the rest is the
/// module's, which reduces it to what it reads (anything else is dropped) and says how the source is labelled.
pub async fn validate(kind: &str, config: &Value) -> Result<Value> {
    if config["acknowledged"].as_bool() != Some(true) {
        bail!("Confirm that you have the right to copy what this source downloads");
    }
    let limit = per_run(config)?;
    let checked = modules::fetch_check(kind, config).await?;
    let Value::Object(mut out) = checked.config else {
        bail!("the module did not answer with settings");
    };
    out.insert("acknowledged_at".into(), json!(unix_now()));
    out.insert("max_per_run".into(), json!(limit));
    out.insert("label".into(), json!(checked.label));
    Ok(Value::Object(out))
}

/// What the Sources page shows of a source: the label its module gave when it was added.
pub fn label(config: &Value) -> String {
    config["label"].as_str().unwrap_or_default().to_string()
}

/// The entries of a module's listing as items, keeping what the module wants back when it is asked for a file, and how many
/// entries were left out (a path that cannot be a file's, or one that appears twice).
fn items_of(entries: Vec<FetchEntry>) -> (Vec<Item>, HashMap<String, FetchEntry>, usize) {
    let (mut items, mut kept, mut skipped, mut seen) = (Vec::new(), HashMap::new(), 0, HashSet::new());
    for mut entry in entries {
        match clean_remote_path(&entry.path) {
            Ok(path) if seen.insert(path.clone()) => {
                entry.path = path.clone();
                items.push(Item {
                    path: path.clone(),
                    key: entry.key.clone(),
                    marker: entry.marker.clone(),
                    size: entry.size,
                    mtime_ns: entry.mtime_ns,
                });
                kept.insert(path, entry);
            }
            _ => skipped += 1,
        }
    }
    (items, kept, skipped)
}

pub async fn run(db: Db, store: Arc<Store>, src: Due) -> Result<SourceRun> {
    let config: Value = serde_json::from_str(&src.config).context("source configuration is invalid")?;
    let kind = src.kind.clone();
    let limit = per_run(&config)? as usize;
    let listing = modules::fetch_list(&kind, &config).await.context("list the source")?;
    let (items, entries, left_out) = items_of(listing.items);
    // Files are deleted for leaving the source only when the listing is the whole truth. One that is cut short, and one
    // with nothing in it (the usual sign that the source could not be read), delete nothing.
    let rules = Rules { limit, removals: listing.complete && !items.is_empty() };
    let (config, entries, kind) = (&config, &entries, &kind);
    let fetch_store = store.clone();
    let source_id = src.id;
    let folder = syncsource::folder_of(&src)?;
    let out = syncsource::apply(
        syncsource::Source { db: db.clone(), store, id: source_id, folder },
        Listing { items, skipped: listing.skipped + left_out },
        rules,
        |item| {
            let store = fetch_store.clone();
            async move {
                let entry = entries.get(&item.path).context("the file is not in the listing")?;
                syncsource::receive(
                    &store,
                    |path| async move { modules::fetch_get(kind, config, entry, &path).await },
                    |reported, size| {
                        ensure!(
                            size != 0 && (reported == 0 || size == reported),
                            "received {size} bytes, the module wrote {reported}"
                        );
                        Ok(())
                    },
                )
                .await
            }
        },
    )
    .await?;
    if out.deferred > 0 {
        // More is waiting than one run takes: run again as soon as this one is recorded, unless it was paused meanwhile.
        db::with(&db, move |c| {
            crate::repo::links::continue_run(c, source_id)?;
            Ok(())
        })
        .await?;
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../../tests/unit/integrations/fetch_source.rs"]
mod tests;
