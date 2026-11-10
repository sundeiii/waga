//! REAL integration tests: Yoru's SSH client code talks over TCP to an
//! in-process SSH server (russh::server) on 127.0.0.1. Nothing is mocked
//! on the client side except the OS credential store (MemoryStore) and the
//! UI sink (collects events in memory).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, Pty};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use yoru_lib::credentials::{password_account, MemoryStore, SecretStore};
use yoru_lib::database::servers::{self, AuthMethod, ProfileKind, ServerInput};
use yoru_lib::database::{known_hosts, Database};
use yoru_lib::security::host_keys::HostKeyStatus;
use yoru_lib::ssh::events::{SessionEvent, SessionSink, SessionState};
use yoru_lib::ssh::prompts::{PromptRegistry, SecretAnswer};
use yoru_lib::ssh::session::{run_session, SessionCommand, SessionContext};
use yoru_lib::security::host_keys::HostKeyAnswer;
use yoru_lib::database::settings;
use yoru_lib::monitoring::manager::{MonitorDeps, MonitorManager, MonitorSink, MonitorState, MonitorStatus};
use yoru_lib::monitoring::Sample;
use yoru_lib::ssh::connections::ConnectionRegistry;

const PASSWORD: &str = "correct horse";

// ---------------------------------------------------------------- server

#[derive(Default)]
struct Observed {
    pty: Option<(String, u32, u32)>,
    resizes: Vec<(u32, u32)>,
    execs: Vec<String>,
}

#[derive(Clone)]
struct TestServer {
    client_key: Option<PublicKey>,
    observed: Arc<Mutex<Observed>>,
}

impl server::Server for TestServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}

impl server::Handler for TestServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if password == PASSWORD { Auth::Accept } else { Auth::reject() })
    }

    async fn auth_publickey(&mut self, _user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        let ok = self.client_key.as_ref().map(|k| k.key_data() == key.key_data()).unwrap_or(false);
        Ok(if ok { Auth::Accept } else { Auth::reject() })
    }

    async fn channel_open_session(&mut self, _channel: Channel<Msg>, reply: server::ChannelOpenHandle, _s: &mut Session) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        _channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        _pw: u32,
        _ph: u32,
        _modes: &[(Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.observed.lock().unwrap().pty = Some((term.to_string(), col_width, row_height));
        Ok(())
    }

    async fn shell_request(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        session.data(channel, b"welcome to test-host\r\n$ ".to_vec())?;
        Ok(())
    }

    async fn window_change_request(&mut self, _c: ChannelId, cols: u32, rows: u32, _pw: u32, _ph: u32, _s: &mut Session) -> Result<(), Self::Error> {
        self.observed.lock().unwrap().resizes.push((cols, rows));
        Ok(())
    }

    /// Fake a Linux host for monitoring: answers Yoru's /proc script with
    /// realistic output whose CPU/network counters advance on each call.
    async fn exec_request(&mut self, channel: ChannelId, data: &[u8], session: &mut Session) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        let cmd = String::from_utf8_lossy(data).to_string();
        let n = {
            let mut o = self.observed.lock().unwrap();
            o.execs.push(cmd.clone());
            o.execs.len() as u64
        };
        let out = if cmd.starts_with("/bin/sh -c '") && cmd.contains("@@stat") {
            format!(
                "@@stat\ncpu  {u} 0 {s} {i} 0 0 0 0 0 0\n@@cpus\n2\n@@meminfo\nMemTotal: 2048000 kB\nMemAvailable: 1024000 kB\n@@loadavg\n0.50 0.40 0.30 1/100 42\n@@uptime\n1234.5 99.0\n@@hostname\ntest-host\n@@os\nPRETTY_NAME=\"Test Linux 1.0\"\n@@uname\nLinux 6.0\n@@df\nFilesystem 1024-blocks Used Available Capacity Mounted on\n/dev/vda1 1000 250 750 25% /\n@@netdev\n  eth0: {rx} 0 0 0 0 0 0 0 {tx} 0 0 0 0 0 0 0\n@@end\n",
                u = 100 * n,
                s = 100 * n,
                i = 800 * n,
                rx = 10_000 * n,
                tx = 1_000 * n
            )
        } else {
            "unknown command\n".to_string()
        };
        session.data(channel, out.into_bytes())?;
        session.exit_status_request(channel, 0)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }

    async fn data(&mut self, channel: ChannelId, data: &[u8], session: &mut Session) -> Result<(), Self::Error> {
        if data == b"exit\r" {
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        let mut echo = b"echo:".to_vec();
        echo.extend_from_slice(data);
        session.data(channel, echo)?;
        Ok(())
    }
}

