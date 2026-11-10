use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::error::AppResult;
use crate::sftp::transfer::{plan_download, plan_upload, TransferEvent, TransferSink};
use crate::sftp::{local, remote, Listing};
use crate::state::AppState;

pub const TRANSFER_EVENT: &str = "sftp://transfer";

struct EventSink(AppHandle);
impl TransferSink for EventSink {
    fn update(&self, e: &TransferEvent) {
        let _ = self.0.emit(TRANSFER_EVENT, e);
    }
}

async fn session(state: &State<'_, AppState>, server_id: &str) -> AppResult<Arc<russh_sftp::client::SftpSession>> {
    state.sftp.get(server_id, &state.connections).await
}

/// Run an SFTP op; on a transport-level failure drop the cached session and retry once.
macro_rules! with_sftp {
    ($state:expr, $server:expr, |$s:ident| $body:expr) => {{
        let $s = session(&$state, &$server).await?;
        match $body {
            Err(crate::error::AppError::Ssh(m)) if m.starts_with("SFTP error") => {
                $state.sftp.invalidate(&$server).await;
                let $s = session(&$state, &$server).await?;
                $body
            }
            other => other,
        }
    }};
}

#[tauri::command]
pub async fn sftp_home(state: State<'_, AppState>, server_id: String) -> AppResult<String> {
    with_sftp!(state, server_id, |s| remote::home(&s).await)
}

#[tauri::command]
pub async fn sftp_list(state: State<'_, AppState>, server_id: String, path: String) -> AppResult<Listing> {
    with_sftp!(state, server_id, |s| remote::list(&s, &path).await)
}

#[tauri::command]
pub async fn sftp_mkdir(state: State<'_, AppState>, server_id: String, dir: String, name: String) -> AppResult<String> {
    with_sftp!(state, server_id, |s| remote::mkdir(&s, &dir, &name).await)
}

#[tauri::command]
pub async fn sftp_rename(state: State<'_, AppState>, server_id: String, dir: String, from: String, to: String) -> AppResult<()> {
    with_sftp!(state, server_id, |s| remote::rename(&s, &dir, &from, &to).await)
}

#[tauri::command]
pub async fn sftp_delete(state: State<'_, AppState>, server_id: String, dir: String, name: String, is_dir: bool) -> AppResult<()> {
    with_sftp!(state, server_id, |s| remote::delete(&s, &dir, &name, is_dir).await)
}

#[tauri::command]
pub async fn sftp_upload(app: AppHandle, state: State<'_, AppState>, server_id: String, local_path: String, remote_dir: String, overwrite: bool) -> AppResult<String> {
    let s = session(&state, &server_id).await?;
    let plan = plan_upload(&s, &server_id, &PathBuf::from(local_path), &remote_dir, overwrite).await?;
    Ok(state.transfers.start(s, plan, Arc::new(EventSink(app))))
}

#[tauri::command]
pub async fn sftp_download(app: AppHandle, state: State<'_, AppState>, server_id: String, remote_path: String, local_dir: String, overwrite: bool) -> AppResult<String> {
    let s = session(&state, &server_id).await?;
    let plan = plan_download(&s, &server_id, &remote_path, &PathBuf::from(local_dir), overwrite).await?;
    Ok(state.transfers.start(s, plan, Arc::new(EventSink(app))))
}

#[tauri::command]
pub fn sftp_cancel(state: State<'_, AppState>, id: String) -> bool {
    state.transfers.cancel(&id)
}

// ---- local side ----

#[tauri::command]
pub fn local_home() -> String {
    local::home()
}

#[tauri::command]
pub async fn local_list(path: String) -> AppResult<Listing> {
    tokio::task::spawn_blocking(move || local::list(&path)).await.map_err(|e| crate::error::AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub fn local_mkdir(dir: String, name: String) -> AppResult<String> {
    local::mkdir(&dir, &name)
}

#[tauri::command]
pub fn local_rename(dir: String, from: String, to: String) -> AppResult<()> {
    local::rename(&dir, &from, &to)
}

#[tauri::command]
pub async fn local_delete(dir: String, name: String) -> AppResult<()> {
    tokio::task::spawn_blocking(move || local::delete(&dir, &name)).await.map_err(|e| crate::error::AppError::Internal(e.to_string()))?
}
