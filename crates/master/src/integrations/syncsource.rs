//! What every source does with a listing of the files it can reach: compare it with what was imported before, then
//! remove and fetch. A source only lists (`Item`s) and fetches one file; the rest is here.

use crate::files::ingest::{self, NewVersion};
use crate::integrations::Outcome;
use crate::platform::db::{self, Db, Device};
use crate::platform::store::Store;
use crate::repo::sources;
use anyhow::Result;
use fafnirx_common::HASH_LEN;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

/// A file found at the source, known by the path it gets in the namespace.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: String,
    pub marker: String,
    pub size: u64,
    pub mtime_ns: i64,
}

/// A complete file in a temporary location.
pub struct Download {
    pub path: PathBuf,
    pub root: [u8; HASH_LEN],
    pub size: u64,
}

/// What a run has to do, decided from the listing and what was imported before.
#[derive(Default)]
struct Plan<'a> {
    unchanged: usize,
    /// New or changed content: (item, whether the path already existed).
    fetch: Vec<(&'a Item, bool)>,
    /// Paths that left the source.
    removals: Vec<String>,
}

/// Path -> version marker of everything imported so far.
type Known = HashMap<String, String>;

/// Pure: compares the listing with the database. `live` holds the paths that are currently not deleted, `removed` those
/// deleted in the web app: they stay deleted until the file changes on the remote.
fn plan<'a>(items: &'a [Item], known: &Known, live: &HashSet<String>, removed: &HashSet<String>) -> Plan<'a> {
    let mut plan = Plan::default();
    for item in items {
        let previous = known.get(&item.path);
        match previous {
            Some(marker) if *marker == item.marker && (removed.contains(&item.path) || live.contains(&item.path)) => {
                plan.unchanged += 1;
            }
            _ => plan.fetch.push((item, previous.is_some() || live.contains(&item.path))),
        }
    }
    let listed: HashSet<&str> = items.iter().map(|i| i.path.as_str()).collect();
    plan.removals = known.keys().filter(|path| !listed.contains(path.as_str())).cloned().collect();
    plan
}

fn commit(
    conn: &mut postgres::Client,
    store: &Store,
    source_id: i64,
    device: &Device,
    item: &Item,
    content: Download,
) -> Result<()> {
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
    sources::remember_item(conn, source_id, &item.path, &item.path, &item.marker)?;
    Ok(())
}

/// Applies a listing: `fetch` downloads one file of it to a temporary place.
pub async fn apply<F, Fut>(
    db: Db,
    store: Arc<Store>,
    source_id: i64,
    device: Device,
    items: Vec<Item>,
    skipped: usize,
    fetch: F,
) -> Result<Outcome>
where
    F: Fn(Item) -> Fut,
    Fut: Future<Output = Result<Download>>,
{
    let device_id = device.id;
    let (known, live, removed): (Known, HashSet<String>, HashSet<String>) = db::with(&db, move |c| {
        let known = sources::items(c, source_id)?.into_iter().map(|(_, path, version)| (path, version)).collect();
        let live = sources::live_paths(c, device_id)?.into_iter().collect();
        // Deleted in the web app: still in the trash, or already removed by the retention job (then only the
        // source's own record remembers it).
        let removed = sources::removed_in_web_app(c, device_id, source_id)?.into_iter().collect();
        Ok((known, live, removed))
    })
    .await?;
    let plan = plan(&items, &known, &live, &removed);
    let mut out = Outcome { skipped, unchanged: plan.unchanged, ..Default::default() };
    let mut failed = |what: &str, e: anyhow::Error| {
        tracing::warn!("source {}: {what}: {e:#}", device.name);
        out.failed.push(format!("{what}: {e:#}"));
    };

    // Removals go first so that a path is free before a new file takes it over.
    for path in plan.removals {
        let shown = path.clone();
        let (store, device) = (store.clone(), device.clone());
        let deleted = db::with(&db, move |c| {
            let changed = ingest::mark_deleted(c, &store, &device, &path, false)?;
            sources::forget_item(c, source_id, &path)?;
            Ok(changed)
        })
        .await;
        match deleted {
            Ok(true) => out.deleted += 1,
            Ok(false) => {}
            Err(e) => failed(&shown, e),
        }
    }

    for (item, is_update) in plan.fetch {
        let fetched = async {
            let content = fetch(item.clone()).await?;
            let (stored, store, device) = (item.clone(), store.clone(), device.clone());
            db::with(&db, move |c| commit(c, &store, source_id, &device, &stored, content)).await
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
mod tests {
    use super::*;

    fn item(path: &str, marker: &str) -> Item {
        Item { path: path.into(), marker: marker.into(), size: 0, mtime_ns: 0 }
    }

    fn known(rows: &[(&str, &str)]) -> Known {
        rows.iter().map(|(path, marker)| (path.to_string(), marker.to_string())).collect()
    }

    fn live(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn plan_classifies_every_kind_of_change() {
        let items = [item("d/same.txt", "m1"), item("d/edited.txt", "m2"), item("d/new.txt", "m6")];
        let known = known(&[("d/same.txt", "m1"), ("d/edited.txt", "old"), ("d/gone.txt", "m7")]);
        let live = live(&["d/same.txt", "d/edited.txt", "d/gone.txt"]);
        let p = plan(&items, &known, &live, &HashSet::new());

        assert_eq!(p.unchanged, 1);
        let fetch: Vec<_> = p.fetch.iter().map(|(i, update)| (i.path.as_str(), *update)).collect();
        assert_eq!(fetch, [("d/edited.txt", true), ("d/new.txt", false)]);
        assert_eq!(p.removals, ["d/gone.txt"]);
    }

    #[test]
    fn plan_refetches_what_the_master_no_longer_has() {
        // Same marker, but the file was purged (or never committed): download it again.
        let items = [item("d/a.txt", "m1")];
        let known = known(&[("d/a.txt", "m1")]);
        let p = plan(&items, &known, &live(&[]), &live(&[]));
        assert_eq!((p.unchanged, p.fetch.len()), (0, 1));
    }

    #[test]
    fn plan_leaves_what_was_deleted_in_the_web_app_until_it_changes() {
        let known = known(&[("d/a.txt", "m1")]);
        let gone = live(&["d/a.txt"]);
        let same = [item("d/a.txt", "m1")];
        let p = plan(&same, &known, &live(&[]), &gone);
        assert_eq!((p.unchanged, p.fetch.len()), (1, 0));
        let edited = [item("d/a.txt", "m2")];
        let p = plan(&edited, &known, &live(&[]), &gone);
        assert_eq!(p.fetch.len(), 1, "changed on the remote: fetched again");
    }
}
