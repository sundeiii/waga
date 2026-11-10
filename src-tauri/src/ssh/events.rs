use serde::Serialize;

use crate::security::host_keys::HostKeyStatus;

/// Lifecycle states shown in the UI. Each one maps to something that
/// actually happened; there is no optimistic "connected" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    Connecting,
    AwaitingHostKey,
    Authenticating,
    AwaitingSecret,
    Connected,
    Disconnected,
    AuthFailed,
    Timeout,
    HostKeyRejected,
    HostKeyMismatch,
    Error,
}

impl SessionState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Disconnected | Self::AuthFailed | Self::Timeout | Self::HostKeyRejected | Self::HostKeyMismatch | Self::Error
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretKind {
    Password,
    Passphrase,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviousKey {
    pub algorithm: String,
    pub fingerprint: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SessionEvent {
    Status {
        state: SessionState,
        message: Option<String>,
    },
    HostKeyPrompt {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
        status: HostKeyStatus,
        previous: Vec<PreviousKey>,
    },
    SecretPrompt {
        kind: SecretKind,
        prompt: String,
        attempt: u32,
    },
    /// Keyboard-interactive round (2FA codes, PAM prompts). Text comes from
    /// the server and is sanitized before being sent to the UI.
    InteractivePrompt {
        name: String,
        instructions: String,
        prompts: Vec<InteractiveQuestion>,
        round: u32,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractiveQuestion {
    pub prompt: String,
    /// false = hide input (passwords, codes)
    pub echo: bool,
}

/// Where a session sends its events and terminal output.
/// The Tauri implementation uses IPC channels; tests collect into memory.
pub trait SessionSink: Send + Sync + 'static {
    fn event(&self, event: SessionEvent);
    fn output(&self, bytes: Vec<u8>);
}

impl SessionEvent {
    pub fn status(state: SessionState, message: impl Into<Option<String>>) -> Self {
        SessionEvent::Status { state, message: message.into() }
    }
}
