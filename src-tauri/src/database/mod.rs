//! SQLite persistence for non-sensitive data.

pub mod activity;
pub mod forwards;
pub mod host_cas;
pub mod known_hosts;
pub mod servers;
pub mod settings;
pub mod snippets;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

/// Ordered list of schema migrations. Never edit an applied migration;
/// append a new one instead.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../migrations/0001_init.sql")),
    (2, include_str!("../../migrations/0002_snippets.sql")),
    (3, include_str!("../../migrations/0003_port_forwards.sql")),
];

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> AppResult<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Run a closure with exclusive access to the connection.
    /// Keep closures short: this is a blocking lock.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
        f(&conn)
    }
}

fn migrate(conn: &mut Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
    )?;
    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    )?;
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, now()],
        )?;
        tx.commit()?;
    }
    Ok(())
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        drop(Database::open(&path).unwrap());
        let db = Database::open(&path).unwrap();
        let v: i64 = db
            .with(|c| Ok(c.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(v, MIGRATIONS.last().unwrap().0);
    }
}
