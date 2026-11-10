//! Keyboard-interactive / 2FA against an in-process SSH server over real TCP.

use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Response, Server as _, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use yoru_lib::credentials::{password_account, MemoryStore, SecretStore};
use yoru_lib::database::servers::{self, AuthMethod, ProfileKind, ServerInput};
use yoru_lib::database::{known_hosts, Database};
use yoru_lib::ssh::connections::ConnectionRegistry;
use yoru_lib::ssh::events::{SessionEvent, SessionSink, SessionState};
use yoru_lib::ssh::prompts::PromptRegistry;
use yoru_lib::ssh::session::{run_session, SessionCommand, SessionContext};

const PASSWORD: &str = "hunter2";
const CODE: &str = "123456";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// PasswordAuthentication no; keyboard-interactive asks password, then code.
    PasswordThenCode,
    /// AuthenticationMethods publickey,keyboard-interactive.
    KeyThenCode,
}

#[derive(Clone)]
struct Srv {
    mode: Mode,
    client_key: Option<PublicKey>,
    stage: Arc<Mutex<u8>>,
    rounds: Arc<Mutex<Vec<String>>>,
}

impl server::Server for Srv {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        let mut c = self.clone();
        c.stage = Arc::new(Mutex::new(0));
        c
    }
}

fn only_kbd(partial: bool) -> Auth {
    Auth::Reject { proceed_with_methods: Some(MethodSet::from(&[MethodKind::KeyboardInteractive][..])), partial_success: partial }
}

fn ask(prompt: &'static str, echo: bool) -> Auth {
    Auth::Partial { name: Cow::Borrowed("2FA"), instructions: Cow::Borrowed("Check your authenticator app"), prompts: Cow::Owned(vec![(Cow::Borrowed(prompt), echo)]) }
}

impl server::Handler for Srv {
    type Error = russh::Error;

    async fn auth_password(&mut self, _u: &str, _p: &str) -> Result<Auth, Self::Error> {
        Ok(only_kbd(false)) // plain password method is disabled
    }

    async fn auth_publickey(&mut self, _u: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        let ok = self.mode == Mode::KeyThenCode && self.client_key.as_ref().is_some_and(|k| k.key_data() == key.key_data());
        Ok(if ok { only_kbd(true) } else { Auth::reject() })
    }

    async fn auth_keyboard_interactive<'a>(&'a mut self, _u: &str, _sub: &str, response: Option<Response<'a>>) -> Result<Auth, Self::Error> {
        let answers: Vec<String> = response.map(|r| r.map(|b| String::from_utf8_lossy(&b).into_owned()).collect()).unwrap_or_default();
        let mut stage = self.stage.lock().unwrap();
        self.rounds.lock().unwrap().push(answers.join("|"));
        let next = match (self.mode, *stage) {
            (Mode::PasswordThenCode, 0) => (1, ask("Password: ", false)),
            (Mode::PasswordThenCode, 1) if answers == [PASSWORD] => (2, ask("Verification code: ", true)),
            (Mode::PasswordThenCode, 2) if answers == [CODE] => (3, Auth::Accept),
            (Mode::KeyThenCode, 0) => (1, ask("Verification code: ", true)),
            (Mode::KeyThenCode, 1) if answers == [CODE] => (2, Auth::Accept),
            _ => (99, Auth::reject()),
        };
        *stage = next.0;
        Ok(next.1)
    }

    async fn channel_open_session(&mut self, _c: Channel<Msg>, reply: server::ChannelOpenHandle, _s: &mut Session) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn shell_request(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        session.data(channel, b"2fa-ok\r\n".to_vec())?;
        Ok(())
    }
}

fn key(seed: u8) -> PrivateKey {
    PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32]))
}

async fn start(mode: Mode, client_key: Option<PublicKey>) -> (u16, Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let rounds = Arc::new(Mutex::new(Vec::new()));
    let cfg = Arc::new(server::Config { keys: vec![key(50)], auth_rejection_time: Duration::from_millis(5), auth_rejection_time_initial: Some(Duration::ZERO), ..Default::default() });
    let mut s = Srv { mode, client_key, stage: Arc::new(Mutex::new(0)), rounds: rounds.clone() };
    tokio::spawn(async move {
        let _ = s.run_on_socket(cfg, &l).await;
    });
    (port, rounds)
}

