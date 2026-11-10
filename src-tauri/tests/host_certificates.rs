//! Host certificates (@cert-authority) over real TCP with an in-process server.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use russh::keys::ssh_key::certificate::{Builder, CertType};
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::{Certificate, PrivateKey};
use russh::server::{self, Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use yoru_lib::credentials::{password_account, MemoryStore, SecretStore};
use yoru_lib::database::servers::{self, AuthMethod, ProfileKind, ServerInput};
use yoru_lib::database::{host_cas, known_hosts, Database};
use yoru_lib::security::host_certs::ca_fingerprint;
use yoru_lib::security::host_keys::HostKeyAnswer;
use yoru_lib::ssh::connections::ConnectionRegistry;
use yoru_lib::ssh::events::{SessionEvent, SessionSink, SessionState};
use yoru_lib::ssh::prompts::PromptRegistry;
use yoru_lib::ssh::session::{run_session, SessionContext};

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
    async fn auth_password(&mut self, _u: &str, _p: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }
    async fn channel_open_session(&mut self, _c: Channel<Msg>, reply: server::ChannelOpenHandle, _s: &mut Session) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }
    async fn shell_request(&mut self, _c: ChannelId, _s: &mut Session) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn key(seed: u8) -> PrivateKey {
    PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32]))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn cert(host: &PrivateKey, ca: &PrivateKey, principal: &str, after: u64, before: u64) -> Certificate {
    let mut b = Builder::new([9u8; 16], host.public_key().key_data().clone(), after, before).unwrap();
    b.cert_type(CertType::Host).unwrap().key_id("srv").unwrap().valid_principal(principal).unwrap();
    b.sign(ca).unwrap()
}

async fn start(host: PrivateKey, c: Certificate) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let cfg = Arc::new(server::Config { keys: vec![host], certificates: vec![c], ..Default::default() });
    tokio::spawn(async move {
        let _ = Srv.run_on_socket(cfg, &l).await;
    });
    port
}

struct Sink(mpsc::UnboundedSender<SessionEvent>);
impl SessionSink for Sink {
    fn event(&self, e: SessionEvent) {
        let _ = self.0.send(e);
    }
    fn output(&self, _: Vec<u8>) {}
}

/// Run a session; auto-accept host key prompts; return (connected?, prompts seen, final message).
async fn connect(db: Arc<Database>, port: u16) -> (bool, u32, Option<String>) {
    let secrets = Arc::new(MemoryStore::default());
    let p = db
        .with(|c| {
            servers::insert(c, &ServerInput {
                name: "c".into(), host: "127.0.0.1".into(), port, username: "u".into(), auth_method: AuthMethod::Password, key_path: None,
                profile_kind: ProfileKind::Linux, group_name: None, tags: vec![], notes: String::new(), favorite: false, auto_reconnect: false, keepalive: false, jump_host_id: None,
            })
        })
        .unwrap();
    secrets.set(&password_account(&p.id), "pw").unwrap();
    let prompts = Arc::new(PromptRegistry::default());
    let (etx, mut erx) = mpsc::unbounded_channel();
    let (_cmd, crx) = mpsc::channel(8);
    let sid = uuid::Uuid::new_v4().to_string();
    tokio::spawn(run_session(
        SessionContext {
            sid: sid.clone(), profile: p, cols: 80, rows: 24, connect_timeout: Duration::from_secs(5), keepalive: None, prompt_timeout: Duration::from_secs(5),
            db, secrets, prompts: prompts.clone(), sink: Arc::new(Sink(etx)), connections: Arc::new(ConnectionRegistry::default()),
        },
        crx,
    ));
    let mut seen = 0;
    loop {
        match tokio::time::timeout(Duration::from_secs(10), erx.recv()).await.unwrap().unwrap() {
            SessionEvent::HostKeyPrompt { .. } => {
                seen += 1;
                prompts.answer_host_key(&sid, HostKeyAnswer { accept: true, acknowledge_changed: false }).unwrap();
            }
            SessionEvent::Status { state: SessionState::Connected, .. } => return (true, seen, None),
            SessionEvent::Status { state, message } if state.is_terminal() => return (false, seen, message),
            _ => {}
        }
    }
}

fn trust_ca(db: &Database, ca: &PrivateKey, pattern: &str) {
    let line = ca.public_key().to_openssh().unwrap();
    db.with(|c| host_cas::insert(c, pattern, &line, &ca_fingerprint(&line).unwrap(), "test CA")).unwrap();
}

#[tokio::test]
async fn trusted_ca_certificate_connects_without_prompt() {
    let (host, ca) = (key(80), key(81));
    let port = start(host.clone(), cert(&host, &ca, "127.0.0.1", now() - 60, now() + 3600)).await;
    let db = Arc::new(Database::open_in_memory().unwrap());
    trust_ca(&db, &ca, "127.0.0.*");
    let (ok, prompts, msg) = connect(db.clone(), port).await;
    assert!(ok, "{msg:?}");
    assert_eq!(prompts, 0, "certificate from a trusted CA needs no fingerprint prompt");
    assert!(db.with(|c| known_hosts::list_for(c, "127.0.0.1", port)).unwrap().is_empty(), "nothing pinned");
}

#[tokio::test]
async fn without_trusted_ca_the_normal_prompt_is_used() {
    let (host, ca) = (key(82), key(83));
    let port = start(host.clone(), cert(&host, &ca, "127.0.0.1", now() - 60, now() + 3600)).await;
    let db = Arc::new(Database::open_in_memory().unwrap());
    let (ok, prompts, _) = connect(db, port).await;
    assert!(ok);
    assert_eq!(prompts, 1, "plain-key client: fingerprint prompt as usual");
}

#[tokio::test]
async fn expired_certificate_from_trusted_ca_is_blocked() {
    let (host, ca) = (key(84), key(85));
    let port = start(host.clone(), cert(&host, &ca, "127.0.0.1", now() - 7200, now() - 3600)).await;
    let db = Arc::new(Database::open_in_memory().unwrap());
    trust_ca(&db, &ca, "*");
    let (ok, prompts, msg) = connect(db, port).await;
    assert!(!ok);
    assert_eq!(prompts, 0, "no 'trust anyway' prompt for an invalid certificate");
    assert!(msg.unwrap().contains("expired"));
}

#[tokio::test]
async fn certificate_for_another_host_is_blocked() {
    let (host, ca) = (key(86), key(87));
    let port = start(host.clone(), cert(&host, &ca, "other.example", now() - 60, now() + 3600)).await;
    let db = Arc::new(Database::open_in_memory().unwrap());
    trust_ca(&db, &ca, "*");
    let (ok, _, msg) = connect(db, port).await;
    assert!(!ok);
    assert!(msg.unwrap().contains("not issued for '127.0.0.1'"));
}
