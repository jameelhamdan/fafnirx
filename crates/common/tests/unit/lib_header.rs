use super::*;

#[test]
fn a_key_survives_the_header() {
    for key in ["family", "bob/work", "master:Música/Rock & Roll", "master:100%"] {
        let sent = header_encode(key);
        assert!(sent.is_ascii());
        assert_eq!(header_decode(&sent), key);
    }
}
