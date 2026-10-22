use super::*;
use crate::platform::db::testing;
use serde_json::json;

const KEY: &str = r#"{"client_email":"sa@p.iam.gserviceaccount.com","private_key":"-----BEGIN PRIVATE KEY-----"}"#;

fn token_hash(token: &str) -> String {
    db::hash_token(token)
}

fn parse(value: Value) -> Config {
    Config::parse(&value.to_string()).unwrap()
}

fn error(value: Value) -> String {
    format!("{:#}", Config::parse(&value.to_string()).unwrap_err())
}

#[test]
fn a_configuration_is_read_in_its_normal_form() {
    let c = parse(json!({
        "devices": [
            { "name": "zeta", "token": "secret" },
            { "name": "alpha", "token_hash": token_hash("t").to_uppercase(), "revoked": true },
            { "name": "photos", "kind": "local", "token_hash": "ignored" },
        ],
        "mirrors": [
            { "name": "b", "remote_type": "S3", "remote_path": " bucket/x ", "options": { "Access-Key-Id": "k", "port": 22, "tls": true }, "interval_hours": 1.3333 },
            { "name": "a", "remote": "crypt:backups/" },
        ],
        "settings": { "retention_days": 10 },
    }));
    let devices = c.devices.unwrap();
    assert_eq!(
        devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["alpha", "photos", "zeta"],
        "sorted by name"
    );
    assert_eq!(devices[0].token_hash.as_deref(), Some(token_hash("t").as_str()), "hex is lowercase");
    assert_eq!(devices[2].token_hash, Some(token_hash("secret")), "a token written by hand becomes its hash");
    assert!(devices[2].token.is_none());
    assert!(devices[1].token_hash.is_none(), "a local folder has no token");

    let mirrors = c.mirrors.unwrap();
    assert_eq!(mirrors[0].name, "a");
    assert_eq!((mirrors[0].remote_type.as_str(), mirrors[0].remote.as_str()), ("", "crypt:backups/"));
    let b = &mirrors[1];
    assert_eq!((b.remote_type.as_str(), b.remote_path.as_str()), ("s3", "bucket/x"));
    assert_eq!(b.options["access_key_id"], "k");
    assert_eq!((b.options["port"].clone(), b.options["tls"].clone()), (json!("22"), json!("true")), "options are text");
    assert_eq!(b.interval_hours, 4800.0 / 3600.0, "the interval is whole seconds");
    assert_eq!((b.keep_runs, b.enabled), (3, true), "defaults");

    assert_eq!(c.settings.unwrap().retention_days, Some(10));
    assert!(c.sources.is_none(), "a section that is not there is not managed");
}

#[test]
fn mistakes_are_named() {
    assert!(error(json!({ "mirrorz": [] })).contains("mirrorz"), "a misspelled key is an error, not ignored");
    assert!(error(json!({ "version": 2 })).contains("version 2"));
    assert!(error(json!({ "devices": [{ "name": "Laptop", "token": "t" }] })).contains("devices[0]"));
    assert!(error(json!({ "devices": [{ "name": "a" }] })).contains("token_hash"));
    assert!(error(json!({ "devices": [{ "name": "a", "kind": "robot", "token": "t" }] })).contains("robot"));
    assert!(
        error(json!({ "devices": [{ "name": "a", "token": "t" }, { "name": "a", "token": "u" }] })).contains("twice")
    );
    assert!(error(json!({ "mirrors": [{ "name": "m" }] })).contains("remote_type"));
    assert!(error(json!({ "mirrors": [{ "name": "m", "remote": "-x:y" }] })).contains("name:path"));
    assert!(error(json!({ "mirrors": [{ "name": "m", "remote": "nocolon" }] })).contains("name:path"));
    assert!(error(json!({ "mirrors": [{ "name": "m", "remote_type": "s3;rm" }] })).contains("remote_type"));
    assert!(error(json!({ "mirrors": [{ "name": "m", "remote_type": "s3", "options": { "bad key": "x" } }] }))
        .contains("option"));
    assert!(error(json!({ "mirrors": [{ "name": "m", "remote_type": "s3", "keep_runs": 0 }] })).contains("keep_runs"));
    assert!(error(json!({ "mirrors": [{ "name": "m", "remote_type": "s3", "interval_hours": 0.01 }] }))
        .contains("Interval"));
    assert!(error(json!({ "settings": { "version_count": 0 } })).contains("version_count"));
    assert!(error(json!({ "sources": [{ "name": "s", "folders": ["short"], "service_account": serde_json::from_str::<Value>(KEY).unwrap() }] })).contains("folder"));
    assert!(Config::parse("{ nope").is_err());
}

#[test]
fn remotes_cannot_be_taken_for_options() {
    assert!(valid_remote("b2crypt:fafnirx/photos") && valid_remote("my remote:") && valid_remote("s3.eu:bucket"));
    assert!(
        !valid_remote("-o:x") && !valid_remote(":s3,key=v:bucket") && !valid_remote("a") && !valid_remote("a\nb:c")
    );
}

