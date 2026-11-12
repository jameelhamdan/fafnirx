use super::*;

#[test]
fn names_are_trimmed_lowered_and_checked() {
    assert_eq!(name("  Family ").unwrap(), "family");
    for bad in ["", "a b", "con", "-x", &"a".repeat(64)] {
        assert!(name(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn paths_lose_the_slashes_at_their_ends_and_a_prefix_gets_one() {
    assert_eq!(folder_path("/Shows//").unwrap(), "Shows");
    assert_eq!(folder_path(" /Shows/Pics/ ").unwrap(), "Shows/Pics");
    assert_eq!((folder_path("/").unwrap(), folder_prefix("").unwrap()), (String::new(), String::new()));
    assert_eq!(folder_prefix("a/b").unwrap(), "a/b/");
    assert!(folder_path("a/../b").is_err() && path_inside("/").is_err());
    assert_eq!(path_inside("a").unwrap(), "a");
}
