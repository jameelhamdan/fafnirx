use super::caller::{lease_secs, UNNAMED_INSTANCE};
use super::*;
use crate::platform::db::testing::{db_with_device, open_alerts};
use crate::platform::db::Device;
use crate::repo::devices::{AddDevice, DeviceKind};

/// The device a call acts on, as the tests ask for it.
fn acting(c: &mut Client, who: &Caller, write: bool) -> Result<Device, Status> {
    Ok(place(c, who, write)?.device)
}

fn caller(instance: &str) -> Caller {
    Caller { token: "secret".into(), instance: instance.into(), ip: Some("10.0.0.9".into()), folder: None }
}

/// What a computer says about a shared folder.
fn said(name: &str, files: i64, held: i64, error: &str) -> devices::FolderState {
    devices::FolderState {
        name: name.into(),
        files,
        bytes: files * 10,
        last_cycle: 100,
        last_error: error.into(),
        held,
        failing: 0,
        mode: "both".into(),
        updated_at: 0,
    }
}

fn add_user(c: &mut Client, name: &str) -> i64 {
    let insert = "INSERT INTO users (name, role, password_hash, created_at) VALUES ($1, 'normal', '!', 0) RETURNING id";
    c.query_one(insert, &[&name]).unwrap().get(0)
}

#[test]
fn a_shared_folder_is_its_owners_until_the_owner_adds_someone() {
    let db = setup();
    let mut conn = db.get().unwrap();
    let c: &mut Client = &mut conn;
    let (_, owner) = authenticate_user(c, &caller("first")).unwrap();
    let folder = devices::create_folder(c, owner, "family", 0).unwrap().expect("made");
    let on = |who: Caller, key: &str| Caller { folder: Some(key.into()), ..who };

    let found = acting(c, &on(caller("first"), "family"), true).expect("the owner needs no setup");
    assert_eq!(found.name, folder);
    let by_folder = acting(c, &on(caller("first"), &folder), true).expect("and can name it in full too");
    assert_eq!(by_folder.id, found.id);
    assert_eq!(acting(c, &on(caller("first"), "nope"), false).unwrap_err().code(), tonic::Code::NotFound);

    // Another user, with a device of their own.
    let bob = add_user(c, "bob");
    devices::insert(
        c,
        &AddDevice {
            user_id: bob,
            name: "phone",
            kind: DeviceKind::Device,
            token_hash: &db::hash_token("bobtoken"),
            now: 0,
        },
    )
    .unwrap();
    let bobs = Caller { token: "bobtoken".into(), instance: "b".into(), ip: None, folder: None };
    for key in [folder.as_str(), "family"] {
        let refused = acting(c, &on(bobs.clone(), key), false);
        assert_eq!(refused.unwrap_err().code(), tonic::Code::NotFound, "{key} is not shared with bob");
    }
    assert!(devices::folders(c, bob).unwrap().is_empty());

    // The owner adds bob to read.
    c.execute(
        "INSERT INTO shared_members (folder_id, user_id, can_write, created_at) VALUES ($1, $2, FALSE, 0)",
        &[&found.id, &bob],
    )
    .unwrap();
    let theirs = devices::folders(c, bob).unwrap();
    assert_eq!(theirs.iter().map(|s| s.folder.as_str()).collect::<Vec<_>>(), [folder.as_str()]);
    assert!(!theirs[0].mine, "it is not bob's own");
    assert_eq!(acting(c, &on(bobs.clone(), &folder), false).unwrap().id, found.id);
    let read_only = acting(c, &on(bobs.clone(), &folder), true).unwrap_err();
    assert_eq!(read_only.code(), tonic::Code::FailedPrecondition);
    assert_eq!(
        acting(c, &on(bobs.clone(), "family"), false).unwrap_err().code(),
        tonic::Code::NotFound,
        "bob's own namespace has no \"family\""
    );

    c.execute("UPDATE shared_members SET can_write = TRUE", &[]).unwrap();
    acting(c, &on(bobs.clone(), &folder), true).expect("now bob may change it");

    // A read-only account never writes, whatever it was given before it was demoted.
    c.execute("UPDATE users SET role = 'readonly' WHERE id = $1", &[&bob]).unwrap();
    assert_eq!(acting(c, &on(bobs.clone(), &folder), true).unwrap_err().code(), tonic::Code::FailedPrecondition);
    acting(c, &on(bobs.clone(), &folder), false).expect("but still reads");

    // And nobody is let into the folder of an owner who was disabled.
    c.execute("UPDATE users SET disabled_at = 1 WHERE id = $1", &[&owner]).unwrap();
    assert_eq!(acting(c, &on(bobs.clone(), &folder), false).unwrap_err().code(), tonic::Code::NotFound);
}

