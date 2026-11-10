use tauri::{AppHandle, Emitter, State};

use crate::database::servers;
use crate::error::AppResult;
use crate::monitoring::manager::{MonitorDeps, MonitorSink, MonitorStatus};
use crate::monitoring::Sample;
use crate::state::AppState;

pub const SAMPLE_EVENT: &str = "monitor://sample";
pub const STATUS_EVENT: &str = "monitor://status";

/// Broadcasts to the webview so the dashboard and server cards can both listen.
struct EventSink(AppHandle);

impl MonitorSink for EventSink {
    fn sample(&self, sample: &Sample) {
        let _ = self.0.emit(SAMPLE_EVENT, sample);
    }
    fn status(&self, status: &MonitorStatus) {
        let _ = self.0.emit(STATUS_EVENT, status);
    }
}

/// async so it runs inside the Tokio runtime (the monitor task is spawned there).
#[tauri::command]
pub async fn monitor_start(app: AppHandle, state: State<'_, AppState>, server_id: String) -> AppResult<()> {
    state.db.with(|c| servers::get(c, &server_id))?; // must exist
    state.monitors.start(
        &server_id,
        MonitorDeps { db: state.db.clone(), connections: state.connections.clone(), sink: std::sync::Arc::new(EventSink(app)) },
    );
    Ok(())
}

#[tauri::command]
pub fn monitor_stop(app: AppHandle, state: State<'_, AppState>, server_id: String) {
    state.monitors.stop(&server_id, &EventSink(app));
}

#[tauri::command]
pub fn monitor_active(state: State<'_, AppState>) -> Vec<String> {
    state.monitors.active()
}
