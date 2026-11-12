//! The schema of the database as a list of numbered migrations (`crates/master/migrations/NNNN_name.sql`).
//! The master applies the ones it has not applied yet when it starts, in order and in one transaction
//! behind an advisory lock, and records each in `schema_migrations` with a checksum.
//!
//! To change the schema, add a migration at the end of [`ALL`]. A migration that has been applied is never edited:
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
    Migration { version: 1, name: "schema", sql: include_str!("../../migrations/0001_schema.sql") },
    Migration { version: 2, name: "folders", sql: include_str!("../../migrations/0002_folders.sql") },
    Migration { version: 3, name: "link_jobs", sql: include_str!("../../migrations/0003_link_jobs.sql") },
    Migration {
        version: 4,
        name: "devices_and_folders",
        sql: include_str!("../../migrations/0004_devices_and_folders.sql"),
    },
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
#[path = "../../tests/unit/platform/migrations.rs"]
mod tests;