#[test]
fn a_master_folder_is_a_folder_that_every_user_may_use() {
    let db = setup();
    let mut conn = db.get().unwrap();
    let c: &mut Client = &mut conn;
    let (_, owner) = authenticate_user(c, &caller("first")).unwrap();
    let on = |key: &str| Caller { folder: Some(key.into()), ..caller("first") };
    assert_eq!(
        place(c, &on("master:Music"), false).err().map(|e| Status::from(e).code()),
        Some(tonic::Code::NotFound),
        "none made yet"
    );

    let path = super::super::master_folders::check(c, "/Music/", None).unwrap();
    assert_eq!(path, "Music");
    let folder = devices::insert_master_folder(c, &path, 1).unwrap().expect("made");
    assert_eq!(folder, "master:Music");
    let at = place(c, &on("master:Music"), true).unwrap();
    assert_eq!((at.device.name.as_str(), at.folder), ("master:Music", true));
    assert_eq!(
        place(c, &on("master:music"), false).err().map(|e| Status::from(e).code()),
        Some(tonic::Code::NotFound),
        "exact name"
    );
    assert_eq!(devices::folders(c, owner).unwrap().iter().filter(|f| f.master).count(), 1);

    // Neither inside another, nor around one, nor in the folders of the users, nor a name Windows cannot keep.
    for bad in ["Music/Rock", "music", "users/x", "A:b", "Users"] {
        assert!(super::super::master_folders::check(c, bad, None).is_err(), "{bad}");
    }
    assert!(super::super::master_folders::check(c, "Photos", None).is_ok());

    // A local folder of the user is a target too, theirs alone.
    devices::insert(c, &AddDevice { user_id: owner, name: "docs", kind: DeviceKind::Local, token_hash: "-", now: 0 }).unwrap();
    let own: Vec<_> = devices::folders(c, owner).unwrap().into_iter().filter(|f| f.mine).collect();
    assert_eq!(own.len(), 1, "the device of the test is a computer, not a folder");
    assert_eq!(place(c, &on(&own[0].folder), true).unwrap().device.id, own[0].id);

    // A read-only account reads it and never writes.
    c.execute("UPDATE users SET role = 'readonly' WHERE id = $1", &[&owner]).unwrap();
    assert_eq!(
        place(c, &on("master:Music"), true).err().map(|e| Status::from(e).code()),
        Some(tonic::Code::FailedPrecondition)
    );
    place(c, &on("master:Music"), false).expect("but still reads");
}

#[test]
fn a_computer_that_may_have_missed_deletions_is_sent_the_whole_list() {
    let db = setup();
    let mut conn = db.get().unwrap();
    let c: &mut Client = &mut conn;
    assert!(deletions_may_be_missing(c, 0), "asking from the start");
    assert!(!deletions_may_be_missing(c, 1_000), "retention has removed nothing yet");
    settings::set(c, settings::DELETIONS_KNOWN_FROM, "5000").unwrap();
    assert!(deletions_may_be_missing(c, 4_999), "away since before deletions were removed");
    assert!(!deletions_may_be_missing(c, 5_000), "away since the time they are all known from");
    assert!(!deletions_may_be_missing(c, 9_000));
}

#[test]
fn what_a_computer_says_about_its_shared_folders_replaces_what_it_said_before() {
    let db = setup();
    let mut conn = db.get().unwrap();
    let c: &mut Client = &mut conn;
    let device = authenticate(c, &caller("first")).unwrap().id;
    devices::save_folder_state(c, device, &[said("alice/family", 3, 0, ""), said("alice/work", 1, 0, "boom")], 500).unwrap();
    let first = devices::folder_states_of_device(c, device).unwrap();
    assert_eq!(first.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["alice/family", "alice/work"]);
    assert_eq!((first[0].files, first[1].last_error.as_str(), first[0].updated_at), (3, "boom", 500));
    assert_eq!(first[0].mode, "both", "the mode the computer keeps the folder in is stored");
    // The next beat names only one: the other is no longer attached.
    devices::save_folder_state(c, device, &[said("alice/family", 4, 0, "")], 600).unwrap();
    let second = devices::folder_states_of_device(c, device).unwrap();
    assert_eq!((second.len(), second[0].files), (1, 4));
    // The owner of a folder sees every computer with it attached, another user only their own.
    let owner: i64 = authenticate_user(c, &caller("first")).unwrap().1;
    let seen = devices::folder_states_of_folder(c, "alice/family", true, -1).unwrap();
    assert_eq!((seen.len(), seen[0].mode.as_str()), (1, "both"));
    assert_eq!(devices::folder_states_of_folder(c, "alice/family", false, owner).unwrap().len(), 1);
    assert!(devices::folder_states_of_folder(c, "alice/family", false, owner + 1).unwrap().is_empty());
    devices::save_folder_state(c, device, &[], 700).unwrap();
    assert!(
        devices::folder_states_of_device(c, device).unwrap().is_empty(),
        "a computer with none attached says nothing"
    );
}

