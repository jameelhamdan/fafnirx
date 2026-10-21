use anyhow::{Context, Result};
use fafnirx_common::unix_now;
use postgres::{Client, Config, GenericClient, NoTls};
use r2d2_postgres::PostgresConnectionManager;
use std::time::Duration;

/// A pool of synchronous PostgreSQL connections. Always use it from a blocking thread (see [`with`]):
/// the `postgres` crate drives its own runtime and panics when called from async code.
pub type Db = r2d2::Pool<PostgresConnectionManager<NoTls>>;

const SCHEMA: &str = include_str!("../../../schema.sql");
/// Serializes schema creation between the engine and the web UI, which both apply it on start.
const SCHEMA_LOCK: i64 = 0x6661_666e_6972_7801;
const POOL_SIZE: u32 = 8;
const CONNECT_ATTEMPTS: u32 = 30;

/// Held shared while a commit checks that its chunks exist and records the version, and exclusively
/// while garbage collection re-checks references and deletes chunks. Without it a chunk could be
/// deleted between a commit's existence check and its insert.
pub static CHUNK_GATE: std::sync::RwLock<()> = std::sync::RwLock::new(());

/// Connects to PostgreSQL (waiting for it to come up) and applies the schema.
pub fn open(url: &str) -> Result<Db> {
    let config: Config = url.parse().context("FAFNIRX_DATABASE_URL is not a valid PostgreSQL connection string")?;
    open_with(config)
}

fn open_with(config: Config) -> Result<Db> {
    let mut attempt = 1;
    let pool = loop {
        let manager = PostgresConnectionManager::new(config.clone(), NoTls);
        let built = r2d2::Pool::builder()
            .max_size(POOL_SIZE)
            .min_idle(Some(1))
            .connection_timeout(Duration::from_secs(5))
            .build(manager);
        match built {
            Ok(pool) => break pool,
            Err(e) if attempt < CONNECT_ATTEMPTS => {
                tracing::warn!("database not reachable yet ({attempt}/{CONNECT_ATTEMPTS}): {e}");
                attempt += 1;
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(e) => return Err(e).context("cannot connect to the database"),
        }
    };
    apply_schema(&mut *pool.get()?)?;
    Ok(pool)
}

fn apply_schema(conn: &mut Client) -> Result<()> {
    let mut tx = conn.transaction()?;
    tx.execute("SELECT pg_advisory_xact_lock($1)", &[&SCHEMA_LOCK])?;
    tx.batch_execute(SCHEMA)?;
    tx.commit()?;
    Ok(())
}

/// Runs `f` on a blocking thread with a pooled database connection.
pub async fn with<T, F>(db: &Db, f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Client) -> Result<T> + Send + 'static,
{
    let db = db.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = db.get()?;
        f(&mut conn)
    })
    .await?
}

pub fn setting(conn: &mut impl GenericClient, key: &str, default: i64) -> i64 {
    conn.query_opt("SELECT value FROM settings WHERE key = $1", &[&key])
        .ok()
        .flatten()
        .and_then(|r| r.get::<_, String>(0).parse().ok())
        .unwrap_or(default)
}

/// Opens an alert, or refreshes the message of the already open one.
pub fn upsert_alert(
    conn: &mut impl GenericClient,
    kind: &str,
    key: &str,
    message: &str,
) -> Result<(), postgres::Error> {
    conn.execute(
        "INSERT INTO alerts (kind, key, message, created_at, updated_at) VALUES ($1, $2, $3, $4, $4)
         ON CONFLICT (kind, key) WHERE resolved_at IS NULL DO UPDATE SET message = excluded.message, updated_at = excluded.updated_at",
        &[&kind, &key, &message, &unix_now()],
    )?;
    Ok(())
}

pub fn resolve_alert(conn: &mut impl GenericClient, kind: &str, key: &str) -> Result<(), postgres::Error> {
    conn.execute(
        "UPDATE alerts SET resolved_at = $3 WHERE kind = $1 AND key = $2 AND resolved_at IS NULL",
        &[&kind, &key, &unix_now()],
    )?;
    Ok(())
}

/// Databases for unit tests. Needs a PostgreSQL server: set `FAFNIRX_TEST_DATABASE_URL`
/// (for example `postgres://postgres:test@localhost:5432/postgres`). Every call gets its own schema.
#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    /// A fresh, empty database with the schema applied.
    pub fn db() -> Db {
        let url = std::env::var("FAFNIRX_TEST_DATABASE_URL")
            .expect("set FAFNIRX_TEST_DATABASE_URL to a PostgreSQL server the tests may create schemas in");
        let mut config: Config = url.parse().expect("FAFNIRX_TEST_DATABASE_URL");
        let schema = format!("t_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        config
            .connect(NoTls)
            .and_then(|mut c| c.batch_execute(&format!("CREATE SCHEMA {schema}")))
            .expect("create test schema");
        config.options(&format!("-c search_path={schema}"));
        open_with(config).expect("open test database")
    }

    /// How many alerts are open.
    pub fn open_alerts(c: &mut Client) -> i64 {
        c.query_one("SELECT COUNT(*) FROM alerts WHERE resolved_at IS NULL", &[]).unwrap().get(0)
    }

    /// `db()` with one ordinary device named `d` (id 1).
    pub fn db_with_device() -> Db {
        let db = db();
        db.get()
            .unwrap()
            .execute("INSERT INTO devices (name, token_hash, created_at) VALUES ('d', 'x', 0)", &[])
            .unwrap();
        db
    }
}
