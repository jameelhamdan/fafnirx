//! How an id is written outside the database. Files, albums, tags, jobs and alerts are UUIDs in PostgreSQL; the API, the
//! URLs and the web app show them as 22 characters of URL-safe Base64 without padding (`B5v1k9yXQ2eQm0GmXH3xwg`), which
//! is the same 16 bytes in a shorter, prettier form. The hyphenated UUID is accepted too.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::de::{self, Deserialize, Deserializer};
use serde::{Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Id(pub Uuid);

impl From<Uuid> for Id {
    fn from(u: Uuid) -> Self {
        Id(u)
    }
}

impl From<Id> for Uuid {
    fn from(id: Id) -> Self {
        id.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&URL_SAFE_NO_PAD.encode(self.0.as_bytes()))
    }
}

impl FromStr for Id {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 22 {
            let bytes = URL_SAFE_NO_PAD.decode(s).map_err(|_| "not a valid id")?;
            return Uuid::from_slice(&bytes).map(Id).map_err(|_| "not a valid id");
        }
        Uuid::parse_str(s).map(Id).map_err(|_| "not a valid id")
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(de::Error::custom)
    }
}

/// The UUIDs of a list of ids.
pub fn uuids(ids: &[Id]) -> Vec<Uuid> {
    ids.iter().map(|i| i.0).collect()
}

#[cfg(test)]
mod tests {
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
}
