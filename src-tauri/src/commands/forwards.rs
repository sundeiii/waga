use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::database::forwards::{self, ForwardInput, SavedForward};
use crate::error::AppResult;
use crate::security::validation::validate_host;
use crate::ssh::forwarding::{ForwardSink, ForwardStatus};
use crate::state::AppState;

pub const FORWARD_EVENT: &str = "forward://update";

struct EventSink(AppHandle);
impl ForwardSink for EventSink {
    fn update(&self, s: &ForwardStatus) {
        let _ = self.0.emit(FORWARD_EVENT, s);
    }
}

#[tauri::command]
pub fn list_saved_forwards(state: State<'_, AppState>, server_id: String) -> AppResult<Vec<SavedForward>> {
    state.db.with(|c| forwards::list_for(c, &server_id))
}

#[tauri::command]
pub fn save_forward(state: State<'_, AppState>, server_id: String, id: Option<String>, input: ForwardInput) -> AppResult<SavedForward> {
    state.db.with(|c| match &id {
        Some(id) => forwards::update(c, id, &input),
        None => forwards::insert(c, &server_id, &input),
    })
}

#[tauri::command]
pub fn delete_saved_forward(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    // Stop it first if it's running.
    for f in state.forwards.list().into_iter().filter(|f| f.saved_id.as_deref() == Some(id.as_str())) {
        state.forwards.stop(&f.id, &EventSink(app.clone()));
    }
    state.db.with(|c| forwards::delete(c, &id))
}

/// Start a saved forward (by id) or an ad-hoc one.
#[tauri::command]
pub async fn start_forward(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    saved_id: Option<String>,
    local_port: Option<u16>,
    remote_host: Option<String>,
    remote_port: Option<u16>,
) -> AppResult<ForwardStatus> {
    let (lp, host, rp) = match &saved_id {
        Some(id) => {
            let f = state.db.with(|c| forwards::get(c, id))?;
            (f.input.local_port, f.input.remote_host, f.input.remote_port)
        }
        None => {
            let host = remote_host.unwrap_or_default();
            validate_host(&host).map_err(crate::error::AppError::Validation)?;
            (local_port.unwrap_or(0), host, remote_port.unwrap_or(0))
        }
    };
    if rp == 0 {
        return Err(crate::error::AppError::Validation("Destination port is required".into()));
    }
    state
        .forwards
        .start(state.connections.clone(), Arc::new(EventSink(app)), &server_id, saved_id, lp, &host, rp)
        .await
}

#[tauri::command]
pub fn stop_forward(app: AppHandle, state: State<'_, AppState>, id: String) -> bool {
    state.forwards.stop(&id, &EventSink(app))
}

#[tauri::command]
pub fn list_active_forwards(state: State<'_, AppState>) -> Vec<ForwardStatus> {
    state.forwards.list()
}
