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
