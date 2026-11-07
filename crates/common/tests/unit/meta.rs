use super::*;

#[test]
fn times_are_unix_seconds() {
    assert_eq!(parse_time("2023-11-14 22:13:20"), Some(1_700_000_000));
    assert_eq!(parse_time("2023-11-14T22:13:20.000000Z"), Some(1_700_000_000));
    assert_eq!(parse_time("1970-01-01 00:00:00"), Some(0));
    assert_eq!(parse_time("2024-02-29 12:00:00"), Some(1_709_208_000));
    assert_eq!(parse_time("not a time"), None);
    assert_eq!(parse_time("2023-13-14 22:13:20"), None);
    assert_eq!(parse_time("0000:00:00 00:00:00"), None);
    assert_eq!(parse_time("2023:11:14 22:13:20"), Some(1_700_000_000), "as EXIF stores it");
    assert_eq!(parse_time("2023:11:14 22:13"), Some(1_699_999_980), "no seconds");
    assert_eq!(parse_time("2023:11:14"), Some(1_699_920_000), "only a day");
    assert_eq!(
        parse_time("2023-11-14T23:13:20+0100"),
        Some(1_700_003_600),
        "the wall clock, the offset is not applied"
    );
    assert_eq!(parse_time("2023:11"), None);
    assert_eq!(parse_time("2023-11-14 22"), None);
}
