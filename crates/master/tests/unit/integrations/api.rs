use super::*;

#[test]
fn options_are_key_value_lines() {
    let opts = parse_options("# comment\nProvider = AWS\naccess-key-id=abc=def\n\n").unwrap();
    assert_eq!(opts["provider"], "AWS");
    assert_eq!(opts["access_key_id"], "abc=def");
    assert!(parse_options("novalue").is_err());
    assert!(parse_options("bad key=1").is_err());
}

#[test]
fn intervals_are_limited() {
    assert_eq!(hours_to_secs(1.5).unwrap(), 5400);
    assert!(hours_to_secs(0.1).is_err() && hours_to_secs(24.0 * 400.0).is_err());
}
