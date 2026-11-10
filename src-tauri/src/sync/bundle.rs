//! The syncable snapshot ("bundle") and an order-independent merge.
//!
//! Merge rules (same on every PC, so all PCs converge):
//! * each row carries `updated_at`; the newer version wins;
//! * a deletion (tombstone) wins over versions older than it and loses to
//!   versions edited after it;
//! * trusted host keys merge per host:port (newest set wins);
//! * per-device fields (last connected, detected OS) are never overwritten;
//! * secrets travel only if the user opted in, and are written to the OS
//!   credential store — never to SQLite.

use std::collections::HashMap;

use chrono::DateTime;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::credentials::{passphrase_account, password_account, SecretStore};
use crate::error::AppResult;

pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerRow {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_method: String,
    pub key_path: Option<String>,
    pub profile_kind: String,
    pub group_name: Option<String>,
    pub tags: String,
    pub notes: String,
    pub favorite: bool,
    pub auto_reconnect: bool,
    pub keepalive: bool,
    pub jump_host_id: Option<String>,
    pub os_detected: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnippetRow {
    pub id: String,
    pub title: String,
    pub description: String,
    pub command: String,
    pub category: String,
    pub tags: String,
    pub favorite: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForwardRow {
    pub id: String,
    pub server_id: String,
    pub label: String,
    pub local_port: i64,
    pub remote_host: String,
    pub remote_port: i64,
    pub auto_start: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostCaRow {
    pub id: String,
    pub pattern: String,
    pub public_key: String,
    pub fingerprint: String,
    pub comment: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KnownHostRow {
    pub host: String,
    pub port: i64,
    pub algorithm: String,
    pub public_key: String,
    pub fingerprint: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tombstone {
    pub kind: String,
    pub id: String,
    pub deleted_at: String,
}

#[derive(Serialize, Deserialize)]
pub struct SecretRow {
    pub server_id: String,
    /// "password" | "passphrase"
    pub kind: String,
    pub value: String,
}

impl Drop for SecretRow {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.value.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
pub struct Bundle {
    pub format: u32,
    pub app: String,
    pub device_id: String,
    pub device_name: String,
    pub created_at: String,
    pub servers: Vec<ServerRow>,
    pub snippets: Vec<SnippetRow>,
    pub forwards: Vec<ForwardRow>,
    pub host_cas: Vec<HostCaRow>,
    pub known_hosts: Vec<KnownHostRow>,
    pub tombstones: Vec<Tombstone>,
    #[serde(default)]
    pub secrets: Vec<SecretRow>,
}

#[derive(Debug, Default, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MergeStats {
    pub added: u32,
    pub updated: u32,
    pub deleted: u32,
    pub secrets: u32,
}

impl MergeStats {
    pub fn add(&mut self, o: &MergeStats) {
        self.added += o.added;
        self.updated += o.updated;
        self.deleted += o.deleted;
        self.secrets += o.secrets;
    }
}

/// a is strictly newer than b (RFC 3339; falls back to string order).
pub fn newer(a: &str, b: &str) -> bool {
    match (DateTime::parse_from_rfc3339(a), DateTime::parse_from_rfc3339(b)) {
        (Ok(x), Ok(y)) => x > y,
        _ => a > b,
    }
}

pub fn device_id(conn: &Connection) -> AppResult<String> {
    if let Some(v) = conn.query_row("SELECT value FROM device WHERE key='id'", [], |r| r.get::<_, String>(0)).optional()? {
        return Ok(v);
    }
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute("INSERT INTO device (key, value) VALUES ('id', ?1)", [&id])?;
    Ok(id)
}

// ---------------------------------------------------------------- export

pub fn export(conn: &Connection, secrets: &dyn SecretStore, include_secrets: bool, device_name: &str) -> AppResult<Bundle> {
    let servers: Vec<ServerRow> = {
        let mut s = conn.prepare(
            "SELECT id, name, host, port, username, auth_method, key_path, profile_kind, group_name, tags, notes, favorite,
                    auto_reconnect, keepalive, jump_host_id, os_detected, created_at, updated_at FROM servers",
        )?;
        let rows = s.query_map([], |r| {
            Ok(ServerRow {
                id: r.get(0)?, name: r.get(1)?, host: r.get(2)?, port: r.get(3)?, username: r.get(4)?, auth_method: r.get(5)?,
                key_path: r.get(6)?, profile_kind: r.get(7)?, group_name: r.get(8)?, tags: r.get(9)?, notes: r.get(10)?,
                favorite: r.get(11)?, auto_reconnect: r.get(12)?, keepalive: r.get(13)?, jump_host_id: r.get(14)?,
                os_detected: r.get(15)?, created_at: r.get(16)?, updated_at: r.get(17)?,
            })
        })?;
        rows.collect::<Result<_, _>>()?
    };
    let snippets: Vec<SnippetRow> = {
        let mut s = conn.prepare("SELECT id, title, description, command, category, tags, favorite, created_at, updated_at FROM snippets")?;
        let rows = s.query_map([], |r| {
            Ok(SnippetRow {
                id: r.get(0)?, title: r.get(1)?, description: r.get(2)?, command: r.get(3)?, category: r.get(4)?, tags: r.get(5)?,
                favorite: r.get(6)?, created_at: r.get(7)?, updated_at: r.get(8)?,
            })
        })?;
        rows.collect::<Result<_, _>>()?
    };
    let forwards: Vec<ForwardRow> = {
        let mut s = conn.prepare(
            "SELECT id, server_id, label, local_port, remote_host, remote_port, auto_start, created_at, COALESCE(updated_at, created_at) FROM port_forwards",
        )?;
        let rows = s.query_map([], |r| {
            Ok(ForwardRow {
                id: r.get(0)?, server_id: r.get(1)?, label: r.get(2)?, local_port: r.get(3)?, remote_host: r.get(4)?,
                remote_port: r.get(5)?, auto_start: r.get(6)?, created_at: r.get(7)?, updated_at: r.get(8)?,
            })
        })?;
        rows.collect::<Result<_, _>>()?
    };
    let host_cas: Vec<HostCaRow> = {
        let mut s = conn.prepare("SELECT id, pattern, public_key, fingerprint, comment, added_at FROM host_cas")?;
        let rows = s.query_map([], |r| {
            Ok(HostCaRow { id: r.get(0)?, pattern: r.get(1)?, public_key: r.get(2)?, fingerprint: r.get(3)?, comment: r.get(4)?, added_at: r.get(5)? })
        })?;
        rows.collect::<Result<_, _>>()?
    };
    let known_hosts: Vec<KnownHostRow> = {
        let mut s = conn.prepare("SELECT host, port, algorithm, public_key, fingerprint, added_at FROM known_hosts")?;
        let rows = s.query_map([], |r| {
            Ok(KnownHostRow { host: r.get(0)?, port: r.get(1)?, algorithm: r.get(2)?, public_key: r.get(3)?, fingerprint: r.get(4)?, added_at: r.get(5)? })
        })?;
        rows.collect::<Result<_, _>>()?
    };
    let tombstones: Vec<Tombstone> = {
        let mut s = conn.prepare("SELECT kind, id, deleted_at FROM sync_tombstones")?;
        let rows = s.query_map([], |r| Ok(Tombstone { kind: r.get(0)?, id: r.get(1)?, deleted_at: r.get(2)? }))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut secret_rows = Vec::new();
    if include_secrets {
        for s in &servers {
            if let Some(v) = secrets.get(&password_account(&s.id))? {
                secret_rows.push(SecretRow { server_id: s.id.clone(), kind: "password".into(), value: v.to_string() });
            }
            if let Some(v) = secrets.get(&passphrase_account(&s.id))? {
                secret_rows.push(SecretRow { server_id: s.id.clone(), kind: "passphrase".into(), value: v.to_string() });
            }
        }
    }
    Ok(Bundle {
        format: FORMAT,
        app: "yoru".into(),
        device_id: device_id(conn)?,
        device_name: device_name.chars().take(80).collect(),
        created_at: crate::database::now(),
        servers,
        snippets,
        forwards,
        host_cas,
        known_hosts,
        tombstones,
        secrets: secret_rows,
    })
}

// ---------------------------------------------------------------- merge

fn local_tomb(conn: &Connection, kind: &str, id: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT deleted_at FROM sync_tombstones WHERE kind=?1 AND id=?2", params![kind, id], |r| r.get(0))
        .optional()?)
}

fn updated_at(conn: &Connection, table: &str, id: &str) -> AppResult<Option<String>> {
    let col = if table == "host_cas" { "added_at" } else { "COALESCE(updated_at, created_at)" };
    Ok(conn.query_row(&format!("SELECT {col} FROM {table} WHERE id=?1"), [id], |r| r.get(0)).optional()?)
}

/// Should a remote row with `remote_ts` be skipped because of a newer local deletion?
fn deleted_after(conn: &Connection, kind: &str, id: &str, remote_ts: &str) -> AppResult<bool> {
    Ok(local_tomb(conn, kind, id)?.is_some_and(|d| !newer(remote_ts, &d)))
}

pub fn apply(conn: &Connection, store: &dyn SecretStore, b: &Bundle) -> AppResult<MergeStats> {
    if b.format != FORMAT || b.app != "yoru" {
        return Err(crate::error::AppError::Validation("This file was made by an unsupported yoru version".into()));
    }
    let mut st = MergeStats::default();
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON;")?;
    let mut deleted_servers: Vec<String> = Vec::new();

    // 1. Deletions.
    for t in &b.tombstones {
        let existing = local_tomb(&tx, &t.kind, &t.id)?;
        if existing.as_ref().is_none_or(|e| newer(&t.deleted_at, e)) {
            tx.execute(
                "INSERT INTO sync_tombstones (kind, id, deleted_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(kind, id) DO UPDATE SET deleted_at = excluded.deleted_at",
                params![t.kind, t.id, t.deleted_at],
            )?;
        }
        let table = match t.kind.as_str() {
            "server" => Some("servers"),
            "snippet" => Some("snippets"),
            "forward" => Some("port_forwards"),
            "host_ca" => Some("host_cas"),
            _ => None,
        };
        if let Some(table) = table {
            if let Some(local_ts) = updated_at(&tx, table, &t.id)? {
                if !newer(&local_ts, &t.deleted_at) {
                    tx.execute(&format!("DELETE FROM {table} WHERE id=?1"), [&t.id])?;
                    st.deleted += 1;
                    if t.kind == "server" {
                        deleted_servers.push(t.id.clone());
                    }
                }
            }
        } else if t.kind == "known_host" {
            if let Some((host, port)) = t.id.rsplit_once(':') {
                let newest: Option<String> =
                    tx.query_row("SELECT MAX(added_at) FROM known_hosts WHERE host=?1 AND port=?2", params![host, port], |r| r.get(0))?;
                if newest.is_some_and(|n| !newer(&n, &t.deleted_at)) {
                    tx.execute("DELETE FROM known_hosts WHERE host=?1 AND port=?2", params![host, port])?;
                    st.deleted += 1;
                }
            }
        }
    }

    // 2. Servers (per-device fields kept).
    for s in &b.servers {
        if deleted_after(&tx, "server", &s.id, &s.updated_at)? {
            continue;
        }
        match updated_at(&tx, "servers", &s.id)? {
            None => {
                tx.execute(
                    "INSERT INTO servers (id, name, host, port, username, auth_method, key_path, profile_kind, group_name, tags, notes,
                       favorite, auto_reconnect, keepalive, jump_host_id, os_detected, created_at, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
                    params![s.id, s.name, s.host, s.port, s.username, s.auth_method, s.key_path, s.profile_kind, s.group_name, s.tags, s.notes,
                        s.favorite, s.auto_reconnect, s.keepalive, s.jump_host_id, s.os_detected, s.created_at, s.updated_at],
                )?;
                st.added += 1;
            }
            Some(local) if newer(&s.updated_at, &local) => {
                tx.execute(
                    "UPDATE servers SET name=?2, host=?3, port=?4, username=?5, auth_method=?6, key_path=?7, profile_kind=?8, group_name=?9,
                       tags=?10, notes=?11, favorite=?12, auto_reconnect=?13, keepalive=?14, jump_host_id=?15, updated_at=?16,
                       os_detected=COALESCE(os_detected, ?17) WHERE id=?1",
                    params![s.id, s.name, s.host, s.port, s.username, s.auth_method, s.key_path, s.profile_kind, s.group_name, s.tags,
                        s.notes, s.favorite, s.auto_reconnect, s.keepalive, s.jump_host_id, s.updated_at, s.os_detected],
                )?;
                st.updated += 1;
            }
            _ => {}
        }
    }

    // 3. Snippets (usage stats stay per device).
    for s in &b.snippets {
        if deleted_after(&tx, "snippet", &s.id, &s.updated_at)? {
            continue;
        }
        match updated_at(&tx, "snippets", &s.id)? {
            None => {
                tx.execute(
                    "INSERT INTO snippets (id, title, description, command, category, tags, favorite, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![s.id, s.title, s.description, s.command, s.category, s.tags, s.favorite, s.created_at, s.updated_at],
                )?;
                st.added += 1;
            }
            Some(local) if newer(&s.updated_at, &local) => {
                tx.execute(
                    "UPDATE snippets SET title=?2, description=?3, command=?4, category=?5, tags=?6, favorite=?7, updated_at=?8 WHERE id=?1",
                    params![s.id, s.title, s.description, s.command, s.category, s.tags, s.favorite, s.updated_at],
                )?;
                st.updated += 1;
            }
            _ => {}
        }
    }

    // 4. Port forwards.
    for f in &b.forwards {
        if deleted_after(&tx, "forward", &f.id, &f.updated_at)? || deleted_after(&tx, "server", &f.server_id, &f.updated_at)? {
            continue;
        }
        match updated_at(&tx, "port_forwards", &f.id)? {
            None => {
                tx.execute(
                    "INSERT INTO port_forwards (id, server_id, label, local_port, remote_host, remote_port, auto_start, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![f.id, f.server_id, f.label, f.local_port, f.remote_host, f.remote_port, f.auto_start, f.created_at, f.updated_at],
                )?;
                st.added += 1;
            }
            Some(local) if newer(&f.updated_at, &local) => {
                tx.execute(
                    "UPDATE port_forwards SET label=?2, local_port=?3, remote_host=?4, remote_port=?5, auto_start=?6, updated_at=?7 WHERE id=?1",
                    params![f.id, f.label, f.local_port, f.remote_host, f.remote_port, f.auto_start, f.updated_at],
                )?;
                st.updated += 1;
            }
            _ => {}
        }
    }

    // 5. Certificate authorities (immutable; add or delete only).
    for c in &b.host_cas {
        if deleted_after(&tx, "host_ca", &c.id, &c.added_at)? || updated_at(&tx, "host_cas", &c.id)?.is_some() {
            continue;
        }
        tx.execute(
            "INSERT INTO host_cas (id, pattern, public_key, fingerprint, comment, added_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![c.id, c.pattern, c.public_key, c.fingerprint, c.comment, c.added_at],
        )?;
        st.added += 1;
    }

    // 6. Trusted host keys: per host:port, the newest set wins.
    let mut groups: HashMap<(String, i64), Vec<&KnownHostRow>> = HashMap::new();
    for k in &b.known_hosts {
        groups.entry((k.host.clone(), k.port)).or_default().push(k);
    }
    for ((host, port), rows) in groups {
        let remote_newest = rows.iter().map(|r| r.added_at.as_str()).fold("", |acc, x| if acc.is_empty() || newer(x, acc) { x } else { acc });
        if deleted_after(&tx, "known_host", &format!("{host}:{port}"), remote_newest)? {
            continue;
        }
        let local_newest: Option<String> = tx.query_row("SELECT MAX(added_at) FROM known_hosts WHERE host=?1 AND port=?2", params![host, port], |r| r.get(0))?;
        if local_newest.as_deref().is_none_or(|l| newer(remote_newest, l)) {
            let existed = local_newest.is_some();
            tx.execute("DELETE FROM known_hosts WHERE host=?1 AND port=?2", params![host, port])?;
            for r in rows {
                tx.execute(
                    "INSERT OR IGNORE INTO known_hosts (host, port, algorithm, public_key, fingerprint, added_at) VALUES (?1,?2,?3,?4,?5,?6)",
                    params![r.host, r.port, r.algorithm, r.public_key, r.fingerprint, r.added_at],
                )?;
            }
            if existed {
                st.updated += 1;
            } else {
                st.added += 1;
            }
        }
    }

    // Jump hosts pointing at servers that no longer exist -> direct.
    tx.execute("UPDATE servers SET jump_host_id = NULL WHERE jump_host_id IS NOT NULL AND jump_host_id NOT IN (SELECT id FROM servers)", [])?;
    tx.commit()?;

    // 7. Secrets (outside SQLite): only for servers that exist here and where
    //    the remote profile is at least as new as ours.
    for id in &deleted_servers {
        let _ = crate::credentials::delete_server_secrets(store, id);
    }
    for s in &b.secrets {
        let Some(remote_server) = b.servers.iter().find(|x| x.id == s.server_id) else { continue };
        let Some(local_ts) = updated_at(conn, "servers", &s.server_id)? else { continue };
        let account = match s.kind.as_str() {
            "password" => password_account(&s.server_id),
            "passphrase" => passphrase_account(&s.server_id),
            _ => continue,
        };
        let have = store.get(&account)?;
        if have.is_none() || !newer(&local_ts, &remote_server.updated_at) {
            if have.as_deref().map(|h| h.as_str()) != Some(s.value.as_str()) {
                store.set(&account, &s.value)?;
                st.secrets += 1;
            }
        }
    }
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryStore;
    use crate::database::{known_hosts, servers, snippets, Database};

    fn pc() -> (Database, MemoryStore) {
        (Database::open_in_memory().unwrap(), MemoryStore::default())
    }

    fn sync_into(from: &(Database, MemoryStore), to: &(Database, MemoryStore), secrets: bool) -> MergeStats {
        let b = from.0.with(|c| export(c, &from.1, secrets, "pc")).unwrap();
        // Through JSON, exactly as files do.
        let b: Bundle = serde_json::from_slice(&serde_json::to_vec(&b).unwrap()).unwrap();
        to.0.with(|c| apply(c, &to.1, &b)).unwrap()
    }

    fn sleep() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    #[test]
    fn two_pcs_converge_edits_and_deletes() {
        let a = pc();
        let b = pc();
        let s = a.0.with(|c| servers::insert(c, &servers::sample_input())).unwrap();
        a.1.set(&password_account(&s.id), "pw").unwrap();
        a.0.with(|c| known_hosts::add(c, "10.0.0.5", 22, "ssh-ed25519", "ssh-ed25519 AAA", "SHA256:a")).unwrap();

        // A -> B without secrets.
        let st = sync_into(&a, &b, false);
        assert!(st.added >= 2);
        assert_eq!(b.0.with(|c| servers::get(c, &s.id)).unwrap().input.name, "web-01");
        assert!(b.1.get(&password_account(&s.id)).unwrap().is_none(), "secrets not synced unless opted in");
        assert_eq!(b.0.with(|c| known_hosts::list_for(c, "10.0.0.5", 22)).unwrap().len(), 1);

        // Re-applying the same bundle changes nothing (idempotent).
        assert_eq!(sync_into(&a, &b, false), MergeStats::default());

        // Edit on B, sync back to A: newer wins.
        sleep();
        let mut i = servers::sample_input();
        i.name = "web-renamed".into();
        b.0.with(|c| servers::update(c, &s.id, &i)).unwrap();
        sync_into(&b, &a, false);
        assert_eq!(a.0.with(|c| servers::get(c, &s.id)).unwrap().input.name, "web-renamed");

        // Older edit does not overwrite newer.
        sync_into(&a, &b, false);
        assert_eq!(b.0.with(|c| servers::get(c, &s.id)).unwrap().input.name, "web-renamed");

        // Delete on A propagates to B (and its secrets there).
        b.1.set(&password_account(&s.id), "b-pw").unwrap();
        sleep();
        a.0.with(|c| servers::delete(c, &s.id)).unwrap();
        let st = sync_into(&a, &b, false);
        assert_eq!(st.deleted, 1);
        assert!(b.0.with(|c| servers::get(c, &s.id)).is_err());
        assert!(b.1.get(&password_account(&s.id)).unwrap().is_none());
        // ...and B doesn't resurrect it on A.
        sync_into(&b, &a, false);
        assert!(a.0.with(|c| servers::get(c, &s.id)).is_err());
    }

    #[test]
    fn edit_after_delete_wins() {
        let a = pc();
        let b = pc();
        let s = a.0.with(|c| servers::insert(c, &servers::sample_input())).unwrap();
        sync_into(&a, &b, false);
        a.0.with(|c| servers::delete(c, &s.id)).unwrap();
        sleep();
        let mut i = servers::sample_input();
        i.name = "edited later".into();
        b.0.with(|c| servers::update(c, &s.id, &i)).unwrap();
        sync_into(&a, &b, false); // tombstone older than B's edit
        assert_eq!(b.0.with(|c| servers::get(c, &s.id)).unwrap().input.name, "edited later");
        sync_into(&b, &a, false);
        assert_eq!(a.0.with(|c| servers::get(c, &s.id)).unwrap().input.name, "edited later", "re-created on A");
    }

    #[test]
    fn secrets_only_when_opted_in() {
        let a = pc();
        let b = pc();
        let s = a.0.with(|c| servers::insert(c, &servers::sample_input())).unwrap();
        a.1.set(&password_account(&s.id), "hunter2").unwrap();
        let st = sync_into(&a, &b, true);
        assert_eq!(st.secrets, 1);
        assert_eq!(b.1.get(&password_account(&s.id)).unwrap().unwrap().as_str(), "hunter2");
    }

    #[test]
    fn snippets_and_jump_references() {
        let a = pc();
        let b = pc();
        let j = a.0.with(|c| servers::insert(c, &servers::sample_input())).unwrap();
        let mut ti = servers::sample_input();
        ti.jump_host_id = Some(j.id.clone());
        let t = a.0.with(|c| servers::insert(c, &ti)).unwrap();
        sync_into(&a, &b, false);
        assert_eq!(b.0.with(|c| servers::get(c, &t.id)).unwrap().input.jump_host_id.as_deref(), Some(j.id.as_str()));
        // Seeded snippets exist on both with the same ids: no duplicates.
        let na = a.0.with(snippets::list).unwrap().len();
        assert_eq!(b.0.with(snippets::list).unwrap().len(), na);
    }

    #[test]
    fn timestamps_compare_by_time_not_text() {
        assert!(newer("2026-01-01T00:00:00.001Z", "2026-01-01T00:00:00Z"));
        assert!(!newer("2026-01-01T00:00:00Z", "2026-01-01T00:00:00.001Z"));
    }
}
