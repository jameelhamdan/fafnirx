use super::*;

#[test]
fn backoff_then_skip_until_changed() {
    let dir = tempfile::tempdir().unwrap();
    let st = State::open(&dir.path().join("s.db")).unwrap();
    assert!(may_attempt(&st.retry("a").unwrap(), 1, 1, 0));
    assert_eq!(st.record_failure(&Failure { path: "a", size: 1, mtime_ns: 1, error: "x" }, 1000).unwrap(), 1);
    let r = st.retry("a").unwrap();
    assert!(!may_attempt(&r, 1, 1, 1000 + 599));
    assert!(may_attempt(&r, 1, 1, 1000 + 600));
    st.record_failure(&Failure { path: "a", size: 1, mtime_ns: 1, error: "x" }, 2000).unwrap();
    assert_eq!(st.retry("a").unwrap().unwrap().next_retry, 2000 + 3600);
    assert_eq!(st.record_failure(&Failure { path: "a", size: 1, mtime_ns: 1, error: "x" }, 3000).unwrap(), 3);
    let r = st.retry("a").unwrap();
    assert!(!may_attempt(&r, 1, 1, i64::MAX / 2)); // skipped while unchanged
    assert!(may_attempt(&r, 1, 2, 0)); // changed again
    st.set_synced(&Synced { path: "a".into(), size: 1, mtime_ns: 2, root_hash: vec![0; 32] }).unwrap();
    assert!(st.retry("a").unwrap().is_none());
}

#[test]
fn what_was_sent_only_counts_for_the_master_it_was_sent_to() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = State::open(&dir.path().join("s.db")).unwrap();
    assert_eq!(st.adopt_master("one").unwrap(), 0, "a new record has nothing to clear");
    assert!(!st.initial_sync_done());
    st.mark_initial_sync_done().unwrap();
    assert!(st.initial_sync_done());
    st.set_synced(&Synced { path: "a".into(), size: 1, mtime_ns: 2, root_hash: vec![0; 32] }).unwrap();
    st.record_failure(&Failure { path: "b", size: 1, mtime_ns: 1, error: "x" }, 1000).unwrap();
    assert_eq!(st.adopt_master("one").unwrap(), 0, "the same master: kept");
    assert!(st.synced("a").unwrap().is_some());
    assert_eq!(st.adopt_master("two").unwrap(), 1, "another master: everything is pending again");
    assert!(!st.initial_sync_done(), "and the first sync is to be done again");
    assert!(st.synced("a").unwrap().is_none() && st.retry("b").unwrap().is_none());
    assert_eq!(st.adopt_master("two").unwrap(), 0);
}

#[test]
fn what_was_synced_only_counts_for_the_folder_it_was_synced_with() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = State::open(&dir.path().join("s.db")).unwrap();
    assert_eq!(st.adopt_folder("/home/a/Family").unwrap(), 0);
    st.set_synced(&Synced { path: "a".into(), size: 1, mtime_ns: 2, root_hash: vec![0; 32] }).unwrap();
    st.set_meta("synced_cursor", "100").unwrap();
    assert_eq!(st.adopt_folder("/home/a/Family").unwrap(), 0, "the same folder: kept");
    assert!(st.synced("a").unwrap().is_some());
    assert_eq!(st.adopt_folder("/home/a/Family2").unwrap(), 1, "another folder: nothing carries over");
    assert!(st.synced("a").unwrap().is_none() && st.meta("synced_cursor").unwrap().is_none());
}
