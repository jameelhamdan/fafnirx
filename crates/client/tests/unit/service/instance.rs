use super::*;

#[test]
fn only_one_process_holds_a_name_per_state_directory() {
    let dir = tempfile::tempdir().unwrap();
    assert!(claim(dir.path(), "daemon", 1).unwrap());
    // A second claim opens the file again and must not get the lock.
    assert!(!claim(dir.path(), "daemon", 1).unwrap(), "already held");
    assert!(claim(dir.path(), "tray", 1).unwrap(), "another name is independent");
    release("daemon");
    assert!(claim(dir.path(), "daemon", 1).unwrap(), "free again once released");
    release("daemon");
    release("tray");
}

#[test]
fn a_held_lock_is_seen_from_outside_and_a_free_one_is_not_taken() {
    // A name of its own: the locks a process holds are kept by name, and the other tests run at the same time.
    let dir = tempfile::tempdir().unwrap();
    assert!(!is_held(dir.path(), "probe"), "nobody holds it");
    assert!(!is_held(dir.path(), "probe"), "and looking did not take it");
    assert!(claim(dir.path(), "probe", 1).unwrap());
    assert!(is_held(dir.path(), "probe"));
    release("probe");
    assert!(!is_held(dir.path(), "probe"), "free again");
}

#[test]
fn an_installation_keeps_its_id_and_another_gets_its_own() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let first = client_id(a.path());
    assert_eq!(first.len(), 32);
    assert_eq!(client_id(a.path()), first, "kept across runs");
    assert_ne!(client_id(b.path()), first, "a second state directory is a second installation");
}

#[test]
fn different_state_directories_do_not_collide() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    assert!(claim(a.path(), "tray", 1).unwrap());
    assert!(claim(b.path(), "tray", 1).unwrap());
    release("tray");
}
