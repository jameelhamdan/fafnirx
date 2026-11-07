use super::*;

#[test]
fn passwords_verify_only_against_their_own_hash() {
    let hash = hash_password("correct horse").unwrap();
    assert!(verify_password(&hash, "correct horse"));
    assert!(!verify_password(&hash, "correct horse "));
    assert!(!verify_password("!", "!"), "a user without a password cannot log in");
    assert!(!verify_password("garbage", "garbage"));
    assert_ne!(hash, hash_password("correct horse").unwrap(), "every hash has its own salt");
}

#[test]
fn only_a_well_formed_cookie_is_taken_for_a_token() {
    let mut headers = HeaderMap::new();
    let token = "ab".repeat(32);
    headers.insert(header::COOKIE, HeaderValue::from_str(&format!("x=1; {COOKIE}={token}")).unwrap());
    assert_eq!(cookie_token(&headers).as_deref(), Some(token.as_str()));
    headers.insert(header::COOKIE, HeaderValue::from_str(&format!("{COOKIE}=short")).unwrap());
    assert_eq!(cookie_token(&headers), None);
}
