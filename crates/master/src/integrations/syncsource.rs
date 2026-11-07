//! What every source does with a listing of the files it can reach: compare it with what was imported before, then
//! remove and fetch. A source only lists (`Item`s) and fetches one file; the rest is here.

use crate::files::ingest::{self, NewVersion};
use crate::integrations::SourceRun;
use crate::platform::db::{self, Db, Device};
use crate::platform::store::{hash_file, Store};
use crate::repo::links::{self, Due};
use crate::repo::sources;
use anyhow::{Context, Result};
use fafnirx_common::HASH_LEN;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

/// A file found at the source, known by the path it gets in the namespace.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: String,
    /// What the file is, when that is not its path (see `FetchEntry::key`).
    pub key: Option<String>,
    pub marker: String,
    pub size: u64,
    pub mtime_ns: i64,
}

impl Item {
    /// What the source remembers it by.
    fn id(&self) -> &str {
        self.key.as_deref().unwrap_or(&self.path)
    }
}

/// The folder a source writes into.
pub fn device_of(link: &Due) -> Result<Device> {
    Ok(Device {
        id: link.folder_id.context("the source has no folder")?,
        name: link.folder.clone().context("the folder of the source is gone")?,
    })
}

/// A complete file in a temporary location.
pub struct Download {
    pub path: PathBuf,
    pub root: [u8; HASH_LEN],
    pub size: u64,
}

/// Receives one file in a temporary place: `fetch` writes it there and says what it reports about it, `check` compares that and
/// the size that was hashed. A file that did not arrive whole is removed.
pub async fn receive<T, Fut>(store: &Store, fetch: impl FnOnce(PathBuf) -> Fut, check: impl FnOnce(T, u64) -> Result<()>) -> Result<Download>
where
    Fut: Future<Output = Result<T>>,
{
    let path = store.temp_path();
    let received = async {
        let reported = fetch(path.clone()).await?;
        let target = path.clone();
        let (root, size) = tokio::task::spawn_blocking(move || hash_file(&target)).await??;
        check(reported, size)?;
        Ok(Download { path: path.clone(), root, size })
    }
    .await;
    if received.is_err() {
        let _ = tokio::fs::remove_file(&path).await;
    }
    received
}

/// What a run has to do, decided from the listing and what was imported before.
#[derive(Default)]
struct Plan<'a> {
    unchanged: usize,
    /// New or changed content: (item, whether the path already existed, the path it leaves when the file is made again
    /// somewhere else).
    fetch: Vec<(&'a Item, bool, Option<String>)>,
    /// The same content under another name: (item, the path it had). Moved, not fetched.
    renames: Vec<(&'a Item, String)>,
    /// Remembered files that left the source: (what it is remembered by, path).
    removals: Vec<(String, String)>,
}

/// One file imported before.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    id: String,
    path: String,
    marker: String,
}

/// Everything imported so far, by what the source remembers it by (its key, else its path).
type Known = HashMap<String, Entry>;

fn known_of(rows: impl IntoIterator<Item = (String, String, String)>) -> Known {
    rows.into_iter().map(|(id, path, marker)| (id.clone(), Entry { id, path, marker })).collect()
}

/// Pure: compares the listing with the database. `live` holds the paths that are currently not deleted, `removed` those
/// deleted in the web app: they stay deleted until the file changes on the remote.
fn plan<'a>(items: &'a [Item], known: &Known, live: &HashSet<String>, removed: &HashSet<String>) -> Plan<'a> {
    let mut plan = Plan::default();
    let mut matched = HashSet::new();
    for item in items {
        let Some(before) = known.get(item.id()) else {
            plan.fetch.push((item, live.contains(&item.path), None));
            continue;
        };
        matched.insert(before.id.clone());
        let kept = removed.contains(&item.path) || live.contains(&item.path);
        if before.marker == item.marker && before.path == item.path && kept {
            plan.unchanged += 1;
        } else if before.marker == item.marker && removed.contains(&before.path) && !live.contains(&item.path) {
            // Deleted in the web app under its old name: it stays deleted under the new one.
            plan.unchanged += 1;
        } else if before.marker == item.marker && before.path != item.path && live.contains(&before.path) && !live.contains(&item.path) {
            plan.renames.push((item, before.path.clone()));
        } else {
            let leaves = (before.path != item.path).then(|| before.path.clone());
            plan.fetch.push((item, true, leaves));
        }
    }
    plan.removals = known
        .values()
        .filter(|e| !matched.contains(&e.id))
        .map(|e| (e.id.clone(), e.path.clone()))
        .collect();
    plan
}

/// Where a source's files are stored: the master's store, the source and the device (folder) it writes into.
struct Target<'a> {
    store: &'a Store,
    source_id: i64,
    device: &'a Device,
}

