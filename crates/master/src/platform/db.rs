use anyhow::{Context, Result};
use fafnirx_db::client::GenericClient;
use postgres::{Client, Config, NoTls};
use r2d2_postgres::PostgresConnectionManager;
use std::time::Duration;

/// A pool of synchronous PostgreSQL connections. Always use it from a blocking thread (see [`with`]):
/// the `postgres` crate drives its own runtime and panics when called from async code.
pub type Db = r2d2::Pool<PostgresConnectionManager<NoTls>>;

/// A device as the data path knows it: the row id and its folder (`<user>/<device>`, or `_master` for the master files).
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

/// The channel the master listens on for changes to show live.
pub const CHANNEL: &str = "fafnirx";

/// What a change is about. Browsers learn of it through the live update stream (`web/events.rs`) and refresh what it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topic {
    /// Everything may have changed: a browser that missed events looks again at all of it.
    Resync,
    /// Albums, tags, favorites and the timeline. Key: none.
    Library,
    /// A picture that someone asked for is made. Key: the id of the file.
    Thumbs,
    /// Files of a device. Key: the folder of the device.
    Files,
    /// A device or folder was added, changed or removed. Key: its folder (empty: the list).
    Devices,
    /// What a computer is doing now. Key: its folder.
    Activity,
    /// An alert was opened or closed. Key: the folder of the device it is about, or what it names.
    Alerts,
    /// A source or mirror ran. Key: `<user>/<source>`, or `mirrors`.
    Jobs,
    /// A user was added or changed. Only for administrators.
    Users,
}

/// Who is told about a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Everybody: not about one device, or only a hint to look again.
    Everyone,
    /// Whoever can see the folder that is the key (everybody when there is none).
    Folder,
    /// Whoever can see the folder that is the key; a key that is not a folder (a mirror) is for the administrators.
    Source,
    Admins,
}

impl Topic {
    pub const ALL: [Topic; 9] = [
        Topic::Resync,
        Topic::Library,
        Topic::Thumbs,
        Topic::Files,
        Topic::Devices,
        Topic::Activity,
        Topic::Alerts,
        Topic::Jobs,
        Topic::Users,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Topic::Resync => "resync",
            Topic::Library => "library",
            Topic::Thumbs => "thumbs",
            Topic::Files => "files",
            Topic::Devices => "devices",
            Topic::Activity => "activity",
            Topic::Alerts => "alerts",
            Topic::Jobs => "jobs",
            Topic::Users => "users",
        }
    }

    pub fn parse(name: &str) -> Option<Topic> {
        Topic::ALL.into_iter().find(|t| t.name() == name)
    }

    pub fn reach(self) -> Reach {
        match self {
            Topic::Resync | Topic::Library | Topic::Thumbs => Reach::Everyone,
            Topic::Files | Topic::Devices | Topic::Activity | Topic::Alerts => Reach::Folder,
            Topic::Jobs => Reach::Source,
            Topic::Users => Reach::Admins,
        }
    }
}

/// Tells the live update stream (and so the browsers) that something changed. Delivered when the transaction commits.
pub fn notify(conn: &mut impl GenericClient, topic: Topic, key: &str) -> Result<(), postgres::Error> {
    let payload = serde_json::json!({ "topic": topic.name(), "key": key }).to_string();
    conn.execute("SELECT pg_notify($1, $2)", &[&CHANNEL, &payload])?;
    Ok(())
}

/// Takes the lock for one path of a device until the end of the transaction, so that commits, deletes and
/// what the file watcher finds never interleave on that path. Works across processes.
pub fn lock_path(conn: &mut impl GenericClient, device_id: i64, path: &str) -> Result<(), postgres::Error> {
    conn.execute("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", &[&format!("{device_id}/{path}")])?;
    Ok(())
}

/// Serializes migrations, in case two masters start on one database.
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
            .max_size(super::tuning::get("FAFNIRX_DB_POOL").map_or(POOL_SIZE, |n| n as u32))
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
    like_escape(prefix) + "%"
}

/// `text` with the characters that mean something to `LIKE` taken literally.
pub fn like_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
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

/// Databases for unit tests. Needs a PostgreSQL server: set `FAFNIRX_TEST_DATABASE_URL`
/// (for example `postgres://postgres:test@localhost:5432/postgres`). Every call gets its own schema.
#[cfg(test)]
#[path = "../../tests/unit/platform/db_testing.rs"]
pub mod testing;

#[cfg(test)]
#[path = "../../tests/unit/platform/db.rs"]
mod tests;
