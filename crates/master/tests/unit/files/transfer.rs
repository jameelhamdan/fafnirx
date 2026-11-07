use super::*;

#[test]
fn ranges() {
    assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
    assert_eq!(parse_range("bytes=900-", 1000), Some((900, 999)));
    assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
    assert_eq!(parse_range("bytes=0-5000", 1000), Some((0, 999)));
    assert_eq!(parse_range("bytes=1000-", 1000), None);
    assert_eq!(parse_range("bytes=0-1,5-6", 1000), None);
    assert_eq!(parse_range("items=0-1", 1000), None);
    assert_eq!(parse_range("bytes=0-1", 0), None);
}

#[test]
fn names_are_safe_in_headers() {
    assert_eq!(percent_encode("my photo é.jpg"), "my%20photo%20%C3%A9.jpg");
    assert!(shows_inline("image/png") && shows_inline("application/pdf"));
    assert!(!shows_inline("image/svg+xml") && !shows_inline("text/html"));
}
