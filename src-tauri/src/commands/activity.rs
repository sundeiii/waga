use tauri::State;

use crate::database::activity::{self, ActivityEntry};
use crate::database::host_cas::{self, HostCa};
use crate::database::known_hosts::{self, KnownHost};
use crate::error::{AppError, AppResult};
use crate::security::host_certs::ca_fingerprint;
use crate::state::AppState;

#[tauri::command]
pub fn list_activity(state: State<'_, AppState>, limit: Option<u32>) -> AppResult<Vec<ActivityEntry>> {
    let limit = limit.unwrap_or(500).clamp(1, 5000);
    state.db.with(|c| activity::list(c, limit))
}

#[tauri::command]
pub fn clear_activity(state: State<'_, AppState>) -> AppResult<()> {
    state.db.with(activity::clear)
}

#[tauri::command]
pub fn list_known_hosts(state: State<'_, AppState>) -> AppResult<Vec<KnownHost>> {
    state.db.with(known_hosts::list_all)
}

#[tauri::command]
pub fn list_host_cas(state: State<'_, AppState>) -> AppResult<Vec<HostCa>> {
    state.db.with(host_cas::list)
}

/// Trust a CA (OpenSSH public key line) for hosts matching `pattern`.
#[tauri::command]
pub fn add_host_ca(state: State<'_, AppState>, pattern: String, public_key: String, comment: String) -> AppResult<HostCa> {
    let line = public_key.trim();
    if line.contains("-cert-v01@openssh.com") {
        return Err(AppError::Validation("Paste the CA's public key, not a certificate".into()));
    }
    let fp = ca_fingerprint(line).map_err(AppError::Validation)?;
    let canonical = crate::security::host_keys::canonical_key(line);
    state.db.with(|c| host_cas::insert(c, &pattern, &canonical, &fp, &comment))
}

#[tauri::command]
pub fn remove_host_ca(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.with(|c| host_cas::delete(c, &id))
}

#[tauri::command]
pub fn remove_known_host(state: State<'_, AppState>, host: String, port: u16) -> AppResult<usize> {
    state.db.with(|c| known_hosts::remove(c, &host, port))
}
