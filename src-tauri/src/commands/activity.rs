use tauri::State;

use crate::database::activity::{self, ActivityEntry};
use crate::database::known_hosts::{self, KnownHost};
use crate::error::AppResult;
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
pub fn remove_known_host(state: State<'_, AppState>, host: String, port: u16) -> AppResult<usize> {
    state.db.with(|c| known_hosts::remove(c, &host, port))
}
