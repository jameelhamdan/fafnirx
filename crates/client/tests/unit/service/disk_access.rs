use super::*;

#[test]
fn the_folders_macos_protects() {
    for p in [
        "/Users/jameel/Documents/docs",
        "/Users/jameel/Desktop",
        "/Users/jameel/downloads/x",
        "/Users/jameel/Library/Mobile Documents/com~apple~CloudDocs/a",
        "/Volumes/Backup/photos",
    ] {
        assert!(is_protected(Path::new(p)), "{p}");
    }
    for p in ["/Users/jameel/projects", "/Users/jameel/Library/Caches", "/srv/files", "/Users/jameel", "/Volumes"] {
        assert!(!is_protected(Path::new(p)), "{p}");
    }
}

#[test]
fn only_a_missing_permission_blocks_a_protected_folder() {
    let docs = Path::new("/Users/a/Documents/docs");
    let other = Path::new("/Users/a/projects");
    assert_eq!(first_blocked(Some(false), [other, docs]), Some(docs));
    assert_eq!(first_blocked(Some(false), [other]), None, "a folder macOS does not protect needs nothing");
    assert_eq!(first_blocked(Some(true), [docs]), None, "allowed");
    assert_eq!(first_blocked(None, [docs]), None, "not macOS, or not known");
    assert!(blocked_text(docs).contains("Full Disk Access"));
}
