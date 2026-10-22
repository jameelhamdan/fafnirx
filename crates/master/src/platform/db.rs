use anyhow::{Context, Result};
use fafnirx_common::unix_now;
use postgres::{Client, Config, GenericClient, NoTls};
use r2d2_postgres::PostgresConnectionManager;
use std::time::Duration;

/// A pool of synchronous PostgreSQL connections. Always use it from a blocking thread (see [`with`]):
/// the `postgres` crate drives its own runtime and panics when called from async code.
pub type Db = r2d2::Pool<PostgresConnectionManager<NoTls>>;

/// A device as the data path knows it: the row id and the folder name.
#[derive(Clone, Debug)]
pub struct Device {
    pub id: i64,
    pub name: String,
}

/// `N` random bytes from the operating system.
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    getrandom::getrandom(&mut out).expect("the operating system has a random source");
    out
}

/// A new device token: 32 random bytes, URL-safe.
pub fn new_token() -> String {
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, random_bytes::<32>())
}

/// Hex SHA-256 of a token, which is all the database keeps of it.
pub fn hash_token(token: &str) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(token.as_bytes()))
}

/// The channel every master process listens on for changes to show live.
pub const CHANNEL: &str = "fafnirx";

/// Tells the processes that serve the admin API that something changed. `topic` is one of `files`
/// (then `key` is the device), `activity`, `devices`, `alerts`, `jobs`. Delivered when the transaction commits.
pub fn notify(conn: &mut impl GenericClient, topic: &str, key: &str) -> Result<(), postgres::Error> {
    let payload = serde_json::json!({ "topic": topic, "key": key }).to_string();
    conn.execute("SELECT pg_notify($1, $2)", &[&CHANNEL, &payload])?;
    Ok(())
}

/// Takes the lock for one path of a device until the end of the transaction, so that commits, deletes and
/// what the file watcher finds never interleave on that path. Works across processes.
pub fn lock_path(conn: &mut impl GenericClient, device_id: i64, path: &str) -> Result<(), postgres::Error> {
    conn.execute("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", &[&format!("{device_id}/{path}")])?;
    Ok(())
}

/// Queues background work for the worker role.
pub fn enqueue(conn: &mut impl GenericClient, kind: &str, payload: &serde_json::Value) -> Result<(), postgres::Error> {
    conn.execute(
        "INSERT INTO jobs (kind, payload, created_at) VALUES ($1, $2, $3)",
        &[&kind, &payload.to_string(), &unix_now()],
    )?;
    Ok(())
}

/// Serializes migrations between master processes, which all apply them on start.
pub(crate) const SCHEMA_LOCK: i64 = 0x6661_666e_6972_7801;
const POOL_SIZE: u32 = 8;
const CONNECT_ATTEMPTS: u32 = 30;

/// Connects to PostgreSQL (waiting for it to come up) and applies the migrations that are missing.
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
    super::migrations::apply(conn, SCHEMA_LOCK, super::migrations::ALL)
}

/// A `LIKE` pattern for the paths that start with `prefix` (`path LIKE $1`), which a `text_pattern_ops` index answers.
/// Characters that mean something to `LIKE` are taken literally.
pub fn like_prefix(prefix: &str) -> String {
    let mut pattern = String::with_capacity(prefix.len() + 1);
    for c in prefix.chars() {
        if matches!(c, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

/// Held by the one process that runs the singleton background work (scheduled jobs, sources, mirrors,
/// the file watcher).
const LEADER_LOCK: i64 = 0x6661_666e_6972_7802;

/// Waits until this process is the leader, then keeps the lock for as long as it lives. If the
/// connection that holds the lock is lost the process exits, so that a supervisor restarts it and a
/// single leader remains.
pub async fn lead(url: &str) -> Result<()> {
    let config: Config = url.parse().context("FAFNIRX_DATABASE_URL is not a valid PostgreSQL connection string")?;
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut conn = config.connect(NoTls)?;
        while !conn.query_one("SELECT pg_try_advisory_lock($1)", &[&LEADER_LOCK])?.get::<_, bool>(0) {
            std::thread::sleep(Duration::from_secs(5));
        }
        tracing::info!("this process runs the scheduled work");
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            if let Err(e) = conn.simple_query("SELECT 1") {
                tracing::error!("lost the connection that holds the leader lock: {e}");
                std::process::exit(1);
            }
        });
        Ok(())
    })
    .await?
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
    notify(conn, "alerts", key)?;
    Ok(())
}

pub fn resolve_alert(conn: &mut impl GenericClient, kind: &str, key: &str) -> Result<(), postgres::Error> {
    let n = conn.execute(
        "UPDATE alerts SET resolved_at = $3 WHERE kind = $1 AND key = $2 AND resolved_at IS NULL",
        &[&kind, &key, &unix_now()],
    )?;
    if n > 0 {
        notify(conn, "alerts", key)?;
    }
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

    /// The device made by [`db_with_device`].
    pub fn device() -> Device {
        Device { id: 1, name: "d".into() }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_pattern_takes_wildcards_literally() {
        assert_eq!(like_prefix("photos/2024/"), "photos/2024/%");
        assert_eq!(like_prefix("100%_sure\\"), r"100\%\_sure\\%");
        assert_eq!(like_prefix(""), "%");

        let db = testing::db_with_device();
        let mut c = db.get().unwrap();
        for path in ["a_b/x", "aXb/x", "a%/y", "abc/z", "a_b2/w"] {
            c.execute("INSERT INTO files (device_id, path) VALUES (1, $1)", &[&path]).unwrap();
        }
        let mut below = |folder: &str| -> Vec<String> {
            let rows = c.query("SELECT path FROM files WHERE path LIKE $1 ORDER BY path", &[&like_prefix(folder)]);
            rows.unwrap().iter().map(|r| r.get(0)).collect()
        };
        assert_eq!(below("a_b/"), ["a_b/x"], "_ is not a wildcard");
        assert_eq!(below("a%/"), ["a%/y"], "% is not a wildcard");
    }

    #[test]
    fn the_indexes_behind_the_lookups_exist() {
        let db = testing::db();
        let mut c = db.get().unwrap();
        for index in [
            "files_path_search",
            "files_path_prefix",
            "files_current_version",
            "files_deleted",
            "files_library",
            "versions_root",
            "versions_pruned",
            "events_path",
            "events_ts",
            "devices_token",
            "jobs_queue",
        ] {
            let found: bool = c.query_one("SELECT to_regclass($1) IS NOT NULL", &[&index]).unwrap().get(0);
            assert!(found, "missing index {index}");
        }
    }
}
