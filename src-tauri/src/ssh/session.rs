//! Session lifecycle: TCP connect -> handshake (host key check) -> auth ->
//! PTY + shell -> bidirectional streaming until either side closes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::{client, ChannelMsg, Disconnect};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use super::auth::authenticate;
use super::connections::ConnectionRegistry;
use super::events::{SessionEvent, SessionSink, SessionState};
use super::handler::{ClientHandler, HostKeyVerdict};
use super::prompts::PromptRegistry;
use crate::credentials::SecretStore;
use crate::database::activity::{self, ActivityKind};
use crate::database::servers::{self, ServerProfile};
use crate::database::Database;

pub const TERM: &str = "xterm-256color";

#[derive(Debug)]
pub enum SessionCommand {
    Data(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
}

/// Outcome that ended a session, reported to the UI as the final status.
#[derive(Debug, Clone)]
pub struct Failure {
    pub state: SessionState,
    pub message: String,
}

impl Failure {
    pub fn new(state: SessionState, message: impl Into<String>) -> Self {
        Self { state, message: message.into() }
    }
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(SessionState::Error, message)
    }
}

pub struct SessionContext {
    pub sid: String,
    pub profile: ServerProfile,
    pub cols: u32,
    pub rows: u32,
    pub connect_timeout: Duration,
    pub keepalive: Option<Duration>,
    pub prompt_timeout: Duration,
    pub db: Arc<Database>,
    pub secrets: Arc<dyn SecretStore>,
    pub prompts: Arc<PromptRegistry>,
    pub sink: Arc<dyn SessionSink>,
    pub connections: Arc<ConnectionRegistry>,
}

impl SessionContext {
    pub fn emit(&self, state: SessionState, message: Option<String>) {
        self.sink.event(SessionEvent::status(state, message));
    }

    pub fn log(&self, kind: ActivityKind, msg: &str) {
        let p = &self.profile;
        let _ = self.db.with(|c| activity::record(c, Some(&p.id), Some(&p.input.name), kind, msg));
    }

    fn endpoint(&self) -> String {
        let h = &self.profile.input.host;
        let host = if h.contains(':') && !h.starts_with('[') { format!("[{h}]") } else { h.clone() };
        format!("{}@{}:{}", self.profile.input.username, host, self.profile.input.port)
    }
}

/// Registry of running sessions -> command senders.
#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<String, mpsc::Sender<SessionCommand>>>,
    pub prompts: Arc<PromptRegistry>,
}

impl SessionManager {
    pub fn register(&self, sid: &str) -> AppResultRx {
        let (tx, rx) = mpsc::channel(512);
        let mut map = self.sessions.lock().unwrap();
        if map.contains_key(sid) {
            return Err(crate::error::AppError::Validation("Session id already in use".into()));
        }
        map.insert(sid.to_string(), tx);
        Ok(rx)
    }

    pub fn sender(&self, sid: &str) -> Option<mpsc::Sender<SessionCommand>> {
        self.sessions.lock().unwrap().get(sid).cloned()
    }

    pub fn remove(&self, sid: &str) {
        self.sessions.lock().unwrap().remove(sid);
        self.prompts.cancel(sid);
    }

    pub fn all_senders(&self) -> Vec<mpsc::Sender<SessionCommand>> {
        self.sessions.lock().unwrap().values().cloned().collect()
    }

    pub fn active_count(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }
}

pub type AppResultRx = crate::error::AppResult<mpsc::Receiver<SessionCommand>>;

/// Drive one session to completion. Always ends by emitting exactly one
/// terminal status (Disconnected or a failure state).
pub async fn run_session(ctx: SessionContext, mut commands: mpsc::Receiver<SessionCommand>) {
    ctx.log(ActivityKind::ConnectAttempt, &format!("Connecting to {}", ctx.endpoint()));
    let result = connect_and_run(&ctx, &mut commands).await;
    ctx.connections.remove(&ctx.sid);
    match result {
        Ok(reason) => {
            ctx.log(ActivityKind::Disconnected, &reason);
            ctx.emit(SessionState::Disconnected, Some(reason));
        }
        Err(f) => {
            let kind = match f.state {
                SessionState::AuthFailed => ActivityKind::AuthFailed,
                SessionState::HostKeyRejected | SessionState::HostKeyMismatch => ActivityKind::HostKeyRejected,
                _ => ActivityKind::ConnectionError,
            };
            ctx.log(kind, &f.message);
            ctx.emit(f.state, Some(f.message));
        }
    }
    ctx.prompts.cancel(&ctx.sid);
}

