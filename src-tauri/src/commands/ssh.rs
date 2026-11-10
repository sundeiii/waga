use std::time::Duration;

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;
use zeroize::Zeroizing;

use crate::database::{servers, settings};
use crate::error::{AppError, AppResult};
use crate::security::host_keys::HostKeyAnswer;
use crate::ssh::events::{SessionEvent, SessionSink};
use crate::ssh::prompts::SecretAnswer;
use crate::ssh::session::{run_session, SessionCommand, SessionContext};
use crate::state::AppState;

/// Streams session events (JSON) and raw terminal bytes to the webview
/// over ordered Tauri IPC channels.
struct TauriSink {
    events: Channel<SessionEvent>,
    output: Channel<InvokeResponseBody>,
}

impl SessionSink for TauriSink {
    fn event(&self, event: SessionEvent) {
        let _ = self.events.send(event);
    }
    fn output(&self, bytes: Vec<u8>) {
        let _ = self.output.send(InvokeResponseBody::Raw(bytes));
    }
}

const PROMPT_TIMEOUT: Duration = Duration::from_secs(300);

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn ssh_open(
    state: State<'_, AppState>,
    session_id: String,
    server_id: String,
    cols: u32,
    rows: u32,
    events: Channel<SessionEvent>,
    output: Channel<InvokeResponseBody>,
) -> AppResult<()> {
    uuid::Uuid::parse_str(&session_id).map_err(|_| AppError::Validation("Invalid session id".into()))?;
    let profile = state.db.with(|c| servers::get(c, &server_id))?;
    let cfg = state.db.with(settings::load)?.connections;
    let commands = state.sessions.register(&session_id)?;

    let keepalive = (profile.input.keepalive && cfg.keepalive_secs > 0).then(|| Duration::from_secs(cfg.keepalive_secs as u64));
    let ctx = SessionContext {
        sid: session_id.clone(),
        profile,
        cols: cols.clamp(10, 1000),
        rows: rows.clamp(2, 500),
        connect_timeout: Duration::from_secs(cfg.timeout_secs as u64),
        keepalive,
        prompt_timeout: PROMPT_TIMEOUT,
        db: state.db.clone(),
        secrets: state.secrets.clone(),
        prompts: state.sessions.prompts.clone(),
        sink: std::sync::Arc::new(TauriSink { events, output }),
        connections: state.connections.clone(),
    };
    let manager = state.sessions.clone();
    tauri::async_runtime::spawn(async move {
        run_session(ctx, commands).await;
        manager.remove(&session_id);
    });
    Ok(())
}

async fn send(state: &State<'_, AppState>, sid: &str, cmd: SessionCommand) -> AppResult<()> {
    let tx = state
        .sessions
        .sender(sid)
        .ok_or_else(|| AppError::NotFound("Session is not active".into()))?;
    tx.send(cmd).await.map_err(|_| AppError::NotFound("Session is not active".into()))
}

#[tauri::command]
pub async fn ssh_write(state: State<'_, AppState>, session_id: String, data: String) -> AppResult<()> {
    send(&state, &session_id, SessionCommand::Data(data.into_bytes())).await
}

#[tauri::command]
pub async fn ssh_resize(state: State<'_, AppState>, session_id: String, cols: u32, rows: u32) -> AppResult<()> {
    if !(1..=1000).contains(&cols) || !(1..=500).contains(&rows) {
        return Err(AppError::Validation("Invalid terminal size".into()));
    }
    send(&state, &session_id, SessionCommand::Resize { cols, rows }).await
}

#[tauri::command]
pub async fn ssh_close(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.sessions.prompts.cancel(&session_id);
    match send(&state, &session_id, SessionCommand::Close).await {
        Ok(()) | Err(AppError::NotFound(_)) => Ok(()),
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub fn ssh_host_key_decision(state: State<'_, AppState>, session_id: String, accept: bool, acknowledge_changed: bool) -> AppResult<()> {
    state
        .sessions
        .prompts
        .answer_host_key(&session_id, HostKeyAnswer { accept, acknowledge_changed })
}

/// `secret: None` cancels the prompt (and therefore the connection).
#[tauri::command]
pub fn ssh_provide_secret(state: State<'_, AppState>, session_id: String, secret: Option<String>, remember: bool) -> AppResult<()> {
    let answer = secret.map(|s| SecretAnswer { secret: Zeroizing::new(s), remember });
    state.sessions.prompts.answer_secret(&session_id, answer)
}

#[tauri::command]
pub fn ssh_active_count(state: State<'_, AppState>) -> usize {
    state.sessions.active_count()
}
