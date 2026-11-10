//! Server profile persistence. Contains no secrets.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::now;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    Password,
    Key,
    Agent,
}

impl AuthMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthMethod::Password => "password",
            AuthMethod::Key => "key",
            AuthMethod::Agent => "agent",
        }
    }
    fn parse(s: &str) -> AppResult<Self> {
        match s {
            "password" => Ok(Self::Password),
            "key" => Ok(Self::Key),
            "agent" => Ok(Self::Agent),
            other => Err(AppError::Internal(format!("unknown auth method {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileKind {
    Linux,
    Debian,
    Windows,
    LocalVm,
    Proxmox,
    Custom,
}

impl ProfileKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProfileKind::Linux => "linux",
            ProfileKind::Debian => "debian",
            ProfileKind::Windows => "windows",
            ProfileKind::LocalVm => "localVm",
            ProfileKind::Proxmox => "proxmox",
            ProfileKind::Custom => "custom",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "debian" => Self::Debian,
            "windows" => Self::Windows,
            "localVm" => Self::LocalVm,
            "proxmox" => Self::Proxmox,
            "custom" => Self::Custom,
            _ => Self::Linux,
        }
    }
}

/// Editable fields of a server profile (what the form submits).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInput {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: AuthMethod,
    pub key_path: Option<String>,
    pub profile_kind: ProfileKind,
    pub group_name: Option<String>,
    pub tags: Vec<String>,
    pub notes: String,
    pub favorite: bool,
    pub auto_reconnect: bool,
    pub keepalive: bool,
    /// Connect through this saved server first (ProxyJump).
    #[serde(default)]
    pub jump_host_id: Option<String>,
}

pub const MAX_JUMPS: usize = 3;

/// Ensure a jump host exists, isn't the server itself, and doesn't create a loop
/// or a chain longer than MAX_JUMPS.
pub fn check_jump_chain(conn: &Connection, self_id: Option<&str>, jump: Option<&str>) -> AppResult<()> {
    let mut next = jump.map(str::to_string);
    let mut depth = 0;
    let mut seen: Vec<String> = self_id.map(|s| vec![s.to_string()]).unwrap_or_default();
    while let Some(id) = next {
        if seen.contains(&id) {
            return Err(AppError::Validation("Jump host chain would loop back to this server".into()));
        }
        depth += 1;
        if depth > MAX_JUMPS {
            return Err(AppError::Validation(format!("At most {MAX_JUMPS} jump hosts in a chain")));
        }
        let p = get(conn, &id).map_err(|_| AppError::Validation("Jump host no longer exists".into()))?;
        seen.push(id);
        next = p.input.jump_host_id;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: String,
    #[serde(flatten)]
    pub input: ServerInput,
    pub os_detected: Option<String>,
    pub last_connected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const COLUMNS: &str = "id, name, host, port, username, auth_method, key_path, profile_kind, group_name, \
    tags, notes, favorite, auto_reconnect, keepalive, os_detected, last_connected_at, created_at, updated_at, jump_host_id";

fn from_row(r: &Row) -> rusqlite::Result<(ServerProfile, String, String)> {
    // Return raw strings for enum/json columns; parsed by caller to surface typed errors.
    let auth: String = r.get(5)?;
    let tags: String = r.get(9)?;
    let port: i64 = r.get(3)?;
    Ok((
        ServerProfile {
            id: r.get(0)?,
            input: ServerInput {
                name: r.get(1)?,
                host: r.get(2)?,
                port: port as u16,
                username: r.get(4)?,
                auth_method: AuthMethod::Password, // replaced below
                key_path: r.get(6)?,
                profile_kind: ProfileKind::parse(&r.get::<_, String>(7)?),
                group_name: r.get(8)?,
                tags: Vec::new(),
                notes: r.get(10)?,
                favorite: r.get(11)?,
                auto_reconnect: r.get(12)?,
                keepalive: r.get(13)?,
                jump_host_id: r.get(18)?,
            },
            os_detected: r.get(14)?,
            last_connected_at: r.get(15)?,
            created_at: r.get(16)?,
            updated_at: r.get(17)?,
        },
        auth,
        tags,
    ))
}

fn finish(raw: (ServerProfile, String, String)) -> AppResult<ServerProfile> {
    let (mut p, auth, tags) = raw;
    p.input.auth_method = AuthMethod::parse(&auth)?;
    p.input.tags = serde_json::from_str(&tags).unwrap_or_default();
    Ok(p)
}

pub fn list(conn: &Connection) -> AppResult<Vec<ServerProfile>> {
    let mut stmt =
        conn.prepare(&format!("SELECT {COLUMNS} FROM servers ORDER BY name COLLATE NOCASE"))?;
    let rows = stmt.query_map([], from_row)?;
    rows.map(|r| finish(r?)).collect()
}

pub fn get(conn: &Connection, id: &str) -> AppResult<ServerProfile> {
    let raw = conn
        .query_row(&format!("SELECT {COLUMNS} FROM servers WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("server {id}")))?;
    finish(raw)
}

pub fn insert(conn: &Connection, input: &ServerInput) -> AppResult<ServerProfile> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO servers (id, name, host, port, username, auth_method, key_path, profile_kind, group_name, \
         tags, notes, favorite, auto_reconnect, keepalive, created_at, updated_at, jump_host_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16)",
        params![
            id,
            input.name,
            input.host,
            input.port,
            input.username,
            input.auth_method.as_str(),
            input.key_path,
            input.profile_kind.as_str(),
            input.group_name,
            serde_json::to_string(&input.tags)?,
            input.notes,
            input.favorite,
            input.auto_reconnect,
            input.keepalive,
            ts,
            input.jump_host_id,
        ],
    )?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, input: &ServerInput) -> AppResult<ServerProfile> {
    let n = conn.execute(
        "UPDATE servers SET name=?2, host=?3, port=?4, username=?5, auth_method=?6, key_path=?7, \
         profile_kind=?8, group_name=?9, tags=?10, notes=?11, favorite=?12, auto_reconnect=?13, \
         keepalive=?14, updated_at=?15, jump_host_id=?16 WHERE id=?1",
        params![
            id,
            input.name,
            input.host,
            input.port,
            input.username,
            input.auth_method.as_str(),
            input.key_path,
            input.profile_kind.as_str(),
            input.group_name,
            serde_json::to_string(&input.tags)?,
            input.notes,
            input.favorite,
            input.auto_reconnect,
            input.keepalive,
            now(),
            input.jump_host_id,
        ],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("server {id}")));
    }
    get(conn, id)
}