fn tcp_error(e: &std::io::Error, endpoint: &str) -> Failure {
    use std::io::ErrorKind::*;
    let msg = match e.kind() {
        ConnectionRefused => format!("Connection refused by {endpoint}. Is the SSH server running and the port correct?"),
        TimedOut => return Failure::new(SessionState::Timeout, format!("Timed out connecting to {endpoint}")),
        HostUnreachable | NetworkUnreachable => format!("{endpoint} is unreachable from this network"),
        _ => format!("Could not reach {endpoint}: {e}"),
    };
    Failure::error(msg)
}

impl SessionContext {
    /// Same session (sink, prompts, timeouts) but for another hop's profile.
    fn for_hop(&self, profile: ServerProfile) -> SessionContext {
        SessionContext {
            sid: self.sid.clone(),
            profile,
            cols: self.cols,
            rows: self.rows,
            connect_timeout: self.connect_timeout,
            keepalive: self.keepalive,
            prompt_timeout: self.prompt_timeout,
            db: self.db.clone(),
            secrets: self.secrets.clone(),
            prompts: self.prompts.clone(),
            sink: self.sink.clone(),
            connections: self.connections.clone(),
        }
    }
}

/// The route to the target: [first jump, …, target]. Rejects loops/missing hops.
pub fn hop_chain(db: &Database, target: &ServerProfile) -> Result<Vec<ServerProfile>, String> {
    let mut chain = vec![target.clone()];
    let mut seen = vec![target.id.clone()];
    let mut next = target.input.jump_host_id.clone();
    while let Some(id) = next {
        if seen.contains(&id) {
            return Err("The jump host chain loops back on itself".into());
        }
        if chain.len() > servers::MAX_JUMPS {
            return Err(format!("At most {} jump hosts are supported", servers::MAX_JUMPS));
        }
        let hop = db.with(|c| servers::get(c, &id)).map_err(|_| "The jump host profile no longer exists".to_string())?;
        seen.push(id);
        next = hop.input.jump_host_id.clone();
        chain.insert(0, hop);
    }
    Ok(chain)
}

type Handle = client::Handle<ClientHandler>;

/// SSH handshake (with host key verification) + authentication over any stream.
async fn handshake_and_auth<S>(ctx: &SessionContext, stream: S) -> Result<Handle, Failure>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let p = &ctx.profile.input;
    let endpoint = ctx.endpoint();
    let host = p.host.trim_start_matches('[').trim_end_matches(']').to_string();

    // Host key verification happens inside the handshake and may wait for the
    // user, so the bound includes the prompt timeout.
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: ctx.keepalive,
        keepalive_max: 3,
        ..Default::default()
    });
    let verdict = Arc::new(Mutex::new(None));
    let handler = ClientHandler {
        sid: ctx.sid.clone(),
        host: host.clone(),
        port: p.port,
        server_id: Some(ctx.profile.id.clone()),
        server_name: p.name.clone(),
        db: ctx.db.clone(),
        prompts: ctx.prompts.clone(),
        sink: ctx.sink.clone(),
        verdict: verdict.clone(),
        prompt_timeout: ctx.prompt_timeout,
    };
    let handshake = client::connect_stream(config, stream, handler);
    let mut handle = match tokio::time::timeout(ctx.connect_timeout + ctx.prompt_timeout, handshake).await {
        Err(_) => return Err(Failure::new(SessionState::Timeout, "Timed out during SSH handshake")),
        Ok(Ok(h)) => h,
        Ok(Err(e)) => {
            return Err(match verdict.lock().unwrap().clone() {
                Some(HostKeyVerdict::Rejected { changed: true }) => Failure::new(
                    SessionState::HostKeyMismatch,
                    "Connection blocked: the server's host key has CHANGED and was not accepted. This can indicate a man-in-the-middle attack.",
                ),
                Some(HostKeyVerdict::Rejected { changed: false }) => {
                    Failure::new(SessionState::HostKeyRejected, "Connection cancelled: the host key was not trusted.")
                }
                Some(HostKeyVerdict::Error(m)) => Failure::error(m),
                None => Failure::error(format!("SSH handshake with {endpoint} failed: {e}")),
            })
        }
    };

    ctx.emit(SessionState::Authenticating, None);
    authenticate(&mut handle, ctx).await?;
    Ok(handle)
}