fn host_key(seed: u8) -> PrivateKey {
    PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32]))
}

async fn start_server(seed: u8, client_key: Option<PublicKey>) -> (u16, Arc<Mutex<Observed>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let observed = Arc::new(Mutex::new(Observed::default()));
    let config = Arc::new(server::Config {
        keys: vec![host_key(seed)],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let mut srv = TestServer { client_key, observed: observed.clone() };
    tokio::spawn(async move {
        let _ = srv.run_on_socket(config, &listener).await;
    });
    (port, observed)
}

// ---------------------------------------------------------------- client harness

struct TestSink {
    events: mpsc::UnboundedSender<SessionEvent>,
    output: mpsc::UnboundedSender<Vec<u8>>,
}

impl SessionSink for TestSink {
    fn event(&self, e: SessionEvent) {
        let _ = self.events.send(e);
    }
    fn output(&self, b: Vec<u8>) {
        let _ = self.output.send(b);
    }
}

struct Harness {
    db: Arc<Database>,
    secrets: Arc<MemoryStore>,
    prompts: Arc<PromptRegistry>,
    connections: Arc<ConnectionRegistry>,
}

struct Running {
    sid: String,
    cmd: mpsc::Sender<SessionCommand>,
    events: mpsc::UnboundedReceiver<SessionEvent>,
    output: mpsc::UnboundedReceiver<Vec<u8>>,
    task: tokio::task::JoinHandle<()>,
}

impl Harness {
    fn new() -> Self {
        Self {
            db: Arc::new(Database::open_in_memory().unwrap()),
            secrets: Arc::new(MemoryStore::default()),
            prompts: Arc::new(PromptRegistry::default()),
            connections: Arc::new(ConnectionRegistry::default()),
        }
    }

    fn profile(&self, port: u16, auth: AuthMethod, key_path: Option<String>) -> servers::ServerProfile {
        let input = ServerInput {
            name: "test".into(),
            host: "127.0.0.1".into(),
            port,
            username: "tester".into(),
            auth_method: auth,
            key_path,
            profile_kind: ProfileKind::Linux,
            group_name: None,
            tags: vec![],
            notes: String::new(),
            favorite: false,
            auto_reconnect: false,
            keepalive: true,
            jump_host_id: None,
        };
        self.db.with(|c| servers::insert(c, &input)).unwrap()
    }

    fn start(&self, profile: servers::ServerProfile, connect_timeout: Duration, prompt_timeout: Duration) -> Running {
        let (etx, erx) = mpsc::unbounded_channel();
        let (otx, orx) = mpsc::unbounded_channel();
        let (ctx_tx, crx) = mpsc::channel(64);
        let sid = uuid::Uuid::new_v4().to_string();
        let ctx = SessionContext {
            sid: sid.clone(),
            profile,
            cols: 100,
            rows: 30,
            connect_timeout,
            keepalive: Some(Duration::from_secs(30)),
            prompt_timeout,
            db: self.db.clone(),
            secrets: self.secrets.clone(),
            prompts: self.prompts.clone(),
            sink: Arc::new(TestSink { events: etx, output: otx }),
            connections: self.connections.clone(),
        };
        let task = tokio::spawn(run_session(ctx, crx));
        Running { sid, cmd: ctx_tx, events: erx, output: orx, task }
    }
}

impl Running {
    async fn next_event(&mut self) -> SessionEvent {
        tokio::time::timeout(Duration::from_secs(15), self.events.recv())
            .await
            .expect("timed out waiting for session event")
            .expect("event stream ended")
    }

    /// Skip events until one matches.
    async fn wait_for(&mut self, pred: impl Fn(&SessionEvent) -> bool) -> SessionEvent {
        loop {
            let e = self.next_event().await;
            if pred(&e) {
                return e;
            }
        }
    }

    async fn wait_state(&mut self, state: SessionState) -> Option<String> {
        match self.wait_for(|e| matches!(e, SessionEvent::Status { state: s, .. } if *s == state || s.is_terminal())).await {
            SessionEvent::Status { state: s, message } => {
                assert_eq!(s, state, "unexpected terminal state, message: {message:?}");
                message
            }
            _ => unreachable!(),
        }
    }

    async fn read_until(&mut self, needle: &str) -> String {
        let mut buf = String::new();
        while !buf.contains(needle) {
            let chunk = tokio::time::timeout(Duration::from_secs(10), self.output.recv())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {needle:?}, got {buf:?}"))
                .expect("output ended");
            buf.push_str(&String::from_utf8_lossy(&chunk));
        }
        buf
    }
}

const T: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn full_lifecycle_unknown_host_key_password_pty_echo_resize_close() {
    let (port, observed) = start_server(1, None).await;
    let h = Harness::new();
    let profile = h.profile(port, AuthMethod::Password, None);
    h.secrets.set(&password_account(&profile.id), PASSWORD).unwrap();

    let mut r = h.start(profile.clone(), T, T);
    assert_eq!(r.wait_state(SessionState::Connecting).await.is_some(), true);

    // First connection: host key is unknown -> must prompt with fingerprint.
    let prompt = r.wait_for(|e| matches!(e, SessionEvent::HostKeyPrompt { .. })).await;
    let SessionEvent::HostKeyPrompt { status, fingerprint, algorithm, .. } = prompt else { unreachable!() };
    assert_eq!(status, HostKeyStatus::Unknown);
    assert!(fingerprint.starts_with("SHA256:"), "{fingerprint}");
    assert_eq!(algorithm, "ssh-ed25519");
    h.prompts.answer_host_key(&r.sid, HostKeyAnswer { accept: true, acknowledge_changed: false }).unwrap();

    r.wait_state(SessionState::Connected).await;
    assert_eq!(h.db.with(|c| known_hosts::list_for(c, "127.0.0.1", port)).unwrap().len(), 1);
    // The shell banner is sent after the server handled pty-req (requests are ordered).
    r.read_until("welcome to test-host").await;
    assert_eq!(observed.lock().unwrap().pty, Some(("xterm-256color".into(), 100, 30)));
    r.cmd.send(SessionCommand::Data(b"ls -la".to_vec())).await.unwrap();
    r.read_until("echo:ls -la").await;

    // UTF-8 passes through untouched.
    r.cmd.send(SessionCommand::Data("ÿoru 夜 🌙".as_bytes().to_vec())).await.unwrap();
    r.read_until("echo:ÿoru 夜 🌙").await;

    r.cmd.send(SessionCommand::Resize { cols: 132, rows: 43 }).await.unwrap();
    // Round-trip something to make sure the resize was processed first.
    r.cmd.send(SessionCommand::Data(b"x".to_vec())).await.unwrap();
    r.read_until("echo:x").await;
    assert_eq!(observed.lock().unwrap().resizes, vec![(132, 43)]);

    r.cmd.send(SessionCommand::Close).await.unwrap();
    assert_eq!(r.wait_state(SessionState::Disconnected).await.as_deref(), Some("Disconnected by user"));
    r.task.await.unwrap();

    // Second connection: key now trusted -> no prompt at all.
    let mut r2 = h.start(profile, T, T);
    let e = r2
        .wait_for(|e| matches!(e, SessionEvent::HostKeyPrompt { .. }) || matches!(e, SessionEvent::Status { state: SessionState::Connected, .. }))
        .await;
    assert!(matches!(e, SessionEvent::Status { state: SessionState::Connected, .. }), "trusted key must not prompt: {e:?}");

    // Remote shell exit is reported as such.
    r2.cmd.send(SessionCommand::Data(b"exit\r".to_vec())).await.unwrap();
    let msg = r2.wait_state(SessionState::Disconnected).await.unwrap();
    assert!(msg.contains("exited (status 0)") || msg.contains("closed"), "{msg}");
}

#[tokio::test]
async fn changed_host_key_is_blocked_without_explicit_acknowledgement() {
    let (port, _) = start_server(2, None).await;
    let h = Harness::new();
    let profile = h.profile(port, AuthMethod::Password, None);
    h.secrets.set(&password_account(&profile.id), PASSWORD).unwrap();
    // Previously trusted a DIFFERENT key for this host:port.
    let old = host_key(99).public_key().clone();
    h.db.with(|c| known_hosts::add(c, "127.0.0.1", port, "ssh-ed25519", &old.to_openssh().unwrap(), "SHA256:old")).unwrap();

    let mut r = h.start(profile.clone(), T, T);
    let prompt = r.wait_for(|e| matches!(e, SessionEvent::HostKeyPrompt { .. })).await;
    let SessionEvent::HostKeyPrompt { status, previous, .. } = prompt else { unreachable!() };
    assert_eq!(status, HostKeyStatus::Changed);
    assert_eq!(previous.len(), 1);

    // "Accept" without the explicit acknowledgement must still be refused.
    h.prompts.answer_host_key(&r.sid, HostKeyAnswer { accept: true, acknowledge_changed: false }).unwrap();
    let msg = r.wait_state(SessionState::HostKeyMismatch).await.unwrap();
    assert!(msg.contains("CHANGED"));
    // Stored key untouched.
    let stored = h.db.with(|c| known_hosts::list_for(c, "127.0.0.1", port)).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].fingerprint, "SHA256:old");

    // With explicit acknowledgement the key is replaced and the connection proceeds.
    let mut r2 = h.start(profile, T, T);
    r2.wait_for(|e| matches!(e, SessionEvent::HostKeyPrompt { .. })).await;
    h.prompts.answer_host_key(&r2.sid, HostKeyAnswer { accept: true, acknowledge_changed: true }).unwrap();
    r2.wait_state(SessionState::Connected).await;
    let stored = h.db.with(|c| known_hosts::list_for(c, "127.0.0.1", port)).unwrap();
    assert_eq!(stored.len(), 1);
    assert_ne!(stored[0].fingerprint, "SHA256:old");
}

