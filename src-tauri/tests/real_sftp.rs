//! Opt-in SFTP end-to-end test against a REAL OpenSSH server.
//! Run:  cargo test --test real_sftp -- --ignored --nocapture
//! Same env vars as real_sshd.rs. Works in a fresh temp folder in the remote home
//! and removes it afterwards.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use yoru_lib::credentials::MemoryStore;
use yoru_lib::database::servers::{self, AuthMethod, ProfileKind, ServerInput};
use yoru_lib::database::{known_hosts, Database};
use yoru_lib::error::AppError;
use yoru_lib::sftp::remote::{self, SftpPool};
use yoru_lib::sftp::transfer::{plan_download, plan_upload, TransferEvent, TransferManager, TransferSink, TransferState};
use yoru_lib::ssh::connections::ConnectionRegistry;
use yoru_lib::ssh::events::{SessionEvent, SessionSink, SessionState};
use yoru_lib::ssh::prompts::PromptRegistry;
use yoru_lib::ssh::session::{run_session, SessionCommand, SessionContext};

struct Sink(mpsc::UnboundedSender<SessionEvent>);
impl SessionSink for Sink {
    fn event(&self, e: SessionEvent) {
        let _ = self.0.send(e);
    }
    fn output(&self, _: Vec<u8>) {}
}

struct TSink(mpsc::UnboundedSender<TransferEvent>);
impl TransferSink for TSink {
    fn update(&self, e: &TransferEvent) {
        let _ = self.0.send(e.clone());
    }
}

