use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A fresh, empty database with the schema applied.
pub fn db() -> Db {
    let url = std::env::var("FAFNIRX_TEST_DATABASE_URL").expect("set FAFNIRX_TEST_DATABASE_URL to a PostgreSQL server the tests may create schemas in");
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

/// The folder made by [`db_with_folder`].
pub fn folder() -> FolderRef {
    FolderRef { id: 1, key: "admin/f".into() }
}

/// `db()` with one folder named `f` (id 1) of the administrator (user 1), so its key is `admin/f`.
pub fn db_with_folder() -> Db {
    let db = db();
    db.get()
        .unwrap()
        .execute("INSERT INTO folders (name, kind, created_at, user_id) VALUES ('f', 'user', 0, 1)", &[])
        .unwrap();
    db
}

/// `db()` with one device named `d` (id 1) of the administrator (user 1), so its key is `admin/d`.
pub fn db_with_device() -> Db {
    let db = db();
    db.get()
        .unwrap()
        .execute("INSERT INTO devices (name, token_hash, created_at, user_id) VALUES ('d', 'x', 0, 1)", &[])
        .unwrap();
    db
}
