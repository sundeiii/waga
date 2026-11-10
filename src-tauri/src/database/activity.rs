//! Activity / connection history. Never record secrets or terminal contents.

use rusqlite::{params, Connection};
use serde::Serialize;

use super::now;
use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityKind {
    ConnectAttempt,
    Connected,
    Disconnected,
    AuthFailed,
    ConnectionError,
    HostKeyTrusted,
    HostKeyChanged,
    HostKeyRejected,
    ServerCreated,
    ServerUpdated,
    ServerDeleted,
    CredentialsDeleted,
    MonitoringFailed,
    Sync,
}

impl ActivityKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::ConnectAttempt => "connectAttempt",
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
            Self::AuthFailed => "authFailed",
            Self::ConnectionError => "connectionError",
            Self::HostKeyTrusted => "hostKeyTrusted",
            Self::HostKeyChanged => "hostKeyChanged",
            Self::HostKeyRejected => "hostKeyRejected",
            Self::ServerCreated => "serverCreated",
            Self::ServerUpdated => "serverUpdated",
            Self::ServerDeleted => "serverDeleted",
            Self::CredentialsDeleted => "credentialsDeleted",
            Self::MonitoringFailed => "monitoringFailed",
            Self::Sync => "sync",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub id: i64,
    pub ts: String,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub kind: String,
    pub message: String,
}

pub fn record(conn: &Connection, server_id: Option<&str>, server_name: Option<&str>, kind: ActivityKind, message: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO activity (ts, server_id, server_name, kind, message) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![now(), server_id, server_name, kind.as_str(), message],
    )?;
    // Keep the table bounded.
    conn.execute(
        "DELETE FROM activity WHERE id NOT IN (SELECT id FROM activity ORDER BY id DESC LIMIT 5000)",
        [],
    )?;
    Ok(())
}

pub fn list(conn: &Connection, limit: u32) -> AppResult<Vec<ActivityEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, ts, server_id, server_name, kind, message FROM activity ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| {
        Ok(ActivityEntry {
            id: r.get(0)?,
            ts: r.get(1)?,
            server_id: r.get(2)?,
            server_name: r.get(3)?,
            kind: r.get(4)?,
            message: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn clear(conn: &Connection) -> AppResult<()> {
    conn.execute("DELETE FROM activity", [])?;
    Ok(())
}
