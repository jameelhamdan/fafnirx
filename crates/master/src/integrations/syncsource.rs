//! What every source does with a listing of the files it can reach: compare it with what was imported before, then
//! remove, rename and fetch. A source only lists (`Item`s) and fetches one file; the rest is here.

use crate::files::ingest::{self, NewVersion};
use crate::integrations::Outcome;
use crate::platform::db::{self, Db, Device};
use crate::platform::store::Store;
use crate::repo::{files, sources};
use anyhow::{Context, Result};
use fafnirx_common::HASH_LEN;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

/// A file found at the source, with the path it gets in the namespace.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
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

/// What a run has to do, decided from the Drive listing and what was imported before.
#[derive(Default)]
struct Plan<'a> {
    unchanged: usize,
    /// Same content under a new path: (old path, item). No download needed.
    renames: Vec<(&'a str, &'a Item)>,
    /// New or changed content: (item, whether the path already existed).
    fetch: Vec<(&'a Item, bool)>,
    removals: Vec<Removal>,
}

struct Removal {
    path: String,
    /// Set when the file left Drive; unset when only its old path is being vacated.
    remote_id: Option<String>,
}

/// Remote id -> (path, version marker) of everything imported so far.
type Known = HashMap<String, (String, String)>;

/// Pure: compares the listing with the database. `live` holds the paths that are currently not deleted, `removed` those
/// deleted in the web app: they stay deleted until the file changes on Drive.
fn plan<'a>(items: &'a [Item], known: &'a Known, live: &HashSet<String>, removed: &HashSet<String>) -> Plan<'a> {
    let now_paths: HashSet<&str> = items.iter().map(|i| i.path.as_str()).collect();
    let mut plan = Plan::default();
    for item in items {
        let previous = known.get(&item.id);
        match previous {
            Some((path, marker)) if *marker == item.marker && *path == item.path && removed.contains(path) => {
                plan.unchanged += 1;
            }
            Some((path, marker)) if *marker == item.marker && live.contains(path) => {
                if *path == item.path {
                    plan.unchanged += 1;
                } else if now_paths.contains(path.as_str()) {
                    plan.fetch.push((item, true)); // another file took the old name: do not rename onto it
                } else {
                    plan.renames.push((path, item));
                }
            }
            _ => plan.fetch.push((item, previous.is_some() || live.contains(&item.path))),
        }
    }
    let listed: HashSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
    for (id, (path, _)) in known {
        if !listed.contains(id.as_str()) {
            plan.removals.push(Removal { path: path.clone(), remote_id: Some(id.clone()) });
        }
    }
    for (item, _) in &plan.fetch {
        if let Some((old, _)) = known.get(&item.id).filter(|(old, _)| *old != item.path) {
            plan.removals.push(Removal { path: old.clone(), remote_id: None });
        }
    }
    // A path that another file now uses is not vacated.
    plan.removals.retain(|r| !now_paths.contains(r.path.as_str()));
    plan
}

