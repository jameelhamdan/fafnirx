use super::*;

/// A folder with twelve files, in step with the master.
async fn synced_rig() -> (Rig, Vec<String>) {
    let mut rig = Rig::new().await;
    let names: Vec<String> = (0..12).map(|i| format!("f{i}.txt")).collect();
    for n in &names {
        rig.remote_has(n, b"x");
        rig.cycle().await;
    }
    (rig, names)
}

/// The same, with all of them deleted by another device, so that the deletions are held back.
async fn held_rig() -> (Rig, Vec<String>) {
    let (mut rig, names) = synced_rig().await;
    rig.held.lock().unwrap().changes = names.iter().map(|n| master::gone(n)).collect();
    rig.cycle().await;
    (rig, names)
}

#[tokio::test]
async fn held_deletions_are_kept_in_the_state_so_they_show_with_the_client_stopped() {
    let (rig, _) = held_rig().await;
    assert_eq!(rig.attached.engine.state.pull_hold().unwrap().map(|(n, _)| n), Some(12));
    let synced = Folder {
        name: "alice/family".into(),
        path: rig.folder.path().to_path_buf(),
        target: crate::config::Target::Folder,
        id: None,
        mode: Mode::Keep,
        ask_timeout_hours: 24,
    };
    // Another process (`fafnirx status`) opens the same database and sees the same.
    let reopened = State::open(&state_file(rig._state.path(), "alice/family")).unwrap();
    let seen = status_of(&synced, &reopened);
    assert_eq!((seen.held, seen.files), (12, 12));
    assert!(seen.last_cycle > 0, "and how the last cycle ended");
    assert_eq!(seen, rig.attached.status());
    assert!(
        seen.last_error.contains("keep alice/family"),
        "the last problem says what to do: {}",
        seen.last_error
    );
    // Held means held: no later cycle applies them by itself.
    let mut rig = rig;
    for _ in 0..3 {
        rig.cycle().await;
    }
    assert_eq!(rig.attached.engine.state.pull_hold().unwrap().map(|(n, _)| n), Some(12));
    assert!(rig.local("f0.txt").is_some());
}

#[tokio::test]
async fn deletions_made_in_the_web_app_are_applied_at_once_however_many() {
    let (mut rig, names) = synced_rig().await;
    rig.held.lock().unwrap().changes = names.iter().map(|n| master::gone_in_the_web_app(n)).collect();
    let report = rig.cycle().await;
    assert_eq!(report.deleted, 12, "deliberate, and in the Trash to restore");
    assert!(names.iter().all(|n| rig.local(n).is_none()));
    assert!(rig.attached.engine.state.pull_hold().unwrap().is_none());
}

#[tokio::test]
async fn only_what_other_devices_deleted_is_held() {
    let (mut rig, names) = synced_rig().await;
    let mut changes: Vec<_> = names[..2].iter().map(|n| master::gone_in_the_web_app(n)).collect();
    changes.extend(names[2..].iter().map(|n| master::gone(n)));
    rig.held.lock().unwrap().changes = changes;
    let report = rig.cycle().await;
    assert_eq!(report.deleted, 2, "the web app's two go, the other device's ten wait");
    assert_eq!(rig.attached.engine.state.pull_hold().unwrap().map(|(n, _)| n), Some(10));
}

#[tokio::test]
async fn held_deletions_can_be_released() {
    let (mut rig, names) = held_rig().await;
    rig.attached.engine.state.request_release().unwrap();
    let report = rig.cycle().await;
    assert_eq!(report.deleted, 12);
    assert!(names.iter().all(|n| rig.local(n).is_none()));
    assert!(rig.attached.engine.state.pull_hold().unwrap().is_none(), "nothing is held any more");
    assert_eq!(rig.attached.status().held, 0);
}

#[tokio::test]
async fn held_deletions_can_be_undone_by_keeping_the_files() {
    let (mut rig, names) = held_rig().await;
    rig.attached.engine.state.request_keep().unwrap();
    let report = rig.cycle().await;
    assert_eq!(report.deleted, 0);
    assert!(names.iter().all(|n| rig.local(n).is_some()), "the files stay");
    assert!(rig.attached.engine.state.pull_hold().unwrap().is_none());
    let sent = rig.held.lock().unwrap().commits.clone();
    assert_eq!(sent.len(), 12, "and go up again, since they are no longer in step: {sent:?}");
    // The master's deletions were let go of, not held for ever: the next listing is quiet.
    assert_eq!(rig.attached.status().held, 0);
}

#[tokio::test]
async fn a_request_to_keep_that_was_not_needed_does_not_wait_for_the_next_hold() {
    let (mut rig, _) = held_rig().await;
    rig.attached.engine.state.request_keep().unwrap();
    // Before it is used the deletions shrink to a few, which are not held: the request goes with them.
    rig.held.lock().unwrap().changes = vec![master::gone("f0.txt"), master::gone("f1.txt")];
    let report = rig.cycle().await;
    assert_eq!(report.deleted, 2);
    assert!(!rig.attached.engine.state.take_keep().unwrap(), "used up, not left behind");
    assert!(rig.attached.engine.state.pull_hold().unwrap().is_none());
}