fn full() -> Config {
    parse(json!({
        "settings": { "version_count": 7, "retention_days": 14, "offline_after_secs": 90 },
        "devices": [
            { "name": "laptop", "token": "secret" },
            { "name": "old", "token": "x", "revoked": true },
            { "name": "photos", "kind": "local" },
        ],
        "mirrors": [
            { "name": "offsite", "remote_type": "s3", "remote_path": "bucket/fafnirx", "options": { "provider": "AWS" }, "interval_hours": 12, "keep_runs": 5 },
            { "name": "vault", "remote": "crypt:backups", "enabled": false },
        ],
        "sources": [
            { "name": "drive", "folders": ["1AbCdEfGhIjKlMnOp", "2AbCdEfGhIjKlMnOp"], "service_account": serde_json::from_str::<Value>(KEY).unwrap(), "interval_hours": 6 },
        ],
    }))
}

fn setup() -> (crate::platform::db::Db, tempfile::TempDir, Arc<Store>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    (testing::db(), dir, store)
}

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
}

#[test]
fn applying_then_exporting_gives_the_same_configuration() {
    let (db, _dir, store) = setup();
    let mut conn = db.get().unwrap();
    let c = full();
    database::apply(&mut conn, &store, &c).unwrap();
    assert_eq!(database::export(&mut conn).unwrap(), c);
    // Nothing to change the second time.
    database::apply(&mut conn, &store, &c).unwrap();
    assert_eq!(database::export(&mut conn).unwrap(), c);
    assert!(
        store.live_dir("laptop").is_dir() && store.live_dir("source-drive").is_dir(),
        "folders exist from the start"
    );
}

#[test]
fn what_the_file_does_not_list_is_removed_or_revoked_but_never_loses_files() {
    let (db, _dir, store) = setup();
    let mut conn = db.get().unwrap();
    database::apply(&mut conn, &store, &full()).unwrap();
    conn.execute(
        "INSERT INTO files (device_id, path) VALUES ((SELECT id FROM devices WHERE name = 'laptop'), 'a.txt')",
        &[],
    )
    .unwrap();
    let id: i64 = conn.query_one("SELECT id FROM mirrors WHERE name = 'offsite'", &[]).unwrap().get(0);
    conn.execute(
        "INSERT INTO mirror_items (mirror_id, device_id, path, root_hash) VALUES ($1, 1, 'a', '\\x00')",
        &[&id],
    )
    .unwrap();

    let smaller = parse(json!({ "devices": [{ "name": "photos", "kind": "local" }], "mirrors": [], "sources": [] }));
    database::apply(&mut conn, &store, &smaller).unwrap();

    let revoked: Vec<String> = conn
        .query("SELECT name FROM devices WHERE kind = 'device' AND revoked_at IS NOT NULL ORDER BY name", &[])
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(revoked, ["laptop", "old"], "devices that are no longer listed cannot send any more");
    let files: i64 = conn.query_one("SELECT COUNT(*) FROM files", &[]).unwrap().get(0);
    assert_eq!(files, 1, "their data stays");
    let mirrors: i64 = conn.query_one("SELECT COUNT(*) FROM mirrors", &[]).unwrap().get(0);
    let items: i64 = conn.query_one("SELECT COUNT(*) FROM mirror_items", &[]).unwrap().get(0);
    assert_eq!((mirrors, items), (0, 0));
    let sources: i64 = conn.query_one("SELECT COUNT(*) FROM sources", &[]).unwrap().get(0);
    assert_eq!(sources, 0);
}

#[test]
fn a_mirror_that_points_somewhere_else_sends_everything_again() {
    let (db, _dir, store) = setup();
    let mut conn = db.get().unwrap();
    database::apply(&mut conn, &store, &full()).unwrap();
    let id: i64 = conn.query_one("SELECT id FROM mirrors WHERE name = 'offsite'", &[]).unwrap().get(0);
    let items = |conn: &mut postgres::Client| -> i64 {
        conn.query_one("SELECT COUNT(*) FROM mirror_items", &[]).unwrap().get(0)
    };
    conn.execute(
        "INSERT INTO mirror_items (mirror_id, device_id, path, root_hash) VALUES ($1, 1, 'a', '\\x00')",
        &[&id],
    )
    .unwrap();

    let mut c = full();
    c.mirrors.as_mut().unwrap()[0].interval_hours = 1.0;
    database::apply(&mut conn, &store, &c).unwrap();
    assert_eq!(items(&mut conn), 1, "a new schedule keeps what was sent");

    c.mirrors.as_mut().unwrap()[0].remote_path = "other/place".into();
    database::apply(&mut conn, &store, &c).unwrap();
    assert_eq!(items(&mut conn), 0, "a new place is empty");
}

