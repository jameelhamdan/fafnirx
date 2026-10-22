//! The schema of the database as a list of numbered migrations (`crates/master/migrations/NNNN_name.sql`).
//! The master applies the ones it has not applied yet when it starts, in order and in one transaction
//! behind an advisory lock, and records each in `schema_migrations` with a checksum.
//!
//! To change the schema, add a migration at the end of [`ALL`]. A migration that has been released is never edited:
//! the checksum would no longer match and the master refuses to start. A database that is newer than this master
//! is refused too, so an old binary never runs against a schema it does not know.

use anyhow::{bail, Context, Result};
use fafnirx_common::unix_now;
use postgres::Client;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const ALL: &[Migration] = &[
    Migration { version: 1, name: "initial", sql: include_str!("../../migrations/0001_initial.sql") },
    Migration { version: 2, name: "indexes", sql: include_str!("../../migrations/0002_indexes.sql") },
    Migration { version: 3, name: "device_totals", sql: include_str!("../../migrations/0003_device_totals.sql") },
    Migration { version: 4, name: "mirror_remote", sql: include_str!("../../migrations/0004_mirror_remote.sql") },
];

const TRACKING: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (
    version    BIGINT PRIMARY KEY,
    name       TEXT NOT NULL,
    checksum   TEXT NOT NULL,
    applied_at BIGINT NOT NULL
)";

fn checksum(sql: &str) -> String {
    hex::encode(Sha256::digest(sql.as_bytes()))
}

/// Brings the database up to the last of `migrations`. All or nothing: if one fails, none is applied.
pub fn apply(conn: &mut Client, lock: i64, migrations: &[Migration]) -> Result<()> {
    let mut tx = conn.transaction()?;
    tx.execute("SELECT pg_advisory_xact_lock($1)", &[&lock])?;
    tx.batch_execute(TRACKING)?;
    let applied: HashMap<i64, String> = tx
        .query("SELECT version, checksum FROM schema_migrations", &[])?
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();

    let known = migrations.last().map_or(0, |m| m.version);
    if let Some(newest) = applied.keys().max().filter(|v| **v > known) {
        bail!(
            "this database is at version {newest} but this fafnirx-master only knows versions up to {known}: \
             run the newer fafnirx-master that made it"
        );
    }
    for m in migrations {
        match applied.get(&m.version) {
            Some(sum) if *sum == checksum(m.sql) => {}
            Some(_) => bail!(
                "migration {} ({}) was changed after it was applied to this database: add a new migration instead",
                m.version,
                m.name
            ),
            None => {
                tx.batch_execute(m.sql).with_context(|| format!("migration {} ({})", m.version, m.name))?;
                tx.execute(
                    "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES ($1, $2, $3, $4)",
                    &[&m.version, &m.name, &checksum(m.sql), &unix_now()],
                )?;
                tracing::info!("database migration {} ({}) applied", m.version, m.name);
            }
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
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
    fn a_database_made_before_migrations_were_recorded_is_adopted() {
        let mut db = Scratch::new();
        db.conn.batch_execute(ALL[0].sql).unwrap();
        db.conn.execute("INSERT INTO devices (name, token_hash, created_at) VALUES ('keep', 'x', 0)", &[]).unwrap();
        apply(&mut db.conn, LOCK, ALL).unwrap();
        assert_eq!(db.versions(), (1..=ALL.len() as i64).collect::<Vec<_>>());
        assert_eq!(db.conn.query_one("SELECT COUNT(*) FROM devices", &[]).unwrap().get::<_, i64>(0), 1);
    }

    #[test]
    fn only_the_missing_migrations_run_and_data_survives() {
        let mut db = Scratch::new();
        let first = step(1, "CREATE TABLE t (a BIGINT)");
        apply(&mut db.conn, LOCK, &[first]).unwrap();
        db.conn.execute("INSERT INTO t VALUES (7)", &[]).unwrap();
        let (first, second) =
            (step(1, "CREATE TABLE t (a BIGINT)"), step(2, "ALTER TABLE t ADD COLUMN b BIGINT DEFAULT 5"));
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
        let t = db.conn.query_one("SELECT to_regclass('t') IS NULL AND to_regclass('schema_migrations') IS NULL", &[]);
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
}