#[tokio::test]
async fn rejecting_unknown_host_key_aborts_and_stores_nothing() {
    let (port, _) = start_server(3, None).await;
    let h = Harness::new();
    let profile = h.profile(port, AuthMethod::Password, None);
    let mut r = h.start(profile, T, T);
    r.wait_for(|e| matches!(e, SessionEvent::HostKeyPrompt { .. })).await;
    h.prompts.answer_host_key(&r.sid, HostKeyAnswer { accept: false, acknowledge_changed: false }).unwrap();
    r.wait_state(SessionState::HostKeyRejected).await;
    assert!(h.db.with(|c| known_hosts::list_for(c, "127.0.0.1", port)).unwrap().is_empty());
}

#[tokio::test]
async fn host_key_prompt_times_out_as_rejection() {
    let (port, _) = start_server(4, None).await;
    let h = Harness::new();
    let profile = h.profile(port, AuthMethod::Password, None);
    let mut r = h.start(profile, T, Duration::from_millis(300));
    r.wait_for(|e| matches!(e, SessionEvent::HostKeyPrompt { .. })).await;
    // Never answer.
    r.wait_state(SessionState::HostKeyRejected).await;
}

#[tokio::test]
async fn password_prompt_wrong_then_right_with_remember() {
    let (port, _) = start_server(5, None).await;
    let h = Harness::new();
    let profile = h.profile(port, AuthMethod::Password, None);
    h.db.with(|c| known_hosts::add(c, "127.0.0.1", port, "ssh-ed25519", &host_key(5).public_key().to_openssh().unwrap(), "SHA256:x")).unwrap();

    let mut r = h.start(profile.clone(), T, T);
    let p1 = r.wait_for(|e| matches!(e, SessionEvent::SecretPrompt { .. })).await;
    assert!(matches!(p1, SessionEvent::SecretPrompt { attempt: 1, .. }));
    h.prompts.answer_secret(&r.sid, Some(SecretAnswer { secret: "nope".to_string().into(), remember: true })).unwrap();

    let p2 = r.wait_for(|e| matches!(e, SessionEvent::SecretPrompt { .. })).await;
    let SessionEvent::SecretPrompt { attempt, prompt, .. } = p2 else { unreachable!() };
    assert_eq!(attempt, 2);
    assert!(prompt.contains("Incorrect password"));
    // Wrong password was NOT remembered.
    assert!(h.secrets.get(&password_account(&profile.id)).unwrap().is_none());

    h.prompts.answer_secret(&r.sid, Some(SecretAnswer { secret: PASSWORD.to_string().into(), remember: true })).unwrap();
    r.wait_state(SessionState::Connected).await;
    assert_eq!(h.secrets.get(&password_account(&profile.id)).unwrap().unwrap().as_str(), PASSWORD);
}

