use super::*;

fn status(online: bool, updated: i64) -> daemon::Status {
    daemon::Status { online, updated, ..Default::default() }
}

#[test]
fn a_client_that_could_not_start_says_why_for_as_long_as_it_is_down() {
    let stopped = daemon::Status {
        fatal: "no device token found; run `fafnirx install --server <address>`".into(),
        updated: 1,
        ..Default::default()
    };
    // Long after it exited: still the reason, not a bare "not running".
    let v = view(Some(&stopped), false, 1_000_000);
    assert_eq!(
        (v.light, v.text.as_str()),
        (Light::Error, "Stopped: no device token found; run `fafnirx install --server <address>`")
    );
    let info = describe(Some(&stopped), &v, 1_000_000);
    assert!(info.now.starts_with("The client stopped: no device token"), "{}", info.now);
    assert!(!info.error.is_empty());
}

#[test]
fn view_reflects_state() {
    let now = 1_000;
    assert_eq!(view(None, false, now).light, Light::Stopped);
    assert_eq!(view(Some(&status(true, now)), false, now).light, Light::Ok);
    assert_eq!(
        view(Some(&status(true, now - 500)), false, now).light,
        Light::Stopped,
        "stale status means not running"
    );
    let v = view(Some(&daemon::Status { message: "tls".into(), ..status(false, now) }), false, now);
    assert_eq!((v.light, v.text.as_str()), (Light::Error, "Error: tls"));
    assert_eq!(view(Some(&status(true, now)), true, now).light, Light::Paused);
}

#[test]
fn only_a_running_online_client_blinks() {
    let now = 1_000;
    let syncing = daemon::Status { syncing: true, ..status(true, now) };
    let v = view(Some(&syncing), false, now);
    assert_eq!((v.light, v.syncing, v.text.as_str()), (Light::Ok, true, "Syncing"));
    assert!(!view(Some(&syncing), true, now).syncing, "paused does not blink");
    assert!(
        !view(Some(&daemon::Status { syncing: true, ..status(true, now - 500) }), false, now).syncing,
        "a dead daemon does not blink"
    );
    assert!(
        !view(Some(&daemon::Status { syncing: true, ..status(false, now) }), false, now).syncing,
        "offline does not blink"
    );
}

fn syncing_status() -> daemon::Status {
    daemon::Status {
        version: "0.1.0".into(),
        server: "https://nas:7443".into(),
        device: "laptop".into(),
        online: true,
        syncing: true,
        updated: 1_000,
        started: 1_000 - 4_000,
        last_cycle: 940,
        last: crate::sync::Report { uploaded: 3, bytes: 2000, ..Default::default() },
        folders: 2,
        synced_files: 1,
        synced_bytes: 6_000_000,
        session_files: 5,
        session_bytes: 3_000_000_000,
        activity: daemon::Activity {
            phase: "uploading".into(),
            current: "docs/big.bin".into(),
            done: 1,
            total: 4,
            bytes: 5_000_000,
            total_bytes: 20_000_000,
            done_bytes: 5_000_000,
            speed_bps: 2_000_000,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn menu_text_describes_a_running_sync() {
    let s = syncing_status();
    let i = describe(Some(&s), &view(Some(&s), false, 1_000), 1_000);
    assert_eq!(i.title, "fafnirx 0.1.0: Syncing 25%, 2.0 MB/s, 7 s left");
    assert_eq!(i.server, "Server: https://nas:7443 (connected)");
    assert_eq!(i.device, "Device: laptop");
    assert_eq!(i.now, "Uploading 2/4, 25%, 2.0 MB/s, 7 s left: docs/big.bin");
    assert_eq!(i.last_sync, "Last sync 1 min ago: 3 uploaded, 0 renamed, 0 deleted, 0 failed (2.0 KB)");
    assert_eq!(i.on_server, "On the server: 1 file, 6.0 MB from 2 folders");
    assert_eq!(i.session, "Since start: 5 files sent (3.0 GB), running 1 h");
    assert_eq!(i.error, "No errors");
}

#[test]
fn a_new_install_says_that_the_first_sync_sends_all_files() {
    let mut s = syncing_status();
    s.initial_sync = true;
    let i = describe(Some(&s), &view(Some(&s), false, 1_000), 1_000);
    assert!(i.now.starts_with("First sync of all files: uploading 2/4"), "{}", i.now);
    assert!(i.on_server.ends_with("(the first sync sends all of them)"), "{}", i.on_server);
    s.syncing = false;
    s.activity = daemon::Activity::default();
    assert_eq!(
        describe(Some(&s), &view(Some(&s), false, 1_000), 1_000).now,
        "First sync of all files has not finished"
    );
}

#[test]
fn menu_text_for_idle_paused_offline_and_stopped() {
    let mut s = syncing_status();
    s.syncing = false;
    s.activity = daemon::Activity::default();
    s.next_scan = 1_000 + 600;
    assert_eq!(describe(Some(&s), &view(Some(&s), false, 1_000), 1_000).now, "Idle, next scan in 10 min");
    assert_eq!(describe(Some(&s), &view(Some(&s), true, 1_000), 1_000).now, "Paused: nothing is uploaded");

    s.online = false;
    s.message = "cannot reach the master: tcp open error".into();
    let i = describe(Some(&s), &view(Some(&s), false, 1_000), 1_000);
    assert!(i.server.ends_with("(unreachable)") && i.error.starts_with("Problem: cannot reach"), "{i:?}");
    assert_eq!(i.now, "Waiting for the master");

    let i = describe(None, &view(None, false, 1_000), 1_000);
    assert_eq!((i.title.as_str(), i.now.as_str()), ("fafnirx: Not running", "The client is not running"));

    s.online = true;
    s.message.clear();
    s.last_error = "x".repeat(300);
    assert!(describe(Some(&s), &view(Some(&s), false, 1_000), 1_000).error.chars().count() <= MAX_LINE);
}

#[test]
fn sizes_and_spans() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(999), "999 B");
    assert_eq!(human_bytes(1500), "1.5 KB");
    assert_eq!(human_bytes(5_000_000), "5.0 MB");
    assert_eq!(
        [span(5), span(59), span(60), span(7200), span(3 * 86400), span(-4)],
        ["5 s", "59 s", "1 min", "2 h", "3 d", "0 s"]
    );
}