pub fn set_favorite(conn: &Connection, id: &str, favorite: bool) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE servers SET favorite=?2, updated_at=?3 WHERE id=?1",
        params![id, favorite, now()],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("server {id}")));
    }
    Ok(())
}

pub fn mark_connected(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("UPDATE servers SET last_connected_at=?2 WHERE id=?1", params![id, now()])?;
    Ok(())
}

pub fn set_os_detected(conn: &Connection, id: &str, os: Option<&str>) -> AppResult<()> {
    conn.execute("UPDATE servers SET os_detected=?2 WHERE id=?1", params![id, os])?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM servers WHERE id=?1", [id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("server {id}")));
    }
    super::tombstone(conn, "server", id)?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn sample_input() -> ServerInput {
    ServerInput {
        name: "web-01".into(),
        host: "10.0.0.5".into(),
        port: 22,
        username: "deploy".into(),
        auth_method: AuthMethod::Password,
        key_path: None,
        profile_kind: ProfileKind::Debian,
        group_name: Some("prod".into()),
        tags: vec!["web".into(), "nginx".into()],
        notes: "front".into(),
        favorite: false,
        auto_reconnect: true,
        keepalive: true,
        jump_host_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    #[test]
    fn insert_get_update_delete_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        db.with(|c| {
            let created = insert(c, &sample_input())?;
            assert_eq!(created.input.name, "web-01");
            assert_eq!(created.input.tags, vec!["web", "nginx"]);
            assert_eq!(created.input.profile_kind, ProfileKind::Debian);

            let mut edit = created.input.clone();
            edit.name = "web-02".into();
            edit.auth_method = AuthMethod::Key;
            edit.key_path = Some("C:\\keys\\id_ed25519".into());
            let updated = update(c, &created.id, &edit)?;
            assert_eq!(updated.input.name, "web-02");
            assert_eq!(updated.input.auth_method, AuthMethod::Key);

            set_favorite(c, &created.id, true)?;
            assert!(get(c, &created.id)?.input.favorite);

            mark_connected(c, &created.id)?;
            assert!(get(c, &created.id)?.last_connected_at.is_some());

            assert_eq!(list(c)?.len(), 1);
            delete(c, &created.id)?;
            assert!(matches!(get(c, &created.id), Err(AppError::NotFound(_))));
            assert!(matches!(delete(c, &created.id), Err(AppError::NotFound(_))));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn jump_chain_rules() {
        let db = Database::open_in_memory().unwrap();
        db.with(|c| {
            let a = insert(c, &sample_input())?;
            let mut bi = sample_input();
            bi.jump_host_id = Some(a.id.clone());
            let b = insert(c, &bi)?;
            assert_eq!(get(c, &b.id)?.input.jump_host_id.as_deref(), Some(a.id.as_str()));

            assert!(check_jump_chain(c, Some(&b.id), Some(&a.id)).is_ok());
            assert!(check_jump_chain(c, Some(&a.id), Some(&a.id)).is_err(), "self");
            assert!(check_jump_chain(c, Some(&a.id), Some(&b.id)).is_err(), "a -> b -> a loop");
            assert!(check_jump_chain(c, None, Some("missing")).is_err());

            // Chain length limit.
            let mut prev = b.id.clone();
            for _ in 0..3 {
                let mut i = sample_input();
                i.jump_host_id = Some(prev.clone());
                prev = insert(c, &i)?.id;
            }
            assert!(check_jump_chain(c, None, Some(&prev)).is_err(), "too deep");

            // Deleting the jump host clears the reference (ON DELETE SET NULL).
            delete(c, &a.id)?;
            assert_eq!(get(c, &b.id)?.input.jump_host_id, None);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.db");
        let id = {
            let db = Database::open(&path).unwrap();
            db.with(|c| insert(c, &sample_input())).unwrap().id
        };
        let db = Database::open(&path).unwrap();
        let p = db.with(|c| get(c, &id)).unwrap();
        assert_eq!(p.input.host, "10.0.0.5");
    }

    #[test]
    fn schema_has_no_secret_columns() {
        let db = Database::open_in_memory().unwrap();
        let cols: Vec<String> = db
            .with(|c| {
                let mut s = c.prepare("SELECT name FROM pragma_table_info('servers')")?;
                let v = s.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
                Ok(v)
            })
            .unwrap();
        for c in cols {
            assert!(!c.contains("password") && !c.contains("passphrase") && !c.contains("secret"), "{c}");
        }
    }
}