#[tokio::test]
async fn cancelled_password_prompt_is_auth_failure() {
    let (port, _) = start_server(6, None).await;
    let h = Harness::new();
    let profile = h.profile(port, AuthMethod::Password, None);
    h.db.with(|c| known_hosts::add(c, "127.0.0.1", port, "ssh-ed25519", &host_key(6).public_key().to_openssh().unwrap(), "SHA256:x")).unwrap();
    h.secrets.set(&password_account(&profile.id), "stale-password").unwrap();

    let mut r = h.start(profile, T, T);
    // Saved password is tried first and rejected -> prompt explains why.
    let p = r.wait_for(|e| matches!(e, SessionEvent::SecretPrompt { .. })).await;
    let SessionEvent::SecretPrompt { prompt, .. } = p else { unreachable!() };
    assert!(prompt.contains("saved password was rejected"), "{prompt}");
    h.prompts.answer_secret(&r.sid, None).unwrap();
    let msg = r.wait_state(SessionState::AuthFailed).await.unwrap();
    assert!(msg.contains("cancelled"));
}

#[tokio::test]
async fn private_key_authentication() {
    let client = host_key(42);
    let (port, _) = start_server(7, Some(client.public_key().clone())).await;
    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("id_ed25519");
    std::fs::write(&key_path, client.to_openssh(russh::keys::ssh_key::LineEnding::LF).unwrap().as_bytes()).unwrap();

    let h = Harness::new();
    let profile = h.profile(port, AuthMethod::Key, Some(key_path.to_string_lossy().into_owned()));
    h.db.with(|c| known_hosts::add(c, "127.0.0.1", port, "ssh-ed25519", &host_key(7).public_key().to_openssh().unwrap(), "SHA256:x")).unwrap();
    let mut r = h.start(profile, T, T);
    r.wait_state(SessionState::Connected).await;
    r.read_until("welcome").await;

    // A different key is rejected with a clear message.
    let other = host_key(43);
    std::fs::write(&key_path, other.to_openssh(russh::keys::ssh_key::LineEnding::LF).unwrap().as_bytes()).unwrap();
    let p2 = h.profile(port, AuthMethod::Key, Some(key_path.to_string_lossy().into_owned()));
    let mut r2 = h.start(p2, T, T);
    let msg = r2.wait_state(SessionState::AuthFailed).await.unwrap();
    assert!(msg.contains("rejected the private key"), "{msg}");
}