fn commit(conn: &mut postgres::Client, to: &Target, item: &Item, content: Download) -> Result<()> {
    let Target { store, source_id, device } = *to;
    let version = NewVersion {
        device,
        path: &item.path,
        root: content.root,
        size: content.size,
        mtime: item.mtime_ns,
        mode: 0,
        renamed_from: None,
    };
    ingest::commit_version(conn, store, &content.path, &version)?;
    links::remember_remote(
        conn,
        &links::RemoteItem {
            link_id: source_id,
            device_id: device.id,
            path: &item.path,
            remote_id: item.id(),
            version: &item.marker,
        },
    )?;
    Ok(())
}

/// What a source offers: the files it found, and how many entries it left out (unusable names, unknown sizes, not available yet).
pub struct Listing {
    pub items: Vec<Item>,
    pub skipped: usize,
}

/// What a run may do besides fetching what is new.
#[derive(Clone, Copy)]
pub struct Rules {
    /// At most this many files are fetched (each is committed as soon as it is there); the rest are counted in
    /// `SourceRun::deferred` for the caller to schedule.
    pub limit: usize,
    /// Whether files missing from the listing are deleted. Off when the listing may be incomplete.
    pub removals: bool,
}

impl Rules {
    /// Everything: a listing that is the whole truth, and all of it fetched.
    pub const ALL: Rules = Rules { limit: usize::MAX, removals: true };
}

/// A source being run: the database and store, the source and the device (folder) it writes into.
pub struct Source {
    pub db: Db,
    pub store: Arc<Store>,
    pub id: i64,
    pub device: Device,
}

/// Applies a listing: `fetch` downloads one file of it to a temporary place.
pub async fn apply<F, Fut>(src: Source, listing: Listing, rules: Rules, fetch: F) -> Result<SourceRun>
where
    F: Fn(Item) -> Fut,
    Fut: Future<Output = Result<Download>>,
{
    let Source { db, store, id: source_id, device } = src;
    let device_id = device.id;
    let (known, live, removed): (Known, HashSet<String>, HashSet<String>) = db::with(&db, move |c| {
        let known = known_of(links::items(c, source_id)?.into_iter().map(|i| (i.remote_id, i.path, i.version)));
        let live = sources::live_paths(c, device_id)?.into_iter().collect();
        // Deleted in the web app: still in the trash, or already removed by the retention job (then only the
        // source's own record remembers it).
        let removed = sources::removed_in_web_app(c, device_id, source_id)?.into_iter().collect();
        Ok((known, live, removed))
    })
    .await?;
    let Listing { items, skipped } = listing;
    let mut plan = plan(&items, &known, &live, &removed);
    if !rules.removals {
        plan.removals.clear();
    }
    let mut out = SourceRun { skipped, unchanged: plan.unchanged, ..Default::default() };
    let mut failed = |what: &str, e: anyhow::Error| {
        tracing::warn!("source {}: {what}: {e:#}", device.name);
        out.failed.push(format!("{what}: {e:#}"));
    };

    // Removals go first so that a path is free before a new file takes it over.
    for (id, path) in plan.removals {
        let shown = path.clone();
        let (store, device) = (store.clone(), device.clone());
        let deleted = db::with(&db, move |c| {
            let changed = ingest::mark_deleted(c, &store, &device, &path, false)?;
            links::forget_remote(c, source_id, &id)?;
            Ok(changed)
        })
        .await;
        match deleted {
            Ok(true) => out.deleted += 1,
            Ok(false) => {}
            Err(e) => failed(&shown, e),
        }
    }

    // A file that only got another name is moved, not made again.
    for (item, from) in plan.renames {
        let (stored, store, device, from) = (item.clone(), store.clone(), device.clone(), from);
        let moved = db::with(&db, move |c| {
            ingest::move_path(c, &store, &device, &from, &stored.path)?;
            links::remember_remote(
                c,
                &links::RemoteItem {
                    link_id: source_id,
                    device_id: device.id,
                    path: &stored.path,
                    remote_id: stored.id(),
                    version: &stored.marker,
                },
            )?;
            Ok(())
        })
        .await;
        match moved {
            Ok(()) => out.updated += 1,
            Err(e) => failed(&item.path, e),
        }
    }

    let wanted = plan.fetch.len();
    for (n, (item, is_update, leaves)) in plan.fetch.into_iter().enumerate() {
        if n >= rules.limit {
            out.deferred = wanted - n;
            break;
        }
        let fetched = async {
            let content = fetch(item.clone()).await?;
            let (stored, store, device) = (item.clone(), store.clone(), device.clone());
            db::with(&db, move |c| {
                commit(c, &Target { store: &store, source_id, device: &device }, &stored, content)?;
                // Made again under another name (another format): the file it replaces goes.
                if let Some(old) = leaves {
                    ingest::mark_deleted(c, &store, &device, &old, false)?;
                }
                Ok(())
            })
            .await
        }
        .await;
        match fetched {
            Ok(()) if is_update => out.updated += 1,
            Ok(()) => out.added += 1,
            Err(e) => failed(&item.path, e),
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../../tests/unit/integrations/syncsource.rs"]
mod tests;
