//! Trusted certificate authorities for SSH host certificates.

use rusqlite::{params, Connection};
use serde::Serialize;

use super::now;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCa {
    pub id: String,
    /// Host pattern this CA may vouch for: "*.example.com", "10.0.*", "*" …
    pub pattern: String,
    /// "<algorithm> <base64>"
    pub public_key: String,
    pub fingerprint: String,
    pub comment: String,
    pub added_at: String,
}

/// OpenSSH-style glob: '*' any run, '?' one char; case-insensitive.
pub fn pattern_matches(pattern: &str, host: &str) -> bool {
    fn rec(p: &[u8], h: &[u8]) -> bool {
        match (p.first(), h.first()) {
            (None, None) => true,
            (Some(b'*'), _) => rec(&p[1..], h) || (!h.is_empty() && rec(p, &h[1..])),
            (Some(b'?'), Some(_)) => rec(&p[1..], &h[1..]),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => rec(&p[1..], &h[1..]),
            _ => false,
        }
    }
    pattern.split(',').map(str::trim).filter(|p| !p.is_empty()).any(|p| rec(p.as_bytes(), host.as_bytes()))
}

pub fn validate_pattern(p: &str) -> AppResult<String> {
    let p = p.trim();
    if p.is_empty() || p.len() > 255 || p.chars().any(|c| c.is_whitespace() && c != ' ' || c.is_control()) {
        return Err(AppError::Validation("Host pattern is required, e.g. *.example.com".into()));
    }
    Ok(p.to_string())
}

pub fn list(conn: &Connection) -> AppResult<Vec<HostCa>> {
    let mut s = conn.prepare("SELECT id, pattern, public_key, fingerprint, comment, added_at FROM host_cas ORDER BY added_at")?;
    let rows = s.query_map([], |r| {
        Ok(HostCa { id: r.get(0)?, pattern: r.get(1)?, public_key: r.get(2)?, fingerprint: r.get(3)?, comment: r.get(4)?, added_at: r.get(5)? })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn insert(conn: &Connection, pattern: &str, public_key: &str, fingerprint: &str, comment: &str) -> AppResult<HostCa> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO host_cas (id, pattern, public_key, fingerprint, comment, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, validate_pattern(pattern)?, public_key, fingerprint, comment.chars().take(100).collect::<String>(), now()],
    )?;
    Ok(list(conn)?.into_iter().find(|c| c.id == id).unwrap())
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    if conn.execute("DELETE FROM host_cas WHERE id = ?1", [id])? > 0 {
        super::tombstone(conn, "host_ca", id)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(pattern_matches("*.example.com", "web.example.com"));
        assert!(pattern_matches("*.example.com", "a.b.EXAMPLE.com"));
        assert!(!pattern_matches("*.example.com", "example.com"));
        assert!(!pattern_matches("*.example.com", "evil-example.com"));
        assert!(pattern_matches("10.0.?.*", "10.0.1.25"));
        assert!(pattern_matches("db,*.corp", "db"));
        assert!(pattern_matches("db,*.corp", "x.corp"));
        assert!(pattern_matches("*", "anything"));
        assert!(!pattern_matches("", "anything"));
    }
}