fn commit(
    conn: &mut postgres::Client,
    store: &Store,
    source_id: i64,
    device: &Device,
    item: &Item,
    content: Download,
    renamed_from: Option<&str>,
) -> Result<()> {
    let version = NewVersion {
        device,
        path: &item.path,
        root: content.root,
        size: content.size,
        mtime: item.mtime_ns,
        mode: 0,
        renamed_from,
    };
    ingest::commit_version(conn, store, &content.path, &version)?;
    sources::remember_item(conn, source_id, &item.id, &item.path, &item.marker)?;
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
        let known =
            sources::items(c, source_id)?.into_iter().map(|(id, path, version)| (id, (path, version))).collect();
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
    for Removal { path, remote_id } in plan.removals {
        let shown = path.clone();
        let (store, device) = (store.clone(), device.clone());
        let deleted = db::with(&db, move |c| {
            let changed = ingest::mark_deleted(c, &store, &device, &path, false)?;
            if let Some(id) = remote_id {
                sources::forget_item(c, source_id, &id)?;
            }
            Ok(changed)
        })
        .await;
        match deleted {
            Ok(true) => out.deleted += 1,
            Ok(false) => {}
            Err(e) => failed(&shown, e),
        }
    }

    for (old, item) in plan.renames {
        let (old_path, moved) = (old.to_string(), item.clone());
        let (store, device) = (store.clone(), device.clone());
        let renamed = db::with(&db, move |c| {
            let (root, size) = files::live_content(c, device_id, &old_path)?.context("the old file is gone")?;
            let path = store.copy_live_to_temp(&device.name, &old_path, &root)?;
            commit(c, &store, source_id, &device, &moved, Download { path, root, size }, Some(&old_path))
        })
        .await;
        match renamed {
            Ok(()) => out.renamed += 1,
            Err(e) => failed(&item.path, e),
        }
    }

    for (item, is_update) in plan.fetch {
        let fetched = async {
            let content = fetch(item.clone()).await?;
            let (stored, store, device) = (item.clone(), store.clone(), device.clone());
            db::with(&db, move |c| commit(c, &store, source_id, &device, &stored, content, None)).await
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

    fn item(id: &str, path: &str, marker: &str) -> Item {
        Item { id: id.into(), path: path.into(), marker: marker.into(), size: 0, mtime_ns: 0 }
    }

    fn known(rows: &[(&str, &str, &str)]) -> Known {
        rows.iter().map(|(id, path, marker)| (id.to_string(), (path.to_string(), marker.to_string()))).collect()
    }

    fn live(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn plan_classifies_every_kind_of_change() {
        let items = [
            item("same", "d/same.txt", "m1"),
            item("edited", "d/edited.txt", "m2"),
            item("renamed", "d/new-name.txt", "m3"),
            item("moved-and-edited", "d/sub/moved.txt", "m5"),
            item("new", "d/new.txt", "m6"),
        ];
        let known = known(&[
            ("same", "d/same.txt", "m1"),
            ("edited", "d/edited.txt", "old"),
            ("renamed", "d/old-name.txt", "m3"),
            ("moved-and-edited", "d/moved.txt", "old"),
            ("gone", "d/gone.txt", "m7"),
        ]);
        let live = live(&["d/same.txt", "d/edited.txt", "d/old-name.txt", "d/moved.txt", "d/gone.txt"]);
        let p = plan(&items, &known, &live, &HashSet::new());

        assert_eq!(p.unchanged, 1);
        assert_eq!(
            p.renames.iter().map(|(old, i)| (*old, i.path.as_str())).collect::<Vec<_>>(),
            [("d/old-name.txt", "d/new-name.txt")]
        );
        let fetch: Vec<_> = p.fetch.iter().map(|(i, update)| (i.id.as_str(), *update)).collect();
        assert_eq!(fetch, [("edited", true), ("moved-and-edited", true), ("new", false)]);
        let mut removals: Vec<_> = p.removals.iter().map(|r| (r.path.as_str(), r.remote_id.as_deref())).collect();
        removals.sort();
        assert_eq!(removals, [("d/gone.txt", Some("gone")), ("d/moved.txt", None)]);
    }

    #[test]
    fn plan_refetches_what_the_master_no_longer_has() {
        // Same marker, but the file was purged (or never committed): download it again.
        let items = [item("a", "d/a.txt", "m1")];
        let known = known(&[("a", "d/a.txt", "m1")]);
        let p = plan(&items, &known, &live(&[]), &live(&[]));
        assert_eq!((p.unchanged, p.fetch.len()), (0, 1));
    }

    #[test]
    fn plan_leaves_what_was_deleted_in_the_web_app_until_it_changes() {
        let known = known(&[("a", "d/a.txt", "m1")]);
        let gone = live(&["d/a.txt"]);
        let same = [item("a", "d/a.txt", "m1")];
        let p = plan(&same, &known, &live(&[]), &gone);
        assert_eq!((p.unchanged, p.fetch.len()), (1, 0));
        let edited = [item("a", "d/a.txt", "m2")];
        let p = plan(&edited, &known, &live(&[]), &gone);
        assert_eq!(p.fetch.len(), 1, "changed on Drive: fetched again");
    }

    #[test]
    fn plan_never_renames_onto_a_path_another_file_still_uses() {
        // a and b swap names: renaming either would clobber the other.
        let items = [item("a", "d/b.txt", "m1"), item("b", "d/a.txt", "m2")];
        let known = known(&[("a", "d/a.txt", "m1"), ("b", "d/b.txt", "m2")]);
        let p = plan(&items, &known, &live(&["d/a.txt", "d/b.txt"]), &live(&[]));
        assert!(p.renames.is_empty());
        assert_eq!(p.fetch.len(), 2);
        assert!(p.removals.is_empty(), "both paths are still in use");
    }
}
