//! Trusted SSH host key storage.

use rusqlite::{params, Connection};
use serde::Serialize;

use super::now;
use crate::error::AppResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownHost {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    /// OpenSSH public key line: "<algorithm> <base64>" (no comment).
    pub public_key: String,
    pub fingerprint: String,
    pub added_at: String,
}

/// Canonical host form used as the lookup key: lowercase, no IPv6 brackets.
pub fn normalize_host(host: &str) -> String {
    host.trim().trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase()
}

pub fn list_for(conn: &Connection, host: &str, port: u16) -> AppResult<Vec<KnownHost>> {
    let mut stmt = conn.prepare(
        "SELECT host, port, algorithm, public_key, fingerprint, added_at FROM known_hosts \
         WHERE host = ?1 AND port = ?2 ORDER BY added_at",
    )?;
    let rows = stmt.query_map(params![normalize_host(host), port], |r| {
        Ok(KnownHost {
            host: r.get(0)?,
            port: r.get::<_, i64>(1)? as u16,
            algorithm: r.get(2)?,
            public_key: r.get(3)?,
            fingerprint: r.get(4)?,
            added_at: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Trust an additional key for a host that has no conflicting entries.
pub fn add(conn: &Connection, host: &str, port: u16, algorithm: &str, public_key: &str, fingerprint: &str) -> AppResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO known_hosts (host, port, algorithm, public_key, fingerprint, added_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![normalize_host(host), port, algorithm, public_key, fingerprint, now()],
    )?;
    Ok(())
}

/// Replace ALL trusted keys for host:port with a single new key.
/// Only call after the user explicitly acknowledged a changed host key.
pub fn replace(conn: &Connection, host: &str, port: u16, algorithm: &str, public_key: &str, fingerprint: &str) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM known_hosts WHERE host = ?1 AND port = ?2",
        params![normalize_host(host), port],
    )?;
    tx.execute(
        "INSERT INTO known_hosts (host, port, algorithm, public_key, fingerprint, added_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![normalize_host(host), port, algorithm, public_key, fingerprint, now()],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn list_all(conn: &Connection) -> AppResult<Vec<KnownHost>> {
    let mut stmt = conn.prepare(
        "SELECT host, port, algorithm, public_key, fingerprint, added_at FROM known_hosts ORDER BY host, port",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(KnownHost {
            host: r.get(0)?,
            port: r.get::<_, i64>(1)? as u16,
            algorithm: r.get(2)?,
            public_key: r.get(3)?,
            fingerprint: r.get(4)?,
            added_at: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn remove(conn: &Connection, host: &str, port: u16) -> AppResult<usize> {
    let n = conn.execute(
        "DELETE FROM known_hosts WHERE host = ?1 AND port = ?2",
        params![normalize_host(host), port],
    )?;
    if n > 0 {
        super::tombstone(conn, "known_host", &format!("{}:{port}", normalize_host(host)))?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    #[test]
    fn add_replace_and_normalization() {
        let db = Database::open_in_memory().unwrap();
        db.with(|c| {
            add(c, "Example.COM", 22, "ssh-ed25519", "ssh-ed25519 AAAA1", "SHA256:a")?;
            add(c, "example.com", 22, "ecdsa-sha2-nistp256", "ecdsa-sha2-nistp256 BBBB", "SHA256:b")?;
            assert_eq!(list_for(c, "EXAMPLE.com", 22)?.len(), 2);
            assert_eq!(list_for(c, "example.com", 2222)?.len(), 0);

            replace(c, "example.com", 22, "ssh-ed25519", "ssh-ed25519 CCCC", "SHA256:c")?;
            let rows = list_for(c, "example.com", 22)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].fingerprint, "SHA256:c");

            add(c, "[fe80::1]", 22, "ssh-ed25519", "ssh-ed25519 DDDD", "SHA256:d")?;
            assert_eq!(list_for(c, "fe80::1", 22)?.len(), 1);
            assert_eq!(remove(c, "fe80::1", 22)?, 1);
            Ok(())
        })
        .unwrap();
    }
}