fn env(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

async fn finish(rx: &mut mpsc::UnboundedReceiver<TransferEvent>, id: &str) -> TransferEvent {
    loop {
        let e = tokio::time::timeout(Duration::from_secs(60), rx.recv()).await.expect("transfer stalled").unwrap();
        if e.id == id && e.state != TransferState::Running {
            return e;
        }
    }
}

#[tokio::test]
#[ignore]
async fn real_sftp_roundtrip() {
    let host = env("YORU_TEST_HOST", "127.0.0.1");
    let port: u16 = env("YORU_TEST_PORT", "22").parse().unwrap();
    let user = env("YORU_TEST_USER", &std::env::var("USERNAME").unwrap_or_default());
    let key = env("YORU_TEST_KEY", &format!("{}\\.ssh\\yoru_test_ed25519", std::env::var("USERPROFILE").unwrap_or_default()));

    let db = Arc::new(Database::open_in_memory().unwrap());
    let profile = db
        .with(|c| {
            servers::insert(c, &ServerInput {
                name: "real".into(), host: host.clone(), port, username: user.clone(), auth_method: AuthMethod::Key, key_path: Some(key.clone()),
                profile_kind: ProfileKind::Windows, group_name: None, tags: vec![], notes: String::new(), favorite: false, auto_reconnect: false, keepalive: true,
            })
        })
        .unwrap();
    let connections = Arc::new(ConnectionRegistry::default());
    let prompts = Arc::new(PromptRegistry::default());
    let (etx, mut erx) = mpsc::unbounded_channel();
    let (cmd_tx, cmd_rx) = mpsc::channel(8);
    let sid = uuid::Uuid::new_v4().to_string();
    let task = tokio::spawn(run_session(
        SessionContext {
            sid: sid.clone(), profile: profile.clone(), cols: 80, rows: 24, connect_timeout: Duration::from_secs(10), keepalive: None,
            prompt_timeout: Duration::from_secs(10), db: db.clone(), secrets: Arc::new(MemoryStore::default()), prompts: prompts.clone(),
            sink: Arc::new(Sink(etx)), connections: connections.clone(),
        },
        cmd_rx,
    ));
    loop {
        match tokio::time::timeout(Duration::from_secs(15), erx.recv()).await.unwrap().unwrap() {
            SessionEvent::HostKeyPrompt { .. } => prompts.answer_host_key(&sid, yoru_lib::security::host_keys::HostKeyAnswer { accept: true, acknowledge_changed: false }).unwrap(),
            SessionEvent::Status { state: SessionState::Connected, .. } => break,
            SessionEvent::Status { state, message } if state.is_terminal() => panic!("{state:?} {message:?}"),
            _ => {}
        }
    }
    assert_eq!(db.with(|c| known_hosts::list_for(c, &host, port)).unwrap().len(), 1);

    // --- browse
    let pool = SftpPool::default();
    let s = pool.get(&profile.id, &connections).await.unwrap();
    let s2 = pool.get(&profile.id, &connections).await.unwrap();
    assert!(Arc::ptr_eq(&s, &s2), "SFTP session reused");
    let home = remote::home(&s).await.unwrap();
    println!("remote home: {home}");
    let dirname = format!("yoru-sftp-test-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let rdir = remote::mkdir(&s, &home, &dirname).await.unwrap();
    assert!(remote::list(&s, &home).await.unwrap().entries.iter().any(|e| e.name == dirname && e.is_dir));
    assert!(remote::mkdir(&s, &home, "../escape").await.is_err());

    // --- upload
    let local = tempfile::tempdir().unwrap();
    let data: Vec<u8> = (0..3 * 1024 * 1024u32).map(|i| (i * 31 % 251) as u8).collect();
    let src = local.path().join("payload.bin");
    std::fs::write(&src, &data).unwrap();
    let tm = Arc::new(TransferManager::default());
    let (ttx, mut trx) = mpsc::unbounded_channel();
    let sink: Arc<dyn TransferSink> = Arc::new(TSink(ttx));

    let id = tm.start(s.clone(), plan_upload(&s, &profile.id, &src, &rdir, false).await.unwrap(), sink.clone());
    let done = finish(&mut trx, &id).await;
    assert_eq!(done.state, TransferState::Done, "{:?}", done.error);
    assert_eq!(done.bytes, data.len() as u64);
    let listing = remote::list(&s, &rdir).await.unwrap();
    assert_eq!(listing.entries.len(), 1, "no .yoru-partial left behind");
    assert_eq!(listing.entries[0].size, Some(data.len() as u64));

    // Overwrite needs explicit consent.
    assert!(matches!(plan_upload(&s, &profile.id, &src, &rdir, false).await, Err(AppError::Exists(_))));
    let id = tm.start(s.clone(), plan_upload(&s, &profile.id, &src, &rdir, true).await.unwrap(), sink.clone());
    assert_eq!(finish(&mut trx, &id).await.state, TransferState::Done);

    // --- download + byte compare
    let dl = tempfile::tempdir().unwrap();
    let rfile = format!("{rdir}/payload.bin");
    let id = tm.start(s.clone(), plan_download(&s, &profile.id, &rfile, dl.path(), false).await.unwrap(), sink.clone());
    assert_eq!(finish(&mut trx, &id).await.state, TransferState::Done);
    assert_eq!(std::fs::read(dl.path().join("payload.bin")).unwrap(), data, "downloaded bytes identical");
    assert!(matches!(plan_download(&s, &profile.id, &rfile, dl.path(), false).await, Err(AppError::Exists(_))));

    // --- cancel a large upload: nothing must remain under either name
    let big = local.path().join("big.bin");
    std::fs::write(&big, vec![7u8; 96 * 1024 * 1024]).unwrap();
    let id = tm.start(s.clone(), plan_upload(&s, &profile.id, &big, &rdir, false).await.unwrap(), sink.clone());
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(tm.cancel(&id));
    let c = finish(&mut trx, &id).await;
    assert_eq!(c.state, TransferState::Cancelled, "{:?}", c.error);
    let names: Vec<_> = remote::list(&s, &rdir).await.unwrap().entries.into_iter().map(|e| e.name).collect();
    assert_eq!(names, vec!["payload.bin"], "cancelled upload left nothing behind");

    // --- rename / delete
    remote::rename(&s, &rdir, "payload.bin", "renamed.bin").await.unwrap();
    assert!(remote::delete(&s, &home, &dirname, true).await.is_err(), "non-empty folder not deleted");
    remote::delete(&s, &rdir, "renamed.bin", false).await.unwrap();
    remote::delete(&s, &home, &dirname, true).await.unwrap();
    assert!(!remote::list(&s, &home).await.unwrap().entries.iter().any(|e| e.name == dirname));

    cmd_tx.send(SessionCommand::Close).await.unwrap();
    task.await.unwrap();
}
