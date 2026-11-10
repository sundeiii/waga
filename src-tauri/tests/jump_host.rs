//! ProxyJump: SSH through a jump host's direct-tcpip tunnel, over real TCP,
//! with two in-process SSH servers.

use std::sync::Arc;
use std::time::Duration;

use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::PrivateKey;
use russh::server::{self, Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use yoru_lib::credentials::{password_account, MemoryStore, SecretStore};
use yoru_lib::database::servers::{self, AuthMethod, ProfileKind, ServerInput, ServerProfile};
use yoru_lib::database::{known_hosts, Database};
use yoru_lib::security::host_keys::{HostKeyAnswer, HostKeyStatus};
use yoru_lib::ssh::connections::ConnectionRegistry;
use yoru_lib::ssh::events::{SessionEvent, SessionSink, SessionState};
use yoru_lib::ssh::prompts::PromptRegistry;
use yoru_lib::ssh::session::{run_session, SessionCommand, SessionContext};

#[derive(Clone)]
struct Srv {
    password: &'static str,
    allow_forwarding: bool,
    banner: &'static str,
}

impl server::Server for Srv {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}

impl server::Handler for Srv {
    type Error = russh::Error;

    async fn auth_password(&mut self, _u: &str, p: &str) -> Result<Auth, Self::Error> {
        Ok(if p == self.password { Auth::Accept } else { Auth::reject() })
    }

    async fn channel_open_session(&mut self, _c: Channel<Msg>, reply: server::ChannelOpenHandle, _s: &mut Session) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn shell_request(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        session.data(channel, self.banner.as_bytes().to_vec())?;
        Ok(())
    }

    /// Like sshd with AllowTcpForwarding: relay the channel to host:port.
    #[allow(clippy::too_many_arguments)]
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host: &str,
        port: u32,
        _oa: &str,
        _op: u32,
        reply: server::ChannelOpenHandle,
        _s: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.allow_forwarding {
            reply.reject(russh::ChannelOpenFailure::AdministrativelyProhibited).await;
            return Ok(());
        }
        let Ok(mut tcp) = TcpStream::connect((host, port as u16)).await else {
            reply.reject(russh::ChannelOpenFailure::ConnectFailed).await;
            return Ok(());
        };
        reply.accept().await;
        tokio::spawn(async move {
            let mut stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
        });
        Ok(())
    }
}

fn key(seed: u8) -> PrivateKey {
    PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32]))
}

async fn start(seed: u8, srv: Srv) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let cfg = Arc::new(server::Config { keys: vec![key(seed)], auth_rejection_time: Duration::from_millis(5), auth_rejection_time_initial: Some(Duration::ZERO), ..Default::default() });
    let mut s = srv;
    tokio::spawn(async move {
        let _ = s.run_on_socket(cfg, &l).await;
    });
    port
}

struct Sink(mpsc::UnboundedSender<SessionEvent>, mpsc::UnboundedSender<Vec<u8>>);
impl SessionSink for Sink {
    fn event(&self, e: SessionEvent) {
        let _ = self.0.send(e);
    }
    fn output(&self, b: Vec<u8>) {
        let _ = self.1.send(b);
    }
}

fn profile(db: &Database, name: &str, port: u16, jump: Option<String>) -> ServerProfile {
    db.with(|c| {
        servers::insert(c, &ServerInput {
            name: name.into(), host: "127.0.0.1".into(), port, username: "u".into(), auth_method: AuthMethod::Password, key_path: None,
            profile_kind: ProfileKind::Linux, group_name: None, tags: vec![], notes: String::new(), favorite: false, auto_reconnect: false,
            keepalive: false, jump_host_id: jump,
        })
    })
    .unwrap()
}

