//! Local port forwarding over a real SSH session (in-process server over TCP).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::PrivateKey;
use russh::server::{self, Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use yoru_lib::credentials::{password_account, MemoryStore, SecretStore};
use yoru_lib::database::servers::{self, AuthMethod, ProfileKind, ServerInput};
use yoru_lib::database::{known_hosts, Database};
use yoru_lib::ssh::connections::ConnectionRegistry;
use yoru_lib::ssh::events::{SessionEvent, SessionSink, SessionState};
use yoru_lib::ssh::forwarding::{ForwardManager, ForwardSink, ForwardState, ForwardStatus};
use yoru_lib::ssh::prompts::PromptRegistry;
use yoru_lib::ssh::session::{run_session, SessionCommand, SessionContext};

#[derive(Clone)]
struct Srv;
impl server::Server for Srv {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        Srv
    }
}
impl server::Handler for Srv {
    type Error = russh::Error;
    async fn auth_password(&mut self, _u: &str, p: &str) -> Result<Auth, Self::Error> {
        Ok(if p == "pw" { Auth::Accept } else { Auth::reject() })
    }
    async fn channel_open_session(&mut self, _c: Channel<Msg>, reply: server::ChannelOpenHandle, _s: &mut Session) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }
    async fn shell_request(&mut self, _c: ChannelId, _s: &mut Session) -> Result<(), Self::Error> {
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    async fn channel_open_direct_tcpip(&mut self, channel: Channel<Msg>, host: &str, port: u32, _oa: &str, _op: u32, reply: server::ChannelOpenHandle, _s: &mut Session) -> Result<(), Self::Error> {
        let Ok(mut tcp) = TcpStream::connect((host, port as u16)).await else {
            reply.reject(russh::ChannelOpenFailure::ConnectFailed).await;
            return Ok(());
        };
        reply.accept().await;
        tokio::spawn(async move {
            let mut s = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut s, &mut tcp).await;
        });
        Ok(())
    }
}

struct Sink(mpsc::UnboundedSender<SessionEvent>);
impl SessionSink for Sink {
    fn event(&self, e: SessionEvent) {
        let _ = self.0.send(e);
    }
    fn output(&self, _: Vec<u8>) {}
}

#[derive(Default)]
struct FSink(Mutex<Vec<ForwardStatus>>);
impl ForwardSink for FSink {
    fn update(&self, s: &ForwardStatus) {
        self.0.lock().unwrap().push(s.clone());
    }
}

async fn echo_server() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = l.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok(n) = s.read(&mut buf).await {
                    if n == 0 || s.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn local_forward_relays_and_stops_with_session() {
    let key = PrivateKey::from(Ed25519Keypair::from_seed(&[70; 32]));
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ssh_port = l.local_addr().unwrap().port();
    let cfg = Arc::new(server::Config { keys: vec![key.clone()], ..Default::default() });
    tokio::spawn(async move {
        let _ = Srv.run_on_socket(cfg, &l).await;
    });
    let echo = echo_server().await;

    let db = Arc::new(Database::open_in_memory().unwrap());
    let secrets = Arc::new(MemoryStore::default());
    let p = db
        .with(|c| {
            servers::insert(c, &ServerInput {
                name: "fw".into(), host: "127.0.0.1".into(), port: ssh_port, username: "u".into(), auth_method: AuthMethod::Password, key_path: None,
                profile_kind: ProfileKind::Linux, group_name: None, tags: vec![], notes: String::new(), favorite: false, auto_reconnect: false, keepalive: false, jump_host_id: None,
            })
        })
        .unwrap();
    secrets.set(&password_account(&p.id), "pw").unwrap();
    db.with(|c| known_hosts::add(c, "127.0.0.1", ssh_port, "ssh-ed25519", &key.public_key().to_openssh().unwrap(), "x")).unwrap();

    let connections = Arc::new(ConnectionRegistry::default());
    let (etx, mut erx) = mpsc::unbounded_channel();
    let (cmd, crx) = mpsc::channel(8);
    let task = tokio::spawn(run_session(
        SessionContext {
            sid: uuid::Uuid::new_v4().to_string(), profile: p.clone(), cols: 80, rows: 24, connect_timeout: Duration::from_secs(5), keepalive: None,
            prompt_timeout: Duration::from_secs(5), db, secrets, prompts: Arc::new(PromptRegistry::default()), sink: Arc::new(Sink(etx)), connections: connections.clone(),
        },
        crx,
    ));
    loop {
        match tokio::time::timeout(Duration::from_secs(10), erx.recv()).await.unwrap().unwrap() {
            SessionEvent::Status { state: SessionState::Connected, .. } => break,
            SessionEvent::Status { state, message } if state.is_terminal() => panic!("{state:?} {message:?}"),
            _ => {}
        }
    }

    let mgr = Arc::new(ForwardManager::default());
    let sink = Arc::new(FSink::default());
    // Refuses without a session for that server.
    assert!(mgr.start(connections.clone(), sink.clone(), "other", None, 0, "127.0.0.1", echo).await.is_err());

    let st = mgr.start(connections.clone(), sink.clone(), &p.id, None, 0, "127.0.0.1", echo).await.unwrap();
    assert_ne!(st.local_port, 0);

    // Two clients through the tunnel.
    for msg in [&b"hello through yoru"[..], &b"second"[..]] {
        let mut c = TcpStream::connect(("127.0.0.1", st.local_port)).await.unwrap();
        c.write_all(msg).await.unwrap();
        let mut buf = vec![0u8; msg.len()];
        tokio::time::timeout(Duration::from_secs(5), c.read_exact(&mut buf)).await.unwrap().unwrap();
        assert_eq!(buf, msg);
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let listed = mgr.list();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].total_connections >= 2);

    // Same port can't be bound twice.
    let busy = mgr.start(connections.clone(), sink.clone(), &p.id, None, st.local_port, "127.0.0.1", echo).await;
    assert!(busy.unwrap_err().to_string().contains("already in use"));

    // Closing the SSH session stops the forward on its own.
    cmd.send(SessionCommand::Close).await.unwrap();
    task.await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    while !mgr.list().is_empty() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(mgr.list().is_empty(), "forward stopped after session closed");
    let last = sink.0.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last.state, ForwardState::Stopped);
    assert_eq!(last.message.as_deref(), Some("SSH session closed"));
    assert!(TcpStream::connect(("127.0.0.1", st.local_port)).await.is_err(), "listener closed");
}
