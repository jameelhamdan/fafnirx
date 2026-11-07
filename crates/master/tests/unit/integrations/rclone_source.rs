use super::*;

#[test]
fn a_listing_becomes_items_and_skips_what_cannot_be_kept() {
    let json = r#"[
            {"Path":"a/b.txt","Name":"b.txt","Size":12,"ModTime":"2024-05-01T10:00:00.5Z","IsDir":false},
            {"Path":"a","Name":"a","Size":-1,"ModTime":"2024-05-01T10:00:00Z","IsDir":true},
            {"Path":"unknown","Name":"unknown","Size":-1,"ModTime":"2024-05-01T10:00:00Z","IsDir":false},
            {"Path":"../escape","Name":"escape","Size":1,"ModTime":"2024-05-01T10:00:00Z","IsDir":false}
        ]"#;
    let (items, skipped) = items_of(json).unwrap();
    assert_eq!(skipped, 2, "an unknown size and a path that leaves the folder");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].path, "a/b.txt");
    assert_eq!(items[0].marker, "12:1714557600500000000");
    assert!(items_of("not json").is_err());
}

#[test]
fn rfc3339_to_unix_nanoseconds() {
    assert_eq!(parse_time_ns("1970-01-01T00:00:01.5Z"), 1_500_000_000);
    assert_eq!(parse_time_ns("garbage"), 0);
}