#[test]
fn what_a_user_sees_of_the_folders_is_one_rule_of_the_database() {
    let db = setup();
    let mut conn = db.get().unwrap();
    let c: &mut Client = &mut conn;
    let (_, owner) = authenticate_user(c, &caller("first")).unwrap();
    let family = devices::create_folder(c, owner, "family", 0).unwrap().expect("made");
    let (member, stranger) = (add_user(c, "member"), add_user(c, "stranger"));
    let folder_id: i64 = c.query_one("SELECT id FROM devices WHERE folder = $1", &[&family]).unwrap().get(0);
    devices::add_shared_member(c, folder_id, member, false, 0).unwrap();
    let sees = |c: &mut Client, which: &str, uid: i64, everything: bool| -> bool {
        c.query_one(
            "SELECT visible(d, $2, $3) FROM devices d WHERE d.folder = $1 OR d.kind = $1",
            &[&which, &uid, &everything],
        )
        .unwrap()
        .get(0)
    };
    let own_device = authenticate(c, &caller("first")).unwrap().name;
    let master = devices::insert_master_folder(c, "Photos", 0).unwrap().expect("made");
    for (who, uid, everything, family_seen, device_seen) in [
        ("its owner", owner, false, true, true),
        ("a member", member, false, true, false),
        ("a stranger", stranger, false, false, false),
        ("an administrator", stranger, true, true, true),
    ] {
        assert_eq!(sees(c, &family, uid, everything), family_seen, "{who} and the shared folder");
        assert_eq!(sees(c, &own_device, uid, everything), device_seen, "{who} and the owner's computer");
        assert!(sees(c, &master, uid, everything), "{who} and a master folder");
    }
    c.execute("UPDATE users SET disabled_at = 1 WHERE id = $1", &[&owner]).unwrap();
    assert!(!sees(c, &family, member, false), "a member loses the folder of a disabled owner");
}

#[test]
fn held_deletions_raise_an_alert_that_closes_when_they_are_dealt_with() {
    let db = setup();
    let mut conn = db.get().unwrap();
    let c: &mut Client = &mut conn;
    let open = |c: &mut Client| -> Vec<(String, String, String)> {
        alerts::open(c)
            .unwrap()
            .into_iter()
            .filter(|a| a.kind == "deletions_held")
            .map(|a| (a.kind, a.key, a.message))
            .collect()
    };
    alert_held_deletions(c, "laptop", &[said("alice/family", 20, 0, "")]).unwrap();
    assert!(open(c).is_empty(), "nothing held, nothing to say");
    alert_held_deletions(c, "laptop", &[said("alice/family", 20, 12, ""), said("alice/work", 20, 3, "")]).unwrap();
    let raised = open(c);
    assert_eq!(raised.len(), 1, "one alert for the computer");
    assert_eq!(raised[0].1, "laptop");
    assert!(raised[0].2.contains("alice/family (12 files)") && raised[0].2.contains("alice/work (3 files)"));
    alert_held_deletions(c, "laptop", &[said("alice/family", 20, 12, "")]).unwrap();
    assert_eq!(open(c).len(), 1, "still one, updated");
    alert_held_deletions(c, "laptop", &[said("alice/family", 20, 0, "")]).unwrap();
    assert!(open(c).is_empty(), "closed once nothing is held");
}

