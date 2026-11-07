use super::*;

#[test]
fn an_id_is_22_url_safe_characters_and_round_trips() {
    let id = Id(Uuid::from_u128(0xfbf0_0000_0000_4000_8000_0000_0000_ffff));
    let text = id.to_string();
    assert_eq!(text.len(), 22);
    assert!(text.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    assert_eq!(text.parse::<Id>().unwrap(), id);
    assert_eq!(id.0.to_string().parse::<Id>().unwrap(), id, "the hyphenated form is accepted");
    assert!("1".parse::<Id>().is_err(), "a counter is not an id");
    assert!("!!!!!!!!!!!!!!!!!!!!!!".parse::<Id>().is_err());
}

#[test]
fn json_carries_the_short_form() {
    let id = Id(Uuid::from_u128(7));
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{id}\""));
    assert_eq!(serde_json::from_str::<Id>(&json).unwrap(), id);
}
