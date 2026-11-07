use super::*;
use postgres::{Config, NoTls};
use std::sync::atomic::{AtomicU32, Ordering};

// The lock every master takes: migrations that run in parallel in one test database must not overlap, because
// some of them install extensions.
const LOCK: i64 = crate::platform::db::SCHEMA_LOCK;
static NEXT: AtomicU32 = AtomicU32::new(0);

/// A connection to a schema of its own, and the server connection that removes it again.
struct Scratch {
    conn: Client,
    admin: Client,
    schema: String,
}

impl Scratch {
    fn new() -> Self {
        let url = std::env::var("FAFNIRX_TEST_DATABASE_URL").expect("set FAFNIRX_TEST_DATABASE_URL");
        let mut config: Config = url.parse().unwrap();
        let schema = format!("m_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let mut admin = config.connect(NoTls).unwrap();
        admin.batch_execute(&format!("CREATE SCHEMA {schema}")).unwrap();
        config.options(&format!("-c search_path={schema}"));
        Self { conn: config.connect(NoTls).unwrap(), admin, schema }
    }

    fn versions(&mut self) -> Vec<i64> {
        let rows = self.conn.query("SELECT version FROM schema_migrations ORDER BY version", &[]).unwrap();
        rows.iter().map(|r| r.get(0)).collect()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = self.admin.batch_execute(&format!("DROP SCHEMA {} CASCADE", self.schema));
    }
}

fn step(version: i64, sql: &'static str) -> Migration {
    Migration { version, name: "test", sql }
}

#[test]
fn the_list_is_numbered_in_order_from_one() {
    for (i, m) in ALL.iter().enumerate() {
        assert_eq!(m.version, i as i64 + 1, "{}", m.name);
    }
}

#[test]
fn a_new_database_gets_every_migration_once() {
    let mut db = Scratch::new();
    apply(&mut db.conn, LOCK, ALL).unwrap();
    assert_eq!(db.versions(), (1..=ALL.len() as i64).collect::<Vec<_>>());
    assert!(db.conn.query_one("SELECT to_regclass('devices') IS NOT NULL", &[]).unwrap().get::<_, bool>(0));
    apply(&mut db.conn, LOCK, ALL).unwrap();
    assert_eq!(db.versions().len(), ALL.len(), "starting again changes nothing");
}

#[test]
fn only_the_missing_migrations_run_and_data_survives() {
    let mut db = Scratch::new();
    let first = step(1, "CREATE TABLE t (a BIGINT)");
    apply(&mut db.conn, LOCK, &[first]).unwrap();
    db.conn.execute("INSERT INTO t VALUES (7)", &[]).unwrap();
    let (first, second) = (step(1, "CREATE TABLE t (a BIGINT)"), step(2, "ALTER TABLE t ADD COLUMN b BIGINT DEFAULT 5"));
    apply(&mut db.conn, LOCK, &[first, second]).unwrap();
    assert_eq!(db.versions(), [1, 2]);
    let row = db.conn.query_one("SELECT a, b FROM t", &[]).unwrap();
    assert_eq!((row.get::<_, i64>(0), row.get::<_, i64>(1)), (7, 5));
}

#[test]
fn a_failing_migration_leaves_nothing_behind() {
    let mut db = Scratch::new();
    let list = [step(1, "CREATE TABLE t (a BIGINT)"), step(2, "this is not sql")];
    let err = apply(&mut db.conn, LOCK, &list).unwrap_err();
    assert!(format!("{err:#}").contains("migration 2"), "{err:#}");
    let t = db
        .conn
        .query_one("SELECT to_regclass('t') IS NULL AND to_regclass('schema_migrations') IS NULL", &[]);
    assert!(t.unwrap().get::<_, bool>(0), "the first migration was rolled back with the second");
}

#[test]
fn a_newer_database_and_an_edited_migration_are_refused() {
    let mut db = Scratch::new();
    apply(&mut db.conn, LOCK, &[step(1, "CREATE TABLE t (a BIGINT)"), step(2, "SELECT 1")]).unwrap();
    let err = apply(&mut db.conn, LOCK, &[step(1, "CREATE TABLE t (a BIGINT)")]).unwrap_err();
    assert!(format!("{err:#}").contains("newer fafnirx-master"), "{err:#}");
    let err = apply(&mut db.conn, LOCK, &[step(1, "CREATE TABLE t (a TEXT)"), step(2, "SELECT 1")]).unwrap_err();
    assert!(format!("{err:#}").contains("was changed"), "{err:#}");
}