#[test]
fn a_shared_folder_lists_what_was_written_and_deleted_since() {
    let db = setup();
    let mut conn = db.get().unwrap();
    let c: &mut Client = &mut conn;
    let (_, owner) = authenticate_user(c, &caller("first")).unwrap();
    devices::create_folder(c, owner, "family", 0).unwrap();
    let id = devices::folders(c, owner).unwrap()[0].id;

    let root = [7u8; 32];
    for (path, at) in [("a.txt", 100), ("b.txt", 200)] {
        let file = files::ensure(c, id, path).unwrap();
        let v = files::insert_version(c, &files::NewVersion { file_id: file, root: &root, size: 3, mtime: 5, mode: 0o644, created_at: at }).unwrap();
        files::make_current(c, file, v).unwrap();
    }
    let paths = |c: &mut Client, since| -> Vec<(String, bool)> {
        files::changes_since(c, id, since)
            .unwrap()
            .into_iter()
            .map(|x| (x.path, x.root.is_some()))
            .collect()
    };
    assert_eq!(paths(c, 0), [("a.txt".into(), true), ("b.txt".into(), true)], "everything from the start");
    assert_eq!(paths(c, 150), [("b.txt".into(), true)], "only what is newer");
    assert!(paths(c, 300).is_empty());

    files::delete_path(c, id, "a.txt", 400, false).unwrap();
    assert_eq!(paths(c, 300), [("a.txt".into(), false)], "a deletion is a change, without content");
    let all = files::changes_since(c, id, 0).unwrap();
    let b = all.iter().find(|x| x.path == "b.txt").unwrap();
    assert_eq!((b.root, b.size, b.mtime, b.mode), (Some(root), 3, 5, 0o644));
}

/// A database with a device whose token is "secret".
fn setup() -> Db {
    let db = db_with_device();
    db.get()
        .unwrap()
        .execute("UPDATE devices SET token_hash = $1", &[&db::hash_token("secret")])
        .unwrap();
    db
}

#[test]
fn only_one_computer_uses_a_device_at_a_time() {
    let db = setup();
    let mut c = db.get().unwrap();
    authenticate(&mut c, &caller("first")).expect("the first to connect holds the device");
    authenticate(&mut c, &caller("first")).expect("and keeps it");
    let refused = authenticate(&mut c, &caller("second")).expect_err("a second computer is refused");
    let refused = Status::from(refused);
    assert_eq!(refused.code(), tonic::Code::PermissionDenied);
    assert!(refused.message().starts_with("device in use"), "{}", refused.message());
    assert_eq!(open_alerts(&mut c), 1, "and the owner is told");
    authenticate(&mut c, &caller("second")).expect_err("still refused");
    assert_eq!(open_alerts(&mut c), 1, "once, not once per attempt");
    authenticate(&mut c, &caller("first")).expect("the first is not disturbed");
    let wrong = Caller { token: "other".into(), ..caller("first") };
    assert_eq!(Status::from(authenticate(&mut c, &wrong).unwrap_err()).code(), tonic::Code::Unauthenticated);
}

#[test]
fn a_computer_that_went_quiet_is_replaced() {
    let db = setup();
    let mut c = db.get().unwrap();
    authenticate(&mut c, &caller("old")).unwrap();
    authenticate(&mut c, &caller("new")).unwrap_err();
    // The first has not been heard from for longer than the lease.
    c.execute("UPDATE devices SET last_seen = $1", &[&(unix_now() - lease_secs() - 5)]).unwrap();
    authenticate(&mut c, &caller("new")).expect("the new computer takes the device over");
    assert_eq!(open_alerts(&mut c), 0, "the conflict is over");
    let back = authenticate(&mut c, &caller("old")).expect_err("and the old one, when it returns, is the one refused");
    assert_eq!(Status::from(back).code(), tonic::Code::PermissionDenied);
}

#[test]
fn a_released_device_goes_to_the_next_computer() {
    let db = setup();
    let mut c = db.get().unwrap();
    authenticate(&mut c, &caller("old")).unwrap();
    c.execute("UPDATE devices SET instance_id = NULL", &[]).unwrap();
    authenticate(&mut c, &caller("new")).expect("whoever connects next holds it");
    authenticate(&mut c, &caller("old")).expect_err("the old one is refused now");
}

#[test]
fn callers_that_do_not_name_an_instance_count_as_one() {
    let db = setup();
    let mut c = db.get().unwrap();
    authenticate(&mut c, &caller(UNNAMED_INSTANCE)).unwrap();
    authenticate(&mut c, &caller(UNNAMED_INSTANCE)).unwrap();
    authenticate(&mut c, &caller("new")).expect_err("but a caller that names one does not share it");
}
