use super::*;

fn spec() -> MirrorSpec {
    MirrorSpec {
        name: "m".into(),
        kind: MirrorKind::Replica,
        crypt_password: "ignored".into(),
        crypt_salt: String::new(),
        remote: String::new(),
        remote_type: "S3".into(),
        remote_path: " bucket/x ".into(),
        options: [("Access-Key".to_string(), Value::from(1))].into(),
        interval_hours: 72.0,
        folder: " /ana/laptop/ ".into(),
        keep_runs: 3,
    }
}

#[test]
fn remotes_are_a_name_and_a_path() {
    assert!(valid_remote("b2crypt:") && valid_remote("my remote:bucket/x"));
    assert!(!valid_remote("no-colon") && !valid_remote("-x:y") && !valid_remote("a:b\nc"));
}

#[test]
fn a_backend_spelled_out_is_normalized() {
    let mut m = spec();
    m.normalize().unwrap();
    assert_eq!((m.remote_type.as_str(), m.remote_path.as_str()), ("s3", "bucket/x"));
    assert_eq!(m.options["access_key"], Value::from("1"));
    assert_eq!(m.folder, "ana/laptop");
    let mut bad = spec();
    bad.keep_runs = 0;
    assert!(bad.normalize().is_err());
    let mut none = spec();
    none.remote_type.clear();
    assert!(none.normalize().is_err());
}

#[test]
fn a_backup_needs_a_key_and_a_replica_has_none() {
    let mut replica = spec();
    replica.normalize().unwrap();
    assert!(replica.crypt_password.is_empty(), "a replica is not encrypted");
    let mut backup = spec();
    backup.kind = MirrorKind::Backup;
    assert!(backup.normalize().is_err(), "no key");
    backup.crypt_password = "short".into();
    backup.crypt_salt = "another long salt".into();
    assert!(backup.normalize().is_err(), "too short");
    backup.crypt_password = "a long enough password".into();
    backup.normalize().unwrap();
    backup.crypt_salt = backup.crypt_password.clone();
    assert!(backup.normalize().is_err(), "salt equals password");
    assert_eq!(MirrorKind::parse("backup"), Some(MirrorKind::Backup));
    assert_eq!(MirrorKind::parse("snapshot"), None, "an unknown kind is not a replica");
    assert!(serde_json::from_str::<MirrorKind>("\"snapshot\"").is_err());
}

#[test]
fn a_named_remote_replaces_the_backend() {
    let mut m = spec();
    m.remote = "vault:fafnirx".into();
    m.normalize().unwrap();
    assert!(m.remote_type.is_empty() && m.options.is_empty());
}
