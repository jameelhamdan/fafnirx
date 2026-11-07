use super::*;

#[test]
fn work_is_listed_while_it_waits_and_runs_and_gone_after() {
    let tasks = Tasks::default();
    let a = tasks.begin("fetch", "list/a.mp3");
    assert!(!tasks.report(2).active[0].running, "waiting for a slot");
    a.started();
    assert!(tasks.report(2).active[0].running);
    a.finish(None);
    let r = tasks.report(2);
    assert!(r.active.is_empty());
    assert_eq!((r.made, r.failed.len()), (1, 0));
}

#[test]
fn a_failure_is_remembered_with_its_last_line_and_a_dropped_request_is_not() {
    let tasks = Tasks::default();
    tasks.begin("fetch", "a.mp3").finish(Some("banner\nreal reason\n".into()));
    drop(tasks.begin("list", "https://x"));
    let r = tasks.report(1);
    assert!(r.active.is_empty());
    assert_eq!((r.failed.len(), r.failed[0].error.as_str(), r.made), (1, "real reason", 0));
}
