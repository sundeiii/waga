//! Saved local port forwards.

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

use super::now;
use crate::error::{AppError, AppResult};
use crate::security::validation::validate_host;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardInput {
    pub label: String,
    /// 0 = pick a free port when started.
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedForward {
    pub id: String,
    pub server_id: String,
    #[serde(flatten)]
    pub input: ForwardInput,
    pub created_at: String,
}

pub fn validate(i: &ForwardInput) -> AppResult<ForwardInput> {
    let mut o = i.clone();
    o.label = i.label.trim().chars().take(60).collect();
    o.remote_host = i.remote_host.trim().to_string();
    validate_host(&o.remote_host).map_err(|e| AppError::Validation(format!("Destination: {e}")))?;
    if o.remote_port == 0 {
        return Err(AppError::Validation("Destination port must be between 1 and 65535".into()));
    }
    Ok(o)
}

fn from_row(r: &Row) -> rusqlite::Result<SavedForward> {
    Ok(SavedForward {
        id: r.get(0)?,
        server_id: r.get(1)?,
        input: ForwardInput {
            label: r.get(2)?,
            local_port: r.get::<_, i64>(3)? as u16,
            remote_host: r.get(4)?,
            remote_port: r.get::<_, i64>(5)? as u16,
            auto_start: r.get(6)?,
        },
        created_at: r.get(7)?,
    })
}

const COLS: &str = "id, server_id, label, local_port, remote_host, remote_port, auto_start, created_at";

pub fn list_for(conn: &Connection, server_id: &str) -> AppResult<Vec<SavedForward>> {
    let mut s = conn.prepare(&format!("SELECT {COLS} FROM port_forwards WHERE server_id = ?1 ORDER BY created_at"))?;
    let rows = s.query_map([server_id], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<SavedForward> {
    conn.query_row(&format!("SELECT {COLS} FROM port_forwards WHERE id = ?1"), [id], from_row)
        .map_err(|_| AppError::NotFound("port forward".into()))
}

pub fn insert(conn: &Connection, server_id: &str, input: &ForwardInput) -> AppResult<SavedForward> {
    let i = validate(input)?;
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO port_forwards (id, server_id, label, local_port, remote_host, remote_port, auto_start, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, server_id, i.label, i.local_port, i.remote_host, i.remote_port, i.auto_start, now()],
    )?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, input: &ForwardInput) -> AppResult<SavedForward> {
    let i = validate(input)?;
    conn.execute(
        "UPDATE port_forwards SET label=?2, local_port=?3, remote_host=?4, remote_port=?5, auto_start=?6 WHERE id=?1",
        params![id, i.label, i.local_port, i.remote_host, i.remote_port, i.auto_start],
    )?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM port_forwards WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{servers, Database};

    #[test]
    fn crud_validation_and_cascade() {
        let db = Database::open_in_memory().unwrap();
        db.with(|c| {
            let s = servers::insert(c, &servers::sample_input())?;
            let f = insert(c, &s.id, &ForwardInput { label: " db ".into(), local_port: 15432, remote_host: "db.internal".into(), remote_port: 5432, auto_start: true })?;
            assert_eq!(f.input.label, "db");
            assert_eq!(list_for(c, &s.id)?.len(), 1);
            assert!(insert(c, &s.id, &ForwardInput { label: "".into(), local_port: 1, remote_host: "-oProxyCommand=x".into(), remote_port: 22, auto_start: false }).is_err());
            assert!(insert(c, &s.id, &ForwardInput { label: "".into(), local_port: 1, remote_host: "h".into(), remote_port: 0, auto_start: false }).is_err());
            servers::delete(c, &s.id)?;
            assert!(list_for(c, &s.id)?.is_empty(), "deleted with the server");
            Ok(())
        })
        .unwrap();
    }
}