struct Sink(mpsc::UnboundedSender<SessionEvent>);
impl SessionSink for Sink {
    fn event(&self, e: SessionEvent) {
        let _ = self.0.send(e);
    }
    fn output(&self, _: Vec<u8>) {}
}

struct H {
    db: Arc<Database>,
    secrets: Arc<MemoryStore>,
    prompts: Arc<PromptRegistry>,
}

impl H {
    fn new() -> Self {
        Self { db: Arc::new(Database::open_in_memory().unwrap()), secrets: Arc::new(MemoryStore::default()), prompts: Arc::new(PromptRegistry::default()) }
    }
    fn start(&self, port: u16, auth: AuthMethod, key_path: Option<String>) -> (String, mpsc::UnboundedReceiver<SessionEvent>, mpsc::Sender<SessionCommand>, String) {
        let p = self
            .db
            .with(|c| {
                servers::insert(c, &ServerInput {
                    name: "2fa".into(), host: "127.0.0.1".into(), port, username: "u".into(), auth_method: auth, key_path, profile_kind: ProfileKind::Linux,
                    group_name: None, tags: vec![], notes: String::new(), favorite: false, auto_reconnect: false, keepalive: false,
                })
            })
            .unwrap();
        self.db.with(|c| known_hosts::add(c, "127.0.0.1", port, "ssh-ed25519", &key(50).public_key().to_openssh().unwrap(), "SHA256:x")).unwrap();
        let (etx, erx) = mpsc::unbounded_channel();
        let (ctx_tx, crx) = mpsc::channel(8);
        let sid = uuid::Uuid::new_v4().to_string();
        tokio::spawn(run_session(
            SessionContext {
                sid: sid.clone(), profile: p.clone(), cols: 80, rows: 24, connect_timeout: Duration::from_secs(5), keepalive: None,
                prompt_timeout: Duration::from_secs(5), db: self.db.clone(), secrets: self.secrets.clone(), prompts: self.prompts.clone(),
                sink: Arc::new(Sink(etx)), connections: Arc::new(ConnectionRegistry::default()),
            },
            crx,
        ));
        (sid, erx, ctx_tx, p.id)
    }
}

async fn next(rx: &mut mpsc::UnboundedReceiver<SessionEvent>) -> SessionEvent {
    tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("no event").unwrap()
}

/// Wait for either an interactive prompt or a terminal/connected state.
async fn until_prompt_or_state(rx: &mut mpsc::UnboundedReceiver<SessionEvent>) -> SessionEvent {
    loop {
        let e = next(rx).await;
        match &e {
            SessionEvent::InteractivePrompt { .. } | SessionEvent::SecretPrompt { .. } => return e,
            SessionEvent::Status { state, .. } if *state == SessionState::Connected || state.is_terminal() => return e,
            _ => {}
        }
    }
}

#[tokio::test]
async fn saved_password_is_prefilled_then_only_the_code_is_asked() {
    let (port, rounds) = start(Mode::PasswordThenCode, None).await;
    let h = H::new();
    let (sid, mut rx, _cmd, server_id) = {
        let r = h.start(port, AuthMethod::Password, None);
        (r.0, r.1, r.2, r.3)
    };
    let _ = &server_id;
    // Password is saved before connecting.
    h.secrets.set(&password_account(&server_id), PASSWORD).unwrap();

    match until_prompt_or_state(&mut rx).await {
        SessionEvent::InteractivePrompt { prompts, instructions, .. } => {
            assert_eq!(prompts.len(), 1);
            assert!(prompts[0].prompt.contains("Verification code"), "only the code is asked: {:?}", prompts[0].prompt);
            assert!(prompts[0].echo);
            assert_eq!(instructions, "Check your authenticator app");
        }
        other => panic!("expected the 2FA prompt, got {other:?}"),
    }
    h.prompts.answer_interactive(&sid, Some(vec![Zeroizing::new(CODE.into())])).unwrap();
    match until_prompt_or_state(&mut rx).await {
        SessionEvent::Status { state: SessionState::Connected, .. } => {}
        other => panic!("expected connected, got {other:?}"),
    }
    let r = rounds.lock().unwrap().clone();
    assert_eq!(r, vec!["".to_string(), PASSWORD.to_string(), CODE.to_string()]);
}

