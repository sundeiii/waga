//! Remote side: one SFTP subsystem channel per server, opened on the
//! server's live SSH connection and reused until that connection changes.

use std::collections::HashMap;
use std::sync::Arc;

use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileType, StatusCode};
use tokio::sync::Mutex;

use super::paths::{remote_join, remote_parent, validate_name};
use super::{mode_string, sort_and_cap, EntryKind, FileEntry, Listing};
use crate::error::{AppError, AppResult};
use crate::ssh::connections::{ConnectionRegistry, SshHandle};

pub fn sftp_err(e: SftpError) -> AppError {
    match &e {
        SftpError::Status(s) => {
            let msg = match s.status_code {
                StatusCode::NoSuchFile => "No such file or directory".to_string(),
                StatusCode::PermissionDenied => "Permission denied".to_string(),
                StatusCode::Failure if !s.error_message.is_empty() => s.error_message.clone(),
                _ if !s.error_message.is_empty() => s.error_message.clone(),
                code => code.to_string(),
            };
            AppError::Ssh(msg)
        }
        _ => AppError::Ssh(format!("SFTP error: {e}")),
    }
}

pub fn is_not_found(e: &SftpError) -> bool {
    matches!(e, SftpError::Status(s) if s.status_code == StatusCode::NoSuchFile)
}

#[derive(Default)]
pub struct SftpPool {
    sessions: Mutex<HashMap<String, (Arc<SshHandle>, Arc<SftpSession>)>>,
}

impl SftpPool {
    pub async fn get(&self, server_id: &str, connections: &ConnectionRegistry) -> AppResult<Arc<SftpSession>> {
        let handle = connections
            .for_server(server_id)
            .ok_or_else(|| AppError::Ssh("SFTP uses your open SSH session. Connect to this server in the Terminal first.".into()))?;
        let mut map = self.sessions.lock().await;
        if let Some((h, s)) = map.get(server_id) {
            if Arc::ptr_eq(h, &handle) {
                return Ok(s.clone());
            }
        }
        let ch = handle.channel_open_session().await.map_err(|e| AppError::Ssh(format!("Cannot open SFTP channel: {e}")))?;
        ch.request_subsystem(true, "sftp").await.map_err(|e| AppError::Ssh(format!("Server refused SFTP: {e}")))?;
        // More requests in flight = better throughput on high-latency links
        // (VPN, remote datacenters). Measured on loopback (debug build):
        // ~110 MiB/s up, ~72 MiB/s down with the defaults.
        let cfg = russh_sftp::client::Config { max_concurrent_reads: 32, max_concurrent_writes: 32, ..Default::default() };
        let sftp = SftpSession::new_with_config(ch.into_stream(), cfg)
            .await
            .map_err(|e| AppError::Ssh(format!("SFTP is not available on this server: {e}")))?;
        sftp.set_timeout(30);
        let sftp = Arc::new(sftp);
        map.insert(server_id.to_string(), (handle, sftp.clone()));
        Ok(sftp)
    }

    pub async fn invalidate(&self, server_id: &str) {
        self.sessions.lock().await.remove(server_id);
    }
}

fn entry_from(name: String, path: String, attrs: &russh_sftp::protocol::FileAttributes) -> FileEntry {
    let kind = match attrs.file_type() {
        FileType::Dir => EntryKind::Dir,
        FileType::File => EntryKind::File,
        FileType::Symlink => EntryKind::Symlink,
        FileType::Other => EntryKind::Other,
    };
    FileEntry {
        hidden: name.starts_with('.'),
        is_dir: kind == EntryKind::Dir,
        name,
        path,
        kind,
        size: if kind == EntryKind::File { attrs.size } else { None },
        modified: attrs.mtime.map(i64::from),
        permissions: attrs.permissions.map(mode_string),
    }
}

pub async fn home(sftp: &SftpSession) -> AppResult<String> {
    sftp.canonicalize(".").await.map_err(sftp_err)
}

pub async fn list(sftp: &SftpSession, path: &str) -> AppResult<Listing> {
    let path = sftp.canonicalize(path).await.map_err(sftp_err)?;
    let dir = sftp.read_dir(path.clone()).await.map_err(sftp_err)?;
    let mut entries = Vec::new();
    let mut links = Vec::new();
    for d in dir {
        let name = d.file_name();
        let full = remote_join(&path, &name).unwrap_or_else(|_| format!("{path}/{name}"));
        let e = entry_from(name, full, &d.metadata());
        if e.kind == EntryKind::Symlink {
            links.push(entries.len());
        }
        entries.push(e);
    }
    // Resolve symlink targets (bounded) so links to directories are navigable.
    for &i in links.iter().take(200) {
        if let Ok(m) = sftp.metadata(entries[i].path.clone()).await {
            entries[i].is_dir = m.is_dir();
            if !m.is_dir() {
                entries[i].size = m.size;
            }
        }
    }
    let (entries, truncated, total) = sort_and_cap(entries);
    let parent = (path != "/").then(|| remote_parent(&path));
    Ok(Listing { path, parent, entries, truncated, total })
}

pub async fn mkdir(sftp: &SftpSession, dir: &str, name: &str) -> AppResult<String> {
    let p = remote_join(dir, name).map_err(AppError::Validation)?;
    sftp.create_dir(p.clone()).await.map_err(sftp_err)?;
    Ok(p)
}

pub async fn rename(sftp: &SftpSession, dir: &str, from: &str, to: &str) -> AppResult<()> {
    let a = remote_join(dir, from).map_err(AppError::Validation)?;
    let b = remote_join(dir, to).map_err(AppError::Validation)?;
    if sftp.try_exists(b.clone()).await.unwrap_or(false) {
        return Err(AppError::Exists(format!("'{to}' already exists")));
    }
    sftp.rename(a, b).await.map_err(sftp_err)
}

/// Deletes a file, or an EMPTY directory (no recursive delete).
pub async fn delete(sftp: &SftpSession, dir: &str, name: &str, is_dir: bool) -> AppResult<()> {
    validate_name(name).map_err(AppError::Validation)?;
    let p = remote_join(dir, name).map_err(AppError::Validation)?;
    if is_dir {
        sftp.remove_dir(p).await.map_err(|e| match sftp_err(e) {
            AppError::Ssh(m) if m.to_lowercase().contains("failure") || m.to_lowercase().contains("not empty") => {
                AppError::Validation("Folder is not empty. Only empty folders can be deleted.".into())
            }
            other => other,
        })
    } else {
        sftp.remove_file(p).await.map_err(sftp_err)
    }
}