#[tokio::test]
async fn connection_refused_is_reported() {
    // Bind then drop to get a port with nothing listening.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let h = Harness::new();
    let profile = h.profile(port, AuthMethod::Password, None);
    let mut r = h.start(profile, T, T);
    let msg = r.wait_state(SessionState::Error).await.unwrap();
    assert!(msg.to_lowercase().contains("refused"), "{msg}");
}

#[tokio::test]
async fn silent_server_times_out() {
    // Accepts TCP but never speaks SSH.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((s, _)) = listener.accept().await {
            held.push(s);
        }
    });
    let h = Harness::new();
    let profile = h.profile(port, AuthMethod::Password, None);
    let mut r = h.start(profile, Duration::from_millis(400), Duration::from_millis(400));
    r.wait_state(SessionState::Timeout).await;
}

// ---------------------------------------------------------------- monitoring

enum MonEvent {
    Sample(Sample),
    Status(MonitorStatus),
}

struct ChanMonSink(mpsc::UnboundedSender<MonEvent>);

impl MonitorSink for ChanMonSink {
    fn sample(&self, s: &Sample) {
        let _ = self.0.send(MonEvent::Sample(s.clone()));
    }
    fn status(&self, s: &MonitorStatus) {
        let _ = self.0.send(MonEvent::Status(s.clone()));
    }
}

