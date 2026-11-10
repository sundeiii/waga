//! Opt-in end-to-end test against a REAL OpenSSH server (not a mock).
//! Run:  cargo test --test real_sshd -- --ignored --nocapture
//! Env (defaults in brackets):
//!   YORU_TEST_HOST [127.0.0.1]  YORU_TEST_PORT [22]  YORU_TEST_USER [%USERNAME%]
//!   YORU_TEST_KEY  [%USERPROFILE%\.ssh\yoru_test_ed25519]  YORU_TEST_KIND [windows|linux, default windows]

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use yoru_lib::credentials::MemoryStore;
use yoru_lib::database::servers::{self, AuthMethod, ProfileKind, ServerInput};
use yoru_lib::database::{known_hosts, settings, Database};
use yoru_lib::monitoring::manager::{MonitorDeps, MonitorManager, MonitorSink, MonitorStatus};
use yoru_lib::monitoring::Sample;
use yoru_lib::security::host_keys::{HostKeyAnswer, HostKeyStatus};
use yoru_lib::ssh::connections::ConnectionRegistry;
use yoru_lib::ssh::events::{SessionEvent, SessionSink, SessionState};
use yoru_lib::ssh::prompts::PromptRegistry;
use yoru_lib::ssh::session::{run_session, SessionCommand, SessionContext};

struct Sink(mpsc::UnboundedSender<SessionEvent>, mpsc::UnboundedSender<Vec<u8>>);
impl SessionSink for Sink {
    fn event(&self, e: SessionEvent) {
        let _ = self.0.send(e);
    }
    fn output(&self, b: Vec<u8>) {
        let _ = self.1.send(b);
    }
}

struct MonSink(mpsc::UnboundedSender<Result<Sample, MonitorStatus>>);
impl MonitorSink for MonSink {
    fn sample(&self, s: &Sample) {
        let _ = self.0.send(Ok(s.clone()));
    }
    fn status(&self, s: &MonitorStatus) {
        let _ = self.0.send(Err(s.clone()));
    }
}

fn env(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

#[tokio::test]
#[ignore]
async fn real_openssh_end_to_end() {
    let host = env("YORU_TEST_HOST", "127.0.0.1");
    let port: u16 = env("YORU_TEST_PORT", "22").parse().unwrap();
    let user = env("YORU_TEST_USER", &std::env::var("USERNAME").unwrap_or_default());
    let key = env("YORU_TEST_KEY", &format!("{}\\.ssh\\yoru_test_ed25519", std::env::var("USERPROFILE").unwrap_or_default()));
    let kind = if env("YORU_TEST_KIND", "windows") == "linux" { ProfileKind::Linux } else { ProfileKind::Windows };

    let db = Arc::new(Database::open_in_memory().unwrap());
    let mut s = settings::AppSettings::default();
    s.monitoring.interval_secs = 2;
    db.with(|c| settings::save(c, &s)).unwrap();
    let profile = db
        .with(|c| {
            servers::insert(
                c,
                &ServerInput {
                    name: "real-sshd".into(),
                    host: host.clone(),
                    port,
                    username: user.clone(),
                    auth_method: AuthMethod::Key,
                    key_path: Some(key.clone()),
                    profile_kind: kind,
                    group_name: None,
                    tags: vec![],
                    notes: String::new(),
                    favorite: false,
                    auto_reconnect: false,
                    keepalive: true,
                },
            )
        })
        .unwrap();

    let prompts = Arc::new(PromptRegistry::default());
    let connections = Arc::new(ConnectionRegistry::default());
    let (etx, mut erx) = mpsc::unbounded_channel();
    let (otx, mut orx) = mpsc::unbounded_channel();
    let (ctx_tx, crx) = mpsc::channel(64);
    let sid = uuid::Uuid::new_v4().to_string();
    let ctx = SessionContext {
        sid: sid.clone(),
        profile: profile.clone(),
        cols: 120,
        rows: 32,
        connect_timeout: Duration::from_secs(10),
        keepalive: Some(Duration::from_secs(30)),
        prompt_timeout: Duration::from_secs(10),
        db: db.clone(),
        secrets: Arc::new(MemoryStore::default()),
        prompts: prompts.clone(),
        sink: Arc::new(Sink(etx, otx)),
        connections: connections.clone(),
    };
    let task = tokio::spawn(run_session(ctx, crx));

    // 1. Unknown host key -> prompt with the real server's fingerprint.
    loop {
        match tokio::time::timeout(Duration::from_secs(15), erx.recv()).await.unwrap().unwrap() {
            SessionEvent::HostKeyPrompt { status, fingerprint, algorithm, .. } => {
                assert_eq!(status, HostKeyStatus::Unknown);
                println!("host key: {algorithm} {fingerprint}");
                prompts.answer_host_key(&sid, HostKeyAnswer { accept: true, acknowledge_changed: false }).unwrap();
                break;
            }
            SessionEvent::Status { state, message } if state.is_terminal() => panic!("{state:?}: {message:?}"),
            _ => {}
        }
    }
    // 2. Key auth + PTY shell.
    loop {
        match tokio::time::timeout(Duration::from_secs(15), erx.recv()).await.unwrap().unwrap() {
            SessionEvent::Status { state: SessionState::Connected, .. } => break,
            SessionEvent::Status { state, message } if state.is_terminal() => panic!("{state:?}: {message:?}"),
            _ => {}
        }
    }
    assert_eq!(db.with(|c| known_hosts::list_for(c, &host, port)).unwrap().len(), 1);

    // 3. Run a real command through the interactive shell.
    ctx_tx.send(SessionCommand::Resize { cols: 100, rows: 30 }).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    ctx_tx.send(SessionCommand::Data(b"echo yoru-e2e-%USERNAME%-$env:USERNAME\r".to_vec())).await.unwrap();
    let mut out = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !out.contains("yoru-e2e-") || !out.to_lowercase().contains(&user.to_lowercase()) {
        let chunk = tokio::time::timeout_at(deadline, orx.recv()).await.unwrap_or_else(|_| panic!("no echo; got: {out:?}")).unwrap();
        out.push_str(&String::from_utf8_lossy(&chunk));
    }
    println!("shell output ok ({} bytes)", out.len());

    // 4. Monitoring over the same connection (real provider against real OS).
    let mgr = Arc::new(MonitorManager::default());
    let (mtx, mut mrx) = mpsc::unbounded_channel();
    mgr.start(&profile.id, MonitorDeps { db: db.clone(), connections: connections.clone(), sink: Arc::new(MonSink(mtx)) });
    let mut got = Vec::new();
    while got.len() < 2 {
        match tokio::time::timeout(Duration::from_secs(60), mrx.recv()).await.expect("no monitor sample").unwrap() {
            Ok(s) => got.push(s),
            Err(st) => assert_ne!(format!("{:?}", st.state), "Error", "monitor error: {:?}", st.message),
        }
    }
    let s = &got[1];
    println!("sample: os={:?} host={:?} cpu={:?} mem={:?}/{:?} disks={} net={:?}/{:?} uptime={:?}", s.os, s.hostname, s.cpu_percent, s.mem_used_bytes, s.mem_total_bytes, s.disks.len(), s.net_rx_bps, s.net_tx_bps, s.uptime_secs);
    assert!(s.os.is_some() && s.hostname.is_some());
    assert!(s.mem_total_bytes.unwrap() > 0);
    assert!(!s.disks.is_empty());
    assert!(s.uptime_secs.is_some());
    assert!(s.net_rx_bps.is_some(), "second sample has a network rate");

    ctx_tx.send(SessionCommand::Close).await.unwrap();
    task.await.unwrap();
}
