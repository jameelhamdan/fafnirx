//! Local state: what was last uploaded for every path, and retry bookkeeping
//! for files that failed. Anything whose current size/mtime differs from this
//! state is pending, which is also the offline queue.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct State {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Synced {
    pub path: String,
    pub size: i64,
    pub mtime_ns: i64,
    pub root_hash: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Retry {
    pub fail_count: i64,
    pub next_retry: i64,
    pub size: i64,
    pub mtime_ns: i64,
}

/// Retry delays after the 1st, 2nd and 3rd failure. After the third the file
/// is skipped until it changes again.
pub const BACKOFF_SECS: [i64; 3] = [600, 3600, 86400];

impl State {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS synced (
                 path TEXT PRIMARY KEY, size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, root_hash BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS retry (
                 path TEXT PRIMARY KEY, fail_count INTEGER NOT NULL, next_retry INTEGER NOT NULL,
                 size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, last_error TEXT);
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        Ok(Self { conn })
    }

    /// Ties this record to one master. When `master_id` is not the one the record was made against, what it
    /// says was sent no longer holds, so it is cleared and everything is collected again. Returns how many
    /// files that made pending.
    pub fn adopt_master(&mut self, master_id: &str) -> Result<usize> {
        let stored: Option<String> =
            self.conn.query_row("SELECT value FROM meta WHERE key = 'master_id'", [], |r| r.get(0)).optional()?;
        if stored.as_deref() == Some(master_id) {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        let cleared = tx.execute("DELETE FROM synced", [])?;
        tx.execute("DELETE FROM retry", [])?;
        tx.execute("DELETE FROM meta WHERE key = 'initial_sync_done'", [])?;
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('master_id', ?1)", [master_id])?;
        tx.commit()?;
        Ok(cleared)
    }

    /// Whether every file of the collected folders has been sent once to this master.
    pub fn initial_sync_done(&self) -> bool {
        self.conn
            .query_row("SELECT 1 FROM meta WHERE key = 'initial_sync_done'", [], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some()
    }

    pub fn mark_initial_sync_done(&self) -> Result<()> {
        self.conn.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('initial_sync_done', '1')", [])?;
        Ok(())
    }

    pub fn synced(&self, path: &str) -> Result<Option<Synced>> {
        Ok(self
            .conn
            .query_row("SELECT path, size, mtime_ns, root_hash FROM synced WHERE path = ?1", [path], |r| {
                Ok(Synced { path: r.get(0)?, size: r.get(1)?, mtime_ns: r.get(2)?, root_hash: r.get(3)? })
            })
            .optional()?)
    }

    pub fn all_synced(&self) -> Result<Vec<Synced>> {
        let mut stmt = self.conn.prepare("SELECT path, size, mtime_ns, root_hash FROM synced")?;
        let rows = stmt.query_map([], |r| {
            Ok(Synced { path: r.get(0)?, size: r.get(1)?, mtime_ns: r.get(2)?, root_hash: r.get(3)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_synced(&self, s: &Synced) -> Result<()> {
        self.conn.execute(
            "INSERT INTO synced (path, size, mtime_ns, root_hash) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (path) DO UPDATE SET size = ?2, mtime_ns = ?3, root_hash = ?4",
            params![s.path, s.size, s.mtime_ns, s.root_hash],
        )?;
        self.conn.execute("DELETE FROM retry WHERE path = ?1", [&s.path])?;
        Ok(())
    }

    pub fn remove_synced(&self, path: &str) -> Result<()> {
        self.conn.execute("DELETE FROM synced WHERE path = ?1", [path])?;
        self.conn.execute("DELETE FROM retry WHERE path = ?1", [path])?;
        Ok(())
    }

    pub fn retry(&self, path: &str) -> Result<Option<Retry>> {
        Ok(self
            .conn
            .query_row("SELECT fail_count, next_retry, size, mtime_ns FROM retry WHERE path = ?1", [path], |r| {
                Ok(Retry { fail_count: r.get(0)?, next_retry: r.get(1)?, size: r.get(2)?, mtime_ns: r.get(3)? })
            })
            .optional()?)
    }

    /// Records a failed attempt and returns the new failure count.
    pub fn record_failure(&self, path: &str, size: i64, mtime_ns: i64, now: i64, error: &str) -> Result<i64> {
        let count = self.retry(path)?.map(|r| r.fail_count).unwrap_or(0) + 1;
        let delay = BACKOFF_SECS[(count as usize - 1).min(BACKOFF_SECS.len() - 1)];
        self.conn.execute(
            "INSERT INTO retry (path, fail_count, next_retry, size, mtime_ns, last_error) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (path) DO UPDATE SET fail_count = ?2, next_retry = ?3, size = ?4, mtime_ns = ?5, last_error = ?6",
            params![path, count, now + delay, size, mtime_ns, error],
        )?;
        Ok(count)
    }

    /// (number of files, their total size) the master has from this device.
    pub fn totals(&self) -> Result<(usize, u64)> {
        let (n, bytes): (i64, i64) =
            self.conn
                .query_row("SELECT COUNT(*), COALESCE(SUM(size), 0) FROM synced", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok((n as usize, bytes as u64))
    }

    pub fn pending_failures(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM retry", [], |r| r.get(0))?)
    }
}

/// Whether a file with this size/mtime may be attempted now.
pub fn may_attempt(retry: &Option<Retry>, size: i64, mtime_ns: i64, now: i64) -> bool {
    match retry {
        None => true,
        Some(r) if r.fail_count as usize >= BACKOFF_SECS.len() => r.size != size || r.mtime_ns != mtime_ns,
        Some(r) => now >= r.next_retry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_then_skip_until_changed() {
        let dir = tempfile::tempdir().unwrap();
        let st = State::open(&dir.path().join("s.db")).unwrap();
        assert!(may_attempt(&st.retry("a").unwrap(), 1, 1, 0));
        assert_eq!(st.record_failure("a", 1, 1, 1000, "x").unwrap(), 1);
        let r = st.retry("a").unwrap();
        assert!(!may_attempt(&r, 1, 1, 1000 + 599));
        assert!(may_attempt(&r, 1, 1, 1000 + 600));
        st.record_failure("a", 1, 1, 2000, "x").unwrap();
        assert_eq!(st.retry("a").unwrap().unwrap().next_retry, 2000 + 3600);
        assert_eq!(st.record_failure("a", 1, 1, 3000, "x").unwrap(), 3);
        let r = st.retry("a").unwrap();
        assert!(!may_attempt(&r, 1, 1, i64::MAX / 2)); // skipped while unchanged
        assert!(may_attempt(&r, 1, 2, 0)); // changed again
        st.set_synced(&Synced { path: "a".into(), size: 1, mtime_ns: 2, root_hash: vec![0; 32] }).unwrap();
        assert!(st.retry("a").unwrap().is_none());
    }

    #[test]
    fn what_was_sent_only_counts_for_the_master_it_was_sent_to() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = State::open(&dir.path().join("s.db")).unwrap();
        assert_eq!(st.adopt_master("one").unwrap(), 0, "a new record has nothing to clear");
        assert!(!st.initial_sync_done());
        st.mark_initial_sync_done().unwrap();
        assert!(st.initial_sync_done());
        st.set_synced(&Synced { path: "a".into(), size: 1, mtime_ns: 2, root_hash: vec![0; 32] }).unwrap();
        st.record_failure("b", 1, 1, 1000, "x").unwrap();
        assert_eq!(st.adopt_master("one").unwrap(), 0, "the same master: kept");
        assert!(st.synced("a").unwrap().is_some());
        assert_eq!(st.adopt_master("two").unwrap(), 1, "another master: everything is pending again");
        assert!(!st.initial_sync_done(), "and the first sync is to be done again");
        assert!(st.synced("a").unwrap().is_none() && st.retry("b").unwrap().is_none());
        assert_eq!(st.adopt_master("two").unwrap(), 0);
    }
}