async fn next_sample(rx: &mut mpsc::UnboundedReceiver<MonEvent>) -> Sample {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("no monitor event").expect("closed") {
            MonEvent::Sample(s) => return s,
            MonEvent::Status(st) => assert_ne!(st.state, MonitorState::Error, "monitor error: {:?}", st.message),
        }
    }
}

#[tokio::test]
async fn monitoring_reuses_session_connection_and_stops_after_disconnect() {
    let (port, observed) = start_server(8, None).await;
    let h = Harness::new();
    let mut s = settings::AppSettings::default();
    s.monitoring.interval_secs = 1;
    h.db.with(|c| settings::save(c, &s)).unwrap();
    let profile = h.profile(port, AuthMethod::Password, None);
    h.secrets.set(&password_account(&profile.id), PASSWORD).unwrap();
    h.db.with(|c| known_hosts::add(c, "127.0.0.1", port, "ssh-ed25519", &host_key(8).public_key().to_openssh().unwrap(), "SHA256:x")).unwrap();

    // Without a session, monitoring never connects on its own: it waits, then stops.
    let mgr = Arc::new(MonitorManager::default());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let deps = || MonitorDeps { db: h.db.clone(), connections: h.connections.clone(), sink: Arc::new(ChanMonSink(tx.clone())) };

    let mut r = h.start(profile.clone(), T, T);
    r.wait_state(SessionState::Connected).await;
    mgr.start(&profile.id, deps());
    mgr.start(&profile.id, deps()); // idempotent

    let first = next_sample(&mut rx).await;
    assert_eq!(first.cpu_percent, None, "first sample cannot know CPU% yet");
    assert_eq!(first.hostname.as_deref(), Some("test-host"));
    assert_eq!(first.mem_used_bytes, Some(1024000 * 1024));
    assert_eq!(first.disks.len(), 1);
    let second = next_sample(&mut rx).await;
    let cpu = second.cpu_percent.expect("cpu from delta");
    assert!((cpu - 20.0).abs() < 0.01, "{cpu}");
    assert!(second.net_rx_bps.unwrap() > 0.0);
    // Detected OS persisted for the server card.
    assert_eq!(h.db.with(|c| servers::get(c, &profile.id)).unwrap().os_detected.as_deref(), Some("Test Linux 1.0"));
    assert!(observed.lock().unwrap().execs.iter().all(|c| c.starts_with("/bin/sh -c '")));

    // Close the only session: monitor reports no connection, then stops itself.
    r.cmd.send(SessionCommand::Close).await.unwrap();
    r.wait_state(SessionState::Disconnected).await;
    let stopped = loop {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.unwrap().unwrap() {
            MonEvent::Status(st) if st.state == MonitorState::Stopped => break st,
            _ => {}
        }
    };
    assert!(stopped.message.unwrap().contains("No open SSH session"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(mgr.active().is_empty());
}
