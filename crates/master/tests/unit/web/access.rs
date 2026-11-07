use super::*;

fn access(role: Role) -> Access {
    // What `folders_for_user` gives bob: a master folder, his own folder and a folder alice shared with him to read.
    let writes = role != Role::Readonly;
    Access {
        user_id: 7,
        name: "bob".into(),
        role,
        // (a read-only account has no folders of its own)
        folders: [("master:Photos", true), ("bob/laptop", writes), ("alice/family", false)]
            .into_iter()
            .filter(|(f, _)| writes || *f != "bob/laptop")
            .map(|(f, w)| (f.to_string(), w && writes))
            .collect(),
    }
}

#[test]
fn what_each_role_can_reach() {
    let (admin, normal, readonly) = (access(Role::Admin), access(Role::Normal), access(Role::Readonly));
    for (folder, seen_by, written_by) in [
        ("master:Photos", [true, true, true], [true, true, false]),
        ("bob/laptop", [true, true, false], [true, true, false]),
        ("alice/laptop", [true, false, false], [true, false, false]),
        ("bobby/laptop", [true, false, false], [true, false, false]),
        // Shared with bob, read-only: seen by him whatever his role, changed by nobody but an administrator.
        ("alice/family", [true, true, true], [true, false, false]),
    ] {
        let who = [&admin, &normal, &readonly];
        for i in 0..3 {
            assert_eq!(who[i].can_see(folder), seen_by[i], "{folder} seen by {:?}", who[i].role);
            assert_eq!(who[i].can_write(folder), written_by[i], "{folder} written by {:?}", who[i].role);
        }
    }
    assert!(!readonly.has_workspace() && normal.has_workspace() && admin.is_admin() && !normal.is_admin());

    // The owner can allow writing, to a normal account only.
    let mut allowed = access(Role::Normal);
    allowed.folders = vec![("alice/family".into(), true)];
    assert!(allowed.can_write("alice/family") && !allowed.can_write("alice/other"));
    let mut read_only_account = access(Role::Readonly);
    read_only_account.folders = vec![("alice/family".into(), true)];
    assert!(read_only_account.can_see("alice/family") && !read_only_account.can_write("alice/family"));
}