#[tokio::test]
async fn connects_through_jump_host_and_verifies_both_host_keys() {
    let jport = start(60, Srv { password: "jump-pw", allow_forwarding: true, banner: "jump-shell\r\n" }).await;
    let tport = start(61, Srv { password: "target-pw", allow_forwarding: false, banner: "target-shell\r\n" }).await;

    let db = Arc::new(Database::open_in_memory().unwrap());
    let secrets = Arc::new(MemoryStore::default());
    let jump = profile(&db, "bastion", jport, None);
    let target = profile(&db, "internal", tport, Some(jump.id.clone()));
    secrets.set(&password_account(&jump.id), "jump-pw").unwrap();
    secrets.set(&password_account(&target.id), "target-pw").unwrap();

    let prompts = Arc::new(PromptRegistry::default());
    let connections = Arc::new(ConnectionRegistry::default());
    let (etx, mut erx) = mpsc::unbounded_channel();
    let (otx, mut orx) = mpsc::unbounded_channel();
    let (cmd, crx) = mpsc::channel(8);
    let sid = uuid::Uuid::new_v4().to_string();
    let task = tokio::spawn(run_session(
        SessionContext {
            sid: sid.clone(), profile: target.clone(), cols: 80, rows: 24, connect_timeout: Duration::from_secs(5), keepalive: None,
            prompt_timeout: Duration::from_secs(5), db: db.clone(), secrets: secrets.clone(), prompts: prompts.clone(),
            sink: Arc::new(Sink(etx, otx)), connections: connections.clone(),
        },
        crx,
    ));

    // Two independent host key confirmations: the jump host first, then the target.
    let mut seen_ports = Vec::new();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), erx.recv()).await.unwrap().unwrap() {
            SessionEvent::HostKeyPrompt { port, status, .. } => {
                assert_eq!(status, HostKeyStatus::Unknown);
                seen_ports.push(port);
                prompts.answer_host_key(&sid, HostKeyAnswer { accept: true, acknowledge_changed: false }).unwrap();
            }
            SessionEvent::Status { state: SessionState::Connected, .. } => break,
            SessionEvent::Status { state, message } if state.is_terminal() => panic!("{state:?} {message:?}"),
            _ => {}
        }
    }
    assert_eq!(seen_ports, vec![jport, tport], "jump host verified first, then the target");
    assert_eq!(db.with(|c| known_hosts::list_for(c, "127.0.0.1", tport)).unwrap().len(), 1);

    // The shell is the TARGET's, reached through the tunnel.
    let mut out = String::new();
    while !out.contains("target-shell") {
        let b = tokio::time::timeout(Duration::from_secs(10), orx.recv()).await.expect("no output").unwrap();
        out.push_str(&String::from_utf8_lossy(&b));
    }
    assert!(!out.contains("jump-shell"));
    // Monitoring/SFTP get the target connection.
    assert!(connections.has_server(&target.id));
    assert!(!connections.has_server(&jump.id));

    cmd.send(SessionCommand::Close).await.unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn jump_host_without_forwarding_gives_a_clear_error() {
    let jport = start(62, Srv { password: "jp", allow_forwarding: false, banner: "" }).await;
    let tport = start(63, Srv { password: "tp", allow_forwarding: false, banner: "" }).await;
    let db = Arc::new(Database::open_in_memory().unwrap());
    let secrets = Arc::new(MemoryStore::default());
    let jump = profile(&db, "bastion", jport, None);
    let target = profile(&db, "internal", tport, Some(jump.id.clone()));
    secrets.set(&password_account(&jump.id), "jp").unwrap();
    db.with(|c| known_hosts::add(c, "127.0.0.1", jport, "ssh-ed25519", &key(62).public_key().to_openssh().unwrap(), "x")).unwrap();

    let (etx, mut erx) = mpsc::unbounded_channel();
    let (otx, _orx) = mpsc::unbounded_channel();
    let (_cmd, crx) = mpsc::channel(8);
    tokio::spawn(run_session(
        SessionContext {
            sid: uuid::Uuid::new_v4().to_string(), profile: target, cols: 80, rows: 24, connect_timeout: Duration::from_secs(5), keepalive: None,
            prompt_timeout: Duration::from_secs(5), db, secrets, prompts: Arc::new(PromptRegistry::default()), sink: Arc::new(Sink(etx, otx)),
            connections: Arc::new(ConnectionRegistry::default()),
        },
        crx,
    ));
    loop {
        if let SessionEvent::Status { state, message } = tokio::time::timeout(Duration::from_secs(10), erx.recv()).await.unwrap().unwrap() {
            if state.is_terminal() {
                assert_eq!(state, SessionState::Error);
                let m = message.unwrap();
                assert!(m.contains("bastion could not reach") && m.contains("forwarding"), "{m}");
                break;
            }
        }
    }
}
