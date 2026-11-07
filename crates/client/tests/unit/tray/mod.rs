#[test]
fn a_tray_starts_over_only_when_the_marker_is_newer_than_it() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("restart-tray");
    let before = std::time::SystemTime::now();
    assert!(!super::asked_to_restart(&marker, before), "no marker, nothing to do");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&marker, b"").unwrap();
    assert!(super::asked_to_restart(&marker, before), "asked after this tray started");
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert!(
        !super::asked_to_restart(&marker, std::time::SystemTime::now()),
        "the new tray is younger than the marker"
    );
}

use super::*;

#[test]
fn a_synced_folder_is_shown_with_its_owner() {
    assert_eq!(with_owner("ana", "family"), "ana/family");
    assert_eq!(with_owner("ana", "bo/trip"), "bo/trip", "already named in full");
    assert_eq!(with_owner("", "family"), "family", "before the master has said who the user is");
}

#[test]
fn the_owner_is_the_user_part_of_the_device_name() {
    let status = |device: &str| daemon::Status { device: device.into(), ..Default::default() };
    assert_eq!(owner_of(Some(&status("ana/laptop"))), "ana");
    assert_eq!(owner_of(Some(&status(""))), "");
    assert_eq!(owner_of(None), "");
}
