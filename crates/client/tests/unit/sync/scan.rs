use super::*;
use std::fs;

fn folder(dir: &Path) -> Folder {
    Folder::computer(dir.to_path_buf(), "docs".into())
}

#[test]
fn scans_with_ignore_file_and_nested_dirs() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("a/b")).unwrap();
    fs::write(d.path().join("a/b/c.txt"), "hi").unwrap();
    fs::write(d.path().join("keep.txt"), "x").unwrap();
    fs::write(d.path().join("skip.tmp"), "x").unwrap();
    fs::write(d.path().join(IGNORE_FILE), "*.tmp\n").unwrap();
    let scan = scan_folder(&folder(d.path()), None).unwrap();
    let remotes: Vec<_> = scan.entries.iter().map(|e| e.remote.as_str()).collect();
    assert_eq!(remotes, [format!("docs/{IGNORE_FILE}").as_str(), "docs/a/b/c.txt", "docs/keep.txt"]);
    assert_eq!(scan.entries[1].size, 2);
}

#[test]
fn paths_that_differ_only_by_case_keep_the_first() {
    let entry = |remote: &str| Entry { remote: remote.into(), local: remote.into(), size: 0, mtime_ns: 0, mode: 0 };
    let mut entries = vec![entry("docs/A/x.txt"), entry("docs/a/x.txt"), entry("docs/b.txt"), entry("docs/B.TXT")];
    let mut skipped = Vec::new();
    drop_case_conflicts(&mut entries, &mut skipped);
    let kept: Vec<_> = entries.iter().map(|e| e.remote.as_str()).collect();
    assert_eq!(kept, ["docs/A/x.txt", "docs/b.txt"], "directories that differ only by case conflict too");
    assert_eq!(skipped.len(), 2);
}

#[test]
fn missing_folder_is_an_error() {
    let d = tempfile::tempdir().unwrap();
    assert!(scan_folder(&folder(&d.path().join("gone")), None).is_err());
}

/// Such names exist only on Linux and macOS, so only there can they be made for the test.
#[cfg(unix)]
#[test]
fn a_synced_folder_skips_the_names_windows_cannot_keep_and_a_backup_does_not() {
    let d = tempfile::tempdir().unwrap();
    for name in ["fine.txt", "a:b.txt", "CON.txt"] {
        fs::write(d.path().join(name), "x").unwrap();
    }
    let names = |name: &str| -> Vec<String> {
        let scan = scan_folder(&Folder::computer(d.path().to_path_buf(), name.into()), None).unwrap();
        scan.entries.iter().map(|e| e.remote.clone()).collect()
    };
    assert_eq!(
        names("docs"),
        ["docs/CON.txt", "docs/a:b.txt", "docs/fine.txt"],
        "a backup keeps everything the computer has"
    );
    assert_eq!(names(""), ["fine.txt"], "a synced folder only what every kind of computer can hold");
}

#[test]
fn names_are_nfc() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("e\u{301}.txt"), "x").unwrap();
    let scan = scan_folder(&folder(d.path()), None).unwrap();
    assert_eq!(scan.entries[0].remote, "docs/\u{e9}.txt");
}
