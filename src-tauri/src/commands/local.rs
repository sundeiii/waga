use std::sync::Arc;

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::local_term::profiles::{load_profiles, LocalProfile};
use crate::local_term::session::run_local;
use crate::ssh::events::{SessionEvent, SessionSink};
use crate::state::AppState;

struct Sink {
    events: Channel<SessionEvent>,
    output: Channel<InvokeResponseBody>,
}

impl SessionSink for Sink {
    fn event(&self, e: SessionEvent) {
        let _ = self.events.send(e);
    }
    fn output(&self, b: Vec<u8>) {
        let _ = self.output.send(InvokeResponseBody::Raw(b));
    }
}

#[tauri::command]
pub async fn local_profiles() -> Vec<LocalProfile> {
    tokio::task::spawn_blocking(load_profiles).await.unwrap_or_default()
}

/// Start a local shell. Only a profile id is accepted: the command line is
/// resolved from the user's own Windows Terminal settings, never from the webview.
#[tauri::command]
pub async fn local_open(
    state: State<'_, AppState>,
    session_id: String,
    profile_id: String,
    cols: u32,
    rows: u32,
    events: Channel<SessionEvent>,
    output: Channel<InvokeResponseBody>,
) -> AppResult<()> {
    uuid::Uuid::parse_str(&session_id).map_err(|_| AppError::Validation("Invalid session id".into()))?;
    let profile = tokio::task::spawn_blocking(load_profiles)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| AppError::NotFound("Local terminal profile".into()))?;
    let commands = state.sessions.register(&session_id)?;
    let manager = state.sessions.clone();
    let sink: Arc<dyn SessionSink> = Arc::new(Sink { events, output });
    tokio::spawn(async move {
        run_local(profile, cols, rows, sink, commands).await;
        manager.remove(&session_id);
    });
    Ok(())
}

/// Close every running session (SSH and local). Called when the window
/// (re)loads so sessions orphaned by a reload don't linger.
#[tauri::command]
pub async fn sessions_close_all(state: State<'_, AppState>) -> AppResult<usize> {
    let senders = state.sessions.all_senders();
    let n = senders.len();
    for tx in senders {
        let _ = tx.send(crate::ssh::session::SessionCommand::Close).await;
    }
    Ok(n)
}
