//! Authenticated SSH connections that other features (monitoring, later SFTP)
//! may reuse by opening extra channels — no second login, no extra TCP connection.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::{client, ChannelMsg};

use super::handler::ClientHandler;

pub type SshHandle = client::Handle<ClientHandler>;

/// Upper bound on captured command output (protects against runaway remotes).
const MAX_OUTPUT: usize = 2 * 1024 * 1024;

#[derive(Default)]
pub struct ConnectionRegistry {
    inner: Mutex<HashMap<String, (String, Arc<SshHandle>)>>,
}

impl ConnectionRegistry {
    pub fn insert(&self, sid: &str, server_id: &str, handle: Arc<SshHandle>) {
        self.inner.lock().unwrap().insert(sid.to_string(), (server_id.to_string(), handle));
    }

    pub fn remove(&self, sid: &str) {
        self.inner.lock().unwrap().remove(sid);
    }

    /// Any live connection to this server.
    pub fn for_server(&self, server_id: &str) -> Option<Arc<SshHandle>> {
        self.inner
            .lock()
            .unwrap()
            .values()
            .find(|(sv, h)| sv == server_id && !h.is_closed())
            .map(|(_, h)| h.clone())
    }

    pub fn has_server(&self, server_id: &str) -> bool {
        self.for_server(server_id).is_some()
    }
}

#[derive(Debug)]
pub struct ExecOutput {
    pub stdout: String,
    pub exit_status: Option<u32>,
}

/// Run a fixed, non-interactive command on a separate channel (no PTY).
/// `command` must be a constant built by Yoru — never user or remote input.
pub async fn exec(handle: &SshHandle, command: &str, timeout: Duration) -> Result<ExecOutput, String> {
    let run = async {
        let mut ch = handle.channel_open_session().await.map_err(|e| format!("Cannot open channel: {e}"))?;
        ch.exec(true, command).await.map_err(|e| format!("Cannot run command: {e}"))?;
        // No stdin: signal EOF so programs that read it (e.g. powershell.exe
        // with redirected input) don't wait forever.
        let _ = ch.eof().await;
        let mut out: Vec<u8> = Vec::new();
        let mut exit_status = None;
        loop {
            match ch.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    if out.len() + data.len() > MAX_OUTPUT {
                        let _ = ch.close().await;
                        return Err("Command output too large".to_string());
                    }
                    out.extend_from_slice(&data);
                }
                Some(ChannelMsg::ExitStatus { exit_status: s }) => exit_status = Some(s),
                Some(ChannelMsg::Close) | None => break,
                Some(_) => {}
            }
        }
        Ok(ExecOutput { stdout: String::from_utf8_lossy(&out).into_owned(), exit_status })
    };
    tokio::time::timeout(timeout, run).await.map_err(|_| "Command timed out".to_string())?
}
