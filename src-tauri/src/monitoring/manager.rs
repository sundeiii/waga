//! One polling task per monitored server. Tasks never open SSH connections:
//! they borrow an existing authenticated one, and stop on their own when the
//! server has had no live session for a few intervals.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::task::JoinHandle;

use super::{Provider, Sample, Sampler};
use crate::database::activity::{self, ActivityKind};
use crate::database::{servers, settings, Database};
use crate::ssh::connections::ConnectionRegistry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MonitorState {
    Running,
    NoConnection,
    Error,
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorStatus {
    pub server_id: String,
    pub state: MonitorState,
    pub message: Option<String>,
    pub ts: String,
}

pub trait MonitorSink: Send + Sync + 'static {
    fn sample(&self, sample: &Sample);
    fn status(&self, status: &MonitorStatus);
}

/// Stop after this many consecutive intervals without a live connection.
const MAX_IDLE_TICKS: u32 = 5;

/// Each Windows sample starts powershell.exe (~0.5–1 s CPU on the server),
/// so Windows hosts are never polled faster than this.
pub const WINDOWS_MIN_INTERVAL_SECS: u32 = 3;

#[derive(Default)]
pub struct MonitorManager {
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
}

pub struct MonitorDeps {
    pub db: Arc<Database>,
    pub connections: Arc<ConnectionRegistry>,
    pub sink: Arc<dyn MonitorSink>,
}

fn status(sink: &dyn MonitorSink, server_id: &str, state: MonitorState, message: Option<String>) {
    sink.status(&MonitorStatus { server_id: server_id.to_string(), state, message, ts: crate::database::now() });
}

impl MonitorManager {
    pub fn active(&self) -> Vec<String> {
        let mut t = self.tasks.lock().unwrap();
        t.retain(|_, h| !h.is_finished());
        t.keys().cloned().collect()
    }

    /// Idempotent: starting an already-running monitor is a no-op.
    pub fn start(self: &Arc<Self>, server_id: &str, deps: MonitorDeps) {
        let mut tasks = self.tasks.lock().unwrap();
        if tasks.get(server_id).is_some_and(|h| !h.is_finished()) {
            return;
        }
        let me = self.clone();
        let sid = server_id.to_string();
        let handle = tokio::spawn(async move {
            run(&sid, &deps).await;
            me.tasks.lock().unwrap().remove(&sid);
        });
        tasks.insert(server_id.to_string(), handle);
    }

    pub fn stop(&self, server_id: &str, sink: &dyn MonitorSink) {
        if let Some(h) = self.tasks.lock().unwrap().remove(server_id) {
            h.abort();
            status(sink, server_id, MonitorState::Stopped, Some("Paused".into()));
        }
    }
}

async fn run(server_id: &str, deps: &MonitorDeps) {
    let mut sampler = Sampler::default();
    let mut idle_ticks = 0;
    let mut error_logged = false;
    let mut last_os: Option<String> = None;

    loop {
        let tick_started = Instant::now();
        let mut interval = deps
            .db
            .with(settings::load)
            .map(|s| s.monitoring.interval_secs.clamp(1, 300))
            .unwrap_or(1);
        let profile = match deps.db.with(|c| servers::get(c, server_id)) {
            Ok(p) => p,
            Err(_) => {
                status(deps.sink.as_ref(), server_id, MonitorState::Stopped, Some("Server profile was removed".into()));
                return;
            }
        };
        let provider = Provider::for_kind(profile.input.profile_kind);
        if provider == Provider::Windows {
            interval = interval.max(WINDOWS_MIN_INTERVAL_SECS);
        }

        match deps.connections.for_server(server_id) {
            None => {
                idle_ticks += 1;
                if idle_ticks >= MAX_IDLE_TICKS {
                    status(deps.sink.as_ref(), server_id, MonitorState::Stopped, Some("No open SSH session; monitoring stopped".into()));
                    return;
                }
                status(deps.sink.as_ref(), server_id, MonitorState::NoConnection, Some("Waiting for an open SSH session".into()));
            }
            Some(handle) => {
                idle_ticks = 0;
                // PowerShell cold start + CIM queries can take several seconds.
                let timeout = Duration::from_secs((interval as u64).max(25));
                match provider.collect(&handle, timeout).await {
                    Ok(raw) => {
                        let sample = sampler.sample(server_id, raw, Instant::now());
                        if sample.os.is_some() && sample.os != last_os {
                            last_os = sample.os.clone();
                            let _ = deps.db.with(|c| servers::set_os_detected(c, server_id, last_os.as_deref()));
                        }
                        deps.sink.sample(&sample);
                        status(deps.sink.as_ref(), server_id, MonitorState::Running, None);
                        error_logged = false;
                    }
                    Err(e) => {
                        if !error_logged {
                            let _ = deps.db.with(|c| activity::record(c, Some(server_id), Some(&profile.input.name), ActivityKind::MonitoringFailed, &e));
                            error_logged = true;
                        }
                        status(deps.sink.as_ref(), server_id, MonitorState::Error, Some(e));
                    }
                }
            }
        }
        // Fixed cadence: subtract the time the sample itself took.
        tokio::time::sleep(Duration::from_secs(interval as u64).saturating_sub(tick_started.elapsed())).await;
    }
}