/// Connect to the target, tunnelling through any jump hosts.
/// Returns the jump handles (kept alive for the session) and the target handle.
async fn establish(ctx: &SessionContext) -> Result<(Vec<Arc<Handle>>, Handle), Failure> {
    let chain = hop_chain(&ctx.db, &ctx.profile).map_err(Failure::error)?;
    let first = ctx.for_hop(chain[0].clone());
    let fp = &first.profile.input;
    let first_ep = first.endpoint();
    let via = if chain.len() > 1 { format!(" (jump host for {})", ctx.profile.input.name) } else { String::new() };
    ctx.emit(SessionState::Connecting, Some(format!("Connecting to {first_ep}{via}")));

    // TCP to the first hop (bounded by the connection timeout; includes DNS).
    let host = fp.host.trim_start_matches('[').trim_end_matches(']').to_string();
    let tcp = match tokio::time::timeout(ctx.connect_timeout, TcpStream::connect((host.as_str(), fp.port))).await {
        Err(_) => return Err(Failure::new(SessionState::Timeout, format!("Timed out after {}s connecting to {first_ep}", ctx.connect_timeout.as_secs()))),
        Ok(Err(e)) => return Err(tcp_error(&e, &first_ep)),
        Ok(Ok(s)) => s,
    };
    let _ = tcp.set_nodelay(true);
    let mut handle = handshake_and_auth(&first, tcp).await?;

    // Each further hop runs a full SSH session inside a direct-tcpip tunnel
    // through the previous one (like OpenSSH ProxyJump). The jump host only
    // relays encrypted bytes; the target's host key is verified end-to-end.
    let mut jumps = Vec::new();
    for (i, hop) in chain.iter().enumerate().skip(1) {
        let prev_name = chain[i - 1].input.name.clone();
        let hctx = ctx.for_hop(hop.clone());
        let ep = hctx.endpoint();
        ctx.emit(SessionState::Connecting, Some(format!("Connecting to {ep} via {prev_name}")));
        let target_host = hop.input.host.trim_start_matches('[').trim_end_matches(']').to_string();
        let ch = match tokio::time::timeout(ctx.connect_timeout, handle.channel_open_direct_tcpip(target_host, hop.input.port as u32, "127.0.0.1", 0)).await {
            Err(_) => return Err(Failure::new(SessionState::Timeout, format!("Timed out opening a tunnel from {prev_name} to {ep}"))),
            Ok(Err(e)) => return Err(Failure::error(format!("Jump host {prev_name} could not reach {ep}: {e}. Is TCP forwarding allowed there?"))),
            Ok(Ok(ch)) => ch,
        };
        jumps.push(Arc::new(handle));
        handle = handshake_and_auth(&hctx, ch.into_stream()).await?;
    }
    Ok((jumps, handle))
}

async fn connect_and_run(ctx: &SessionContext, commands: &mut mpsc::Receiver<SessionCommand>) -> Result<String, Failure> {
    let endpoint = ctx.endpoint();
    let (jumps, handle) = establish(ctx).await?;
    // From here on the handle is shared (read-only) with monitoring etc.
    let handle = Arc::new(handle);
    ctx.connections.insert(&ctx.sid, &ctx.profile.id, handle.clone());

    // 4. Interactive shell with a remote PTY.
    let ssh = |e: russh::Error| Failure::error(format!("Could not start shell: {e}"));
    let channel = handle.channel_open_session().await.map_err(ssh)?;
    channel.request_pty(false, TERM, ctx.cols, ctx.rows, 0, 0, &[]).await.map_err(ssh)?;
    channel.request_shell(false).await.map_err(ssh)?;

    let _ = ctx.db.with(|c| servers::mark_connected(c, &ctx.profile.id));
    ctx.log(ActivityKind::Connected, &format!("Connected to {endpoint}"));
    ctx.emit(SessionState::Connected, Some(format!("Connected to {endpoint}")));

    // 5. Stream until closed.
    let (mut reader, writer) = channel.split();
    let mut exit_status: Option<u32> = None;
    let reason = loop {
        tokio::select! {
            msg = reader.wait() => match msg {
                Some(ChannelMsg::Data { data }) => ctx.sink.output(data.to_vec()),
                Some(ChannelMsg::ExtendedData { data, .. }) => ctx.sink.output(data.to_vec()),
                Some(ChannelMsg::ExitStatus { exit_status: s }) => exit_status = Some(s),
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                    break match exit_status {
                        Some(s) => format!("Remote shell exited (status {s})"),
                        None if handle.is_closed() => "Connection to server lost".to_string(),
                        None => "Remote shell closed".to_string(),
                    };
                }
                Some(_) => {}
            },
            cmd = commands.recv() => match cmd {
                Some(SessionCommand::Data(bytes)) => {
                    if let Err(e) = writer.data_bytes(bytes).await {
                        break format!("Connection lost while sending input: {e}");
                    }
                }
                Some(SessionCommand::Resize { cols, rows }) => {
                    let _ = writer.window_change(cols, rows, 0, 0).await;
                }
                Some(SessionCommand::Close) | None => {
                    let _ = writer.close().await;
                    break "Disconnected by user".to_string();
                }
            },
        }
    };
    let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;
    // Tear down jump hosts innermost-first.
    for j in jumps.iter().rev() {
        let _ = j.disconnect(Disconnect::ByApplication, "", "en").await;
    }
    Ok(reason)
}