#[tokio::test]
async fn without_saved_password_user_answers_both_prompts() {
    let (port, _) = start(Mode::PasswordThenCode, None).await;
    let h = H::new();
    let (sid, mut rx, _cmd, _id) = h.start(port, AuthMethod::Password, None);

    // First Yoru asks for the password (its own secret prompt) and tries the password method.
    match until_prompt_or_state(&mut rx).await {
        SessionEvent::SecretPrompt { .. } => h.prompts.answer_secret(&sid, Some(yoru_lib::ssh::prompts::SecretAnswer { secret: Zeroizing::new(PASSWORD.into()), remember: false })).unwrap(),
        other => panic!("expected password prompt, got {other:?}"),
    }
    // That password is reused for the keyboard-interactive "Password:" prompt; only the code is asked.
    match until_prompt_or_state(&mut rx).await {
        SessionEvent::InteractivePrompt { prompts, .. } => assert!(prompts[0].prompt.contains("code")),
        other => panic!("expected code prompt, got {other:?}"),
    }
    h.prompts.answer_interactive(&sid, Some(vec![Zeroizing::new(CODE.into())])).unwrap();
    assert!(matches!(until_prompt_or_state(&mut rx).await, SessionEvent::Status { state: SessionState::Connected, .. }));
}

#[tokio::test]
async fn wrong_code_fails_and_cancel_fails() {
    let (port, _) = start(Mode::PasswordThenCode, None).await;
    let h = H::new();
    let (sid, mut rx, _c, id) = h.start(port, AuthMethod::Password, None);
    h.secrets.set(&password_account(&id), PASSWORD).unwrap();
    assert!(matches!(until_prompt_or_state(&mut rx).await, SessionEvent::InteractivePrompt { .. }));
    h.prompts.answer_interactive(&sid, Some(vec![Zeroizing::new("000000".into())])).unwrap();
    match until_prompt_or_state(&mut rx).await {
        SessionEvent::Status { state: SessionState::AuthFailed, .. } => {}
        SessionEvent::SecretPrompt { .. } => {} // retry offered: also acceptable, never "connected"
        other => panic!("wrong code must not connect: {other:?}"),
    }

    let (sid2, mut rx2, _c2, id2) = h.start(port, AuthMethod::Password, None);
    h.secrets.set(&password_account(&id2), PASSWORD).unwrap();
    assert!(matches!(until_prompt_or_state(&mut rx2).await, SessionEvent::InteractivePrompt { .. }));
    h.prompts.answer_interactive(&sid2, None).unwrap();
    match until_prompt_or_state(&mut rx2).await {
        SessionEvent::Status { state: SessionState::AuthFailed, message } => assert!(message.unwrap().contains("cancelled")),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn key_then_code() {
    let client = key(51);
    let (port, _) = start(Mode::KeyThenCode, Some(client.public_key().clone())).await;
    let dir = tempfile::tempdir().unwrap();
    let kp = dir.path().join("id");
    std::fs::write(&kp, client.to_openssh(russh::keys::ssh_key::LineEnding::LF).unwrap().as_bytes()).unwrap();
    let h = H::new();
    let (sid, mut rx, _c, _id) = h.start(port, AuthMethod::Key, Some(kp.to_string_lossy().into_owned()));
    match until_prompt_or_state(&mut rx).await {
        SessionEvent::InteractivePrompt { prompts, .. } => assert!(prompts[0].prompt.contains("code")),
        other => panic!("{other:?}"),
    }
    h.prompts.answer_interactive(&sid, Some(vec![Zeroizing::new(CODE.into())])).unwrap();
    assert!(matches!(until_prompt_or_state(&mut rx).await, SessionEvent::Status { state: SessionState::Connected, .. }));
}
