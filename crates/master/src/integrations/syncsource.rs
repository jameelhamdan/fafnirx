//! What every source does with a listing of the files it can reach: compare it with what was imported before, then
//! remove and fetch. A source only lists (`Item`s) and fetches one file; the rest is here.

use crate::files::ingest::{self, NewVersion};
use crate::integrations::Outcome;
use crate::platform::db::{self, Db, Device};
use crate::platform::store::Store;
use crate::repo::{links, sources};
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

fn commit(conn: &mut postgres::Client, store: &Store, source_id: i64, device: &Device, item: &Item, content: Download) -> Result<()> {
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
    links::remember_remote(conn, source_id, device.id, &item.path, item.id(), &item.marker)?;
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
    /// `Outcome::deferred` for the caller to schedule.
    pub limit: usize,
    /// Whether files missing from the listing are deleted. Off when the listing may be incomplete.
    pub removals: bool,
}

impl Rules {
    /// Everything: a listing that is the whole truth, and all of it fetched.
    pub const ALL: Rules = Rules { limit: usize::MAX, removals: true };
}

/// Applies a listing: `fetch` downloads one file of it to a temporary place.
pub async fn apply<F, Fut>(db: Db, store: Arc<Store>, source_id: i64, device: Device, listing: Listing, rules: Rules, fetch: F) -> Result<Outcome>
where
    F: Fn(Item) -> Fut,
    Fut: Future<Output = Result<Download>>,
{
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
    let mut out = Outcome { skipped, unchanged: plan.unchanged, ..Default::default() };
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
            links::remember_remote(c, source_id, device.id, &stored.path, stored.id(), &stored.marker)?;
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
                commit(c, &store, source_id, &device, &stored, content)?;
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
mod tests {
    use super::*;

    fn item(path: &str, marker: &str) -> Item {
        Item { path: path.into(), key: None, marker: marker.into(), size: 0, mtime_ns: 0 }
    }

    fn keyed(key: &str, path: &str, marker: &str) -> Item {
        Item { key: Some(key.into()), ..item(path, marker) }
    }

    /// What was imported, by path (a file without a key is remembered by its path).
    fn known(rows: &[(&str, &str)]) -> Known {
        known_of(rows.iter().map(|(path, marker)| (path.to_string(), path.to_string(), marker.to_string())))
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
        let fetch: Vec<_> = p.fetch.iter().map(|(i, update, _)| (i.path.as_str(), *update)).collect();
        assert_eq!(fetch, [("d/edited.txt", true), ("d/new.txt", false)]);
        assert_eq!(p.removals, [("d/gone.txt".to_string(), "d/gone.txt".to_string())]);
    }

    #[test]
    fn a_file_with_a_key_is_the_same_file_under_another_name_or_format() {
        let before = known_of([("id1".to_string(), "d/Old name.mp4".to_string(), "mp4".to_string())]);
        let here = live(&["d/Old name.mp4"]);

        // Only the name changed: moved, nothing fetched.
        let renamed = [keyed("id1", "d/New name.mp4", "mp4")];
        let p = plan(&renamed, &before, &here, &HashSet::new());
        assert_eq!((p.fetch.len(), p.removals.len(), p.renames.len()), (0, 0, 1));
        assert_eq!((p.renames[0].0.path.as_str(), p.renames[0].1.as_str()), ("d/New name.mp4", "d/Old name.mp4"));

        // The format changed: made again at the new path, and the old one goes once that worked.
        let other = [keyed("id1", "d/Old name.mkv", "mkv")];
        let p = plan(&other, &before, &here, &HashSet::new());
        assert_eq!((p.fetch.len(), p.removals.len(), p.renames.len()), (1, 0, 0));
        assert_eq!(p.fetch[0].2.as_deref(), Some("d/Old name.mp4"));

        // Deleted in the web app, then renamed at the source: it stays deleted.
        let p = plan(&renamed, &before, &live(&[]), &live(&["d/Old name.mp4"]));
        assert_eq!((p.unchanged, p.fetch.len(), p.renames.len()), (1, 0, 0));

        // A key that is no longer listed is a file that left the source.
        let p = plan(&[], &before, &here, &HashSet::new());
        assert_eq!(p.removals, [("id1".to_string(), "d/Old name.mp4".to_string())]);
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
