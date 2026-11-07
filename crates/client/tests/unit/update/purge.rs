use super::*;

#[test]
fn the_version_is_what_fafnirx_says() {
    assert_eq!(parse_version("fafnirx 3.13.0\n").as_deref(), Some("3.13.0"));
    assert_eq!(parse_version("fafnirx 3.13"), Some("3.13".into()));
    assert_eq!(parse_version("fafnirx"), None);
    assert_eq!(parse_version("rclone v1.75.2"), None, "another program");
    assert_eq!(parse_version("fafnirx-tool 3.0.0"), None);
    assert_eq!(parse_version("fafnirx beta"), None);
    assert_eq!(parse_version(""), None);
}

#[test]
fn only_older_versions_are_chosen() {
    let found = vec![
        (PathBuf::from("/a"), "3.12.9".to_string()),
        (PathBuf::from("/b"), "3.13.0".to_string()),
        (PathBuf::from("/c"), "3.14.0".to_string()),
        (PathBuf::from("/d"), "2.0".to_string()),
    ];
    let older: Vec<_> = older_than("3.13.0", found).into_iter().map(|(p, _)| p).collect();
    assert_eq!(older, [PathBuf::from("/a"), PathBuf::from("/d")], "the same and the newer ones stay");
}

#[test]
fn programs_are_found_once_and_the_running_one_is_not_one_of_them() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    for d in [&a, &b] {
        std::fs::create_dir_all(d).unwrap();
        std::fs::write(d.join(PROGRAM), b"x").unwrap();
    }
    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let dirs = vec![a.clone(), b.clone(), a.clone(), empty, dir.path().join("missing")];
    let found = programs_in(&dirs, &a.join(PROGRAM));
    assert_eq!(
        found,
        [fafnirx_common::plain_path(&std::fs::canonicalize(b.join(PROGRAM)).unwrap())],
        "not the running one, not twice"
    );
    assert_eq!(programs_in(&dirs, &dir.path().join("elsewhere")).len(), 2);
}

#[cfg(unix)]
#[test]
fn a_program_that_is_not_the_client_is_not_asked_twice_nor_trusted() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, body: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    let old = write("old", "#!/bin/sh\necho 'fafnirx 1.2.3'\n");
    let other = write("other", "#!/bin/sh\necho 'something 9.9.9'\n");
    let failing = write("failing", "#!/bin/sh\necho 'fafnirx 1.0.0'\nexit 1\n");
    assert_eq!(version_of(&old).as_deref(), Some("1.2.3"));
    assert_eq!(version_of(&other), None);
    assert_eq!(version_of(&failing), None);
    assert_eq!(version_of(&dir.path().join("missing")), None);
}

#[cfg(unix)]
#[test]
fn an_older_copy_is_removed_and_the_running_one_stays() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let make = |name: &str, version: &str| {
        std::fs::create_dir_all(dir.path().join(name)).unwrap();
        let path = dir.path().join(name).join(PROGRAM);
        std::fs::write(&path, format!("#!/bin/sh\necho 'fafnirx {version}'\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    let (old, newer_copy, current) = (make("old", "0.0.1"), make("newer", "99.0.0"), make("current", super::super::VERSION));
    // The same steps as `announce`, with the folders given.
    let folders: Vec<_> = ["old", "newer", "current"].iter().map(|n| dir.path().join(n)).collect();
    let found: Vec<_> = programs_in(&folders, &current).into_iter().filter_map(|p| version_of(&p).map(|v| (p, v))).collect();
    let older = older_than(super::super::VERSION, found);
    assert_eq!(older.len(), 1);
    remove(&older[0].0).unwrap();
    assert!(!old.exists() && newer_copy.exists() && current.exists());
}