fn read_config(path: &Path) -> Config {
    Config::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn the_file_is_applied_at_start_and_follows_the_database_after() {
    let (db, dir, store) = setup();
    let path = dir.path().join("config").join("fafnirx.json");
    let file = ConfigFile::new(path.clone());

    // No file yet: it is made from the database, which has the defaults.
    block(file.start(&db, &store)).unwrap();
    let first = read_config(&path);
    assert_eq!(first.settings.as_ref().unwrap().version_count, Some(5));
    assert!(first.mirrors.as_ref().unwrap().is_empty());

    // A file written by hand is applied by the next start, and completed.
    std::fs::write(
        &path,
        json!({ "mirrors": [{ "name": "offsite", "remote_type": "s3", "options": { "region": "eu" } }], "devices": [{ "name": "laptop", "token": "hello" }] }).to_string(),
    )
    .unwrap();
    let file = ConfigFile::new(path.clone());
    block(file.start(&db, &store)).unwrap();
    let mut conn = db.get().unwrap();
    let hash: String = conn.query_one("SELECT token_hash FROM devices WHERE name = 'laptop'", &[]).unwrap().get(0);
    assert_eq!(hash, token_hash("hello"));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("hello") && text.contains(&hash), "the token in the clear is replaced by its hash");
    assert!(text.contains("\"settings\""), "the sections that were missing are added");

    // Nothing changed: the file is left exactly as it is.
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    block(file.sync(&db)).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);

    // A change made in the web app (here: straight in the database) reaches the file.
    conn.execute("UPDATE mirrors SET enabled = 0, keep_runs = 9", &[]).unwrap();
    conn.execute("INSERT INTO devices (name, kind, token_hash, created_at) VALUES ('shared', 'local', '!', 0)", &[])
        .unwrap();
    block(file.sync(&db)).unwrap();
    let now = read_config(&path);
    let mirror = &now.mirrors.as_ref().unwrap()[0];
    assert_eq!((mirror.enabled, mirror.keep_runs), (false, 9));
    assert!(
        now.devices.as_ref().unwrap().iter().any(|d| d.name == "shared"),
        "a folder adopted by the master is in the file"
    );

    // An edit made while the master runs is not applied, and not written over.
    let mut edited = now.clone();
    edited.mirrors.as_mut().unwrap()[0].enabled = true;
    edited
        .mirrors
        .as_mut()
        .unwrap()
        .push(serde_json::from_value(json!({ "name": "second", "remote": "crypt:x" })).unwrap());
    let edited_text = serde_json::to_string_pretty(&edited).unwrap();
    std::fs::write(&path, &edited_text).unwrap();
    conn.execute("UPDATE mirrors SET keep_runs = 2", &[]).unwrap();
    block(file.sync(&db)).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited_text, "the edit is left as it is");
    let names: Vec<String> =
        conn.query("SELECT name FROM mirrors ORDER BY name", &[]).unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(names, ["offsite"], "and not applied");

    // The restart applies it.
    let restarted = ConfigFile::new(path.clone());
    block(restarted.start(&db, &store)).unwrap();
    let names: Vec<String> =
        conn.query("SELECT name FROM mirrors ORDER BY name", &[]).unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(names, ["offsite", "second"]);
    let (enabled, keep): (i64, i64) = {
        let r = conn.query_one("SELECT enabled, keep_runs FROM mirrors WHERE name = 'offsite'", &[]).unwrap();
        (r.get(0), r.get(1))
    };
    assert_eq!((enabled, keep), (1, 9), "the file wins over what the web app changed meanwhile");
}

#[test]
fn a_file_that_cannot_be_applied_stops_the_start_and_is_left_alone() {
    let (db, dir, store) = setup();
    let path = dir.path().join("fafnirx.json");
    block(ConfigFile::new(path.clone()).start(&db, &store)).unwrap();
    let mut conn = db.get().unwrap();

    let broken = r#"{ "mirrors": [ { "name": "x", "remote_type": "s3", "keep_runs": 0 } ] "#;
    std::fs::write(&path, broken).unwrap();
    let err = block(ConfigFile::new(path.clone()).start(&db, &store)).unwrap_err();
    assert!(format!("{err:#}").contains("cannot be applied"), "{err:#}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), broken, "the file is not written over");

    // Valid JSON that does not mean anything is refused the same way, and changes nothing.
    let wrong = json!({ "mirrors": [{ "name": "x", "remote_type": "s3", "keep_runs": 0 }] }).to_string();
    std::fs::write(&path, &wrong).unwrap();
    let err = block(ConfigFile::new(path.clone()).start(&db, &store)).unwrap_err();
    assert!(format!("{err:#}").contains("keep_runs"), "{err:#}");
    let n: i64 = conn.query_one("SELECT COUNT(*) FROM mirrors", &[]).unwrap().get(0);
    assert_eq!(n, 0);

    let fixed = json!({ "mirrors": [{ "name": "x", "remote_type": "s3" }] }).to_string();
    std::fs::write(&path, fixed).unwrap();
    block(ConfigFile::new(path.clone()).start(&db, &store)).unwrap();
    let n: i64 = conn.query_one("SELECT COUNT(*) FROM mirrors", &[]).unwrap().get(0);
    assert_eq!(n, 1);

    // An empty file (just created) is no configuration: it is filled in.
    std::fs::write(&path, "  \n").unwrap();
    block(ConfigFile::new(path.clone()).start(&db, &store)).unwrap();
    assert!(read_config(&path).mirrors.unwrap().iter().any(|m| m.name == "x"));
}
