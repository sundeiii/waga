//! Local port forwarding (ssh -L) over a server's live SSH connection.
//! Listeners bind to 127.0.0.1 only — nothing is exposed to the network.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use super::connections::ConnectionRegistry;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ForwardState {
    Active,
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardStatus {
    pub id: String,
    /// Saved forward this was started from, if any.
    pub saved_id: Option<String>,
    pub server_id: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub state: ForwardState,
    pub active_connections: u32,
    pub total_connections: u32,
    pub bytes: u64,
    pub message: Option<String>,
}

pub trait ForwardSink: Send + Sync + 'static {
    fn update(&self, s: &ForwardStatus);
}

struct Running {
    task: JoinHandle<()>,
    stats: Arc<Stats>,
    base: ForwardStatus,
}

#[derive(Default)]
struct Stats {
    active: AtomicU32,
    total: AtomicU32,
    bytes: AtomicU64,
}

#[derive(Default)]
pub struct ForwardManager {
    running: Mutex<HashMap<String, Running>>,
}

fn snapshot(base: &ForwardStatus, st: &Stats) -> ForwardStatus {
    ForwardStatus {
        active_connections: st.active.load(Ordering::Relaxed),
        total_connections: st.total.load(Ordering::Relaxed),
        bytes: st.bytes.load(Ordering::Relaxed),
        ..base.clone()
    }
}

impl ForwardManager {
    pub fn list(&self) -> Vec<ForwardStatus> {
        let mut map = self.running.lock().unwrap();
        map.retain(|_, r| !r.task.is_finished());
        map.values().map(|r| snapshot(&r.base, &r.stats)).collect()
    }

    /// Start listening on 127.0.0.1:`local_port` (0 = any free port).
    pub async fn start(
        self: &Arc<Self>,
        connections: Arc<ConnectionRegistry>,
        sink: Arc<dyn ForwardSink>,
        server_id: &str,
        saved_id: Option<String>,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
    ) -> AppResult<ForwardStatus> {
        if !connections.has_server(server_id) {
            return Err(AppError::Validation("Connect to the server first; forwards use its open SSH session".into()));
        }
        if let Some(sid) = &saved_id {
            if self.list().iter().any(|f| f.saved_id.as_ref() == Some(sid)) {
                return Err(AppError::Validation("This forward is already running".into()));
            }
        }
        let listener = TcpListener::bind(("127.0.0.1", local_port)).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                AppError::Validation(format!("Local port {local_port} is already in use"))
            } else {
                AppError::Validation(format!("Cannot listen on 127.0.0.1:{local_port}: {e}"))
            }
        })?;
        let port = listener.local_addr()?.port();
        let id = uuid::Uuid::new_v4().to_string();
        let base = ForwardStatus {
            id: id.clone(),
            saved_id,
            server_id: server_id.to_string(),
            local_port: port,
            remote_host: remote_host.to_string(),
            remote_port,
            state: ForwardState::Active,
            active_connections: 0,
            total_connections: 0,
            bytes: 0,
            message: None,
        };
        let stats = Arc::new(Stats::default());
        let me = self.clone();
        let (b, st, srv, host, fid) = (base.clone(), stats.clone(), server_id.to_string(), remote_host.to_string(), id.clone());
        let task = tokio::spawn(async move {
            let reason = accept_loop(listener, connections, &srv, &host, remote_port, st.clone(), sink.clone(), &b).await;
            let mut done = snapshot(&b, &st);
            done.state = ForwardState::Stopped;
            done.message = Some(reason);
            sink.update(&done);
            me.running.lock().unwrap().remove(&fid);
        });
        self.running.lock().unwrap().insert(id, Running { task, stats, base: base.clone() });
        Ok(base)
    }

    pub fn stop(&self, id: &str, sink: &dyn ForwardSink) -> bool {
        match self.running.lock().unwrap().remove(id) {
            Some(r) => {
                r.task.abort();
                let mut s = snapshot(&r.base, &r.stats);
                s.state = ForwardState::Stopped;
                s.message = Some("Stopped".into());
                sink.update(&s);
                true
            }
            None => false,
        }
    }

    pub fn stop_all_for(&self, server_id: &str, sink: &dyn ForwardSink) {
        let ids: Vec<String> = self.list().into_iter().filter(|f| f.server_id == server_id).map(|f| f.id).collect();
        for id in ids {
            self.stop(&id, sink);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn accept_loop(
    listener: TcpListener,
    connections: Arc<ConnectionRegistry>,
    server_id: &str,
    host: &str,
    port: u16,
    stats: Arc<Stats>,
    sink: Arc<dyn ForwardSink>,
    base: &ForwardStatus,
) -> String {
    // Child connection tasks are aborted when this loop ends.
    let mut children = tokio::task::JoinSet::new();
    let mut check = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            _ = check.tick() => {
                if !connections.has_server(server_id) {
                    return "SSH session closed".into();
                }
                sink.update(&snapshot(base, &stats));
            }
            accepted = listener.accept() => {
                let Ok((mut tcp, peer)) = accepted else { continue };
                let Some(handle) = connections.for_server(server_id) else {
                    return "SSH session closed".into();
                };
                let (host, st) = (host.to_string(), stats.clone());
                children.spawn(async move {
                    let ch = match handle.channel_open_direct_tcpip(host, port as u32, "127.0.0.1", peer.port() as u32).await {
                        Ok(ch) => ch,
                        Err(_) => return, // server refused (forwarding disabled / unreachable): drop the client
                    };
                    st.active.fetch_add(1, Ordering::Relaxed);
                    st.total.fetch_add(1, Ordering::Relaxed);
                    let mut remote = ch.into_stream();
                    if let Ok((a, b)) = tokio::io::copy_bidirectional(&mut tcp, &mut remote).await {
                        st.bytes.fetch_add(a + b, Ordering::Relaxed);
                    }
                    st.active.fetch_sub(1, Ordering::Relaxed);
                });
                while children.try_join_next().is_some() {}
            }
        }
    }
}
