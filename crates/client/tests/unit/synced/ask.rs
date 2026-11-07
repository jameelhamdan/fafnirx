use super::*;

async fn waiting() -> Rig {
    let mut rig = Rig::new().await.with_policy(Mode::Ask);
    std::fs::write(rig.folder.path().join("b.txt"), "mine").unwrap();
    rig.remote_has("b.txt", b"theirs");
    rig.cycle().await;
    rig.nothing_new();
    rig
}

#[tokio::test]
async fn ask_changes_nothing_on_either_side_until_answered() {
    let mut rig = waiting().await;
    assert_eq!(rig.local("b.txt").as_deref(), Some("mine"));
    let pending = rig.attached.engine.state.pendings().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!((pending[0].path.as_str(), pending[0].local_size), ("b.txt", 4));
    assert_eq!(rig.attached.status().conflicts.len(), 1, "the tray is told");
    rig.cycle().await;
    assert!(rig.held.lock().unwrap().commits.is_empty(), "it is not sent while it waits");
    assert_eq!(rig.local("b.txt").as_deref(), Some("mine"));
}

#[tokio::test]
async fn the_answer_is_applied_at_the_next_cycle() {
    for (choice, name, ours_aside, sent) in [("theirs", "theirs", true, false), ("both", "theirs", false, false)] {
        let mut rig = waiting().await;
        assert_eq!(rig.attached.engine.state.answer_pending(Some("b.txt"), choice).unwrap(), 1);
        rig.cycle().await;
        assert_eq!(rig.local("b.txt").as_deref(), Some(name), "{choice}");
        assert_eq!(rig.aside().len(), usize::from(ours_aside), "{choice}");
        assert_eq!(rig.conflicts().len(), usize::from(choice == "both"), "{choice}");
        assert_eq!(rig.sent("b.txt"), sent, "{choice}");
        assert!(rig.attached.engine.state.pendings().unwrap().is_empty(), "{choice}: done");
    }
}

#[tokio::test]
async fn a_change_listed_again_after_the_answer_is_not_asked_about_again() {
    let mut rig = waiting().await;
    // The cursor was held back, so the master lists the same change once more.
    rig.remote_has("b.txt", b"theirs");
    rig.attached.engine.state.answer_pending(Some("b.txt"), "theirs").unwrap();
    rig.cycle().await;
    assert!(rig.attached.engine.state.pendings().unwrap().is_empty(), "not asked again");
    assert_eq!(rig.local("b.txt").as_deref(), Some("theirs"));
    assert!(!rig.sent("b.txt"));
}

#[tokio::test]
async fn a_newer_version_of_the_master_needs_a_new_answer() {
    let rig = waiting().await;
    let state = &rig.attached.engine.state;
    state.answer_pending(Some("b.txt"), "theirs").unwrap();
    let mut newer = state.pendings().unwrap().remove(0);
    newer.root_hash = vec![9; 32];
    state.add_pending(&newer).unwrap();
    assert_eq!(state.pendings().unwrap()[0].choice, None, "the answer was about another version");
}

#[tokio::test]
async fn nobody_answering_keeps_both_after_the_timeout() {
    let mut rig = waiting().await;
    rig.attached.synced.ask_timeout_hours = 1;
    rig.cycle().await;
    assert_eq!(rig.attached.engine.state.pendings().unwrap().len(), 1, "not yet");
    rig.attached.engine.state.backdate_pending(unix_now() - 2 * 3600).unwrap();
    rig.cycle().await;
    assert_eq!(rig.local("b.txt").as_deref(), Some("theirs"));
    assert_eq!(rig.conflicts().len(), 1, "ours is kept beside it");
    assert!(rig.attached.engine.state.pendings().unwrap().is_empty());
}
