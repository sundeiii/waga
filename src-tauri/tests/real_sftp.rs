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

/// Connected session + SFTP for the folder/speed test.
async fn connect_real() -> (Arc<russh_sftp::client::SftpSession>, String, mpsc::Sender<SessionCommand>, tokio::task::JoinHandle<()>) {
    let host = env("YORU_TEST_HOST", "127.0.0.1");
    let port: u16 = env("YORU_TEST_PORT", "22").parse().unwrap();
    let user = env("YORU_TEST_USER", &std::env::var("USERNAME").unwrap_or_default());
    let key = env("YORU_TEST_KEY", &format!("{}\\.ssh\\yoru_test_ed25519", std::env::var("USERPROFILE").unwrap_or_default()));
    let db = Arc::new(Database::open_in_memory().unwrap());
    let profile = db
        .with(|c| {
            servers::insert(c, &ServerInput {
                name: "real".into(), host, port, username: user, auth_method: AuthMethod::Key, key_path: Some(key),
                profile_kind: ProfileKind::Windows, group_name: None, tags: vec![], notes: String::new(), favorite: false, auto_reconnect: false, keepalive: true, jump_host_id: None,
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
            prompt_timeout: Duration::from_secs(10), db, secrets: Arc::new(MemoryStore::default()), prompts: prompts.clone(),
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
    let s = SftpPool::default().get(&profile.id, &connections).await.unwrap();
    (s, profile.id, cmd_tx, task)
}

#[tokio::test]
#[ignore]
async fn real_sftp_folder_roundtrip_and_throughput() {
    let (s, sid, cmd, task) = connect_real().await;
    let home = remote::home(&s).await.unwrap();
    let tm = Arc::new(TransferManager::default());
    let (ttx, mut trx) = mpsc::unbounded_channel();
    let sink: Arc<dyn TransferSink> = Arc::new(TSink(ttx));

    // Nested local folder.
    let src_root = tempfile::tempdir().unwrap();
    let folder = src_root.path().join(format!("yoru-dir-{}", &uuid::Uuid::new_v4().to_string()[..6]));
    std::fs::create_dir_all(folder.join("sub/deeper")).unwrap();
    std::fs::write(folder.join("a.txt"), b"alpha").unwrap();
    std::fs::write(folder.join("sub/b.bin"), vec![42u8; 300_000]).unwrap();
    std::fs::write(folder.join("sub/deeper/c.txt"), "ünïcode ✓".as_bytes()).unwrap();
    let fname = folder.file_name().unwrap().to_string_lossy().into_owned();

    // Upload folder.
    let plan = plan_upload(&s, &sid, &folder, &home, false).await.unwrap();
    assert!(plan.is_dir);
    assert_eq!(plan.files.len(), 3);
    let id = tm.start(s.clone(), plan, sink.clone());
    let done = finish(&mut trx, &id).await;
    assert_eq!(done.state, TransferState::Done, "{:?}", done.error);
    assert_eq!((done.files_done, done.files_total), (3, 3));
    // Re-uploading the same folder needs consent.
    assert!(matches!(plan_upload(&s, &sid, &folder, &home, false).await, Err(AppError::Exists(_))));

    // Download it back and compare every file.
    let back = tempfile::tempdir().unwrap();
    let rdir = format!("{home}/{fname}");
    let id = tm.start(s.clone(), plan_download(&s, &sid, &rdir, back.path(), false).await.unwrap(), sink.clone());
    assert_eq!(finish(&mut trx, &id).await.state, TransferState::Done);
    for rel in ["a.txt", "sub/b.bin", "sub/deeper/c.txt"] {
        assert_eq!(std::fs::read(back.path().join(&fname).join(rel)).unwrap(), std::fs::read(folder.join(rel)).unwrap(), "{rel}");
    }

    // Throughput over loopback (64 MiB each way).
    let big = src_root.path().join("speed.bin");
    std::fs::write(&big, (0..64 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect::<Vec<_>>()).unwrap();
    let t0 = std::time::Instant::now();
    let id = tm.start(s.clone(), plan_upload(&s, &sid, &big, &rdir, false).await.unwrap(), sink.clone());
    assert_eq!(finish(&mut trx, &id).await.state, TransferState::Done);
    let up = 64.0 / t0.elapsed().as_secs_f64();
    let dl_dir = tempfile::tempdir().unwrap();
    let t1 = std::time::Instant::now();
    let id = tm.start(s.clone(), plan_download(&s, &sid, &format!("{rdir}/speed.bin"), dl_dir.path(), false).await.unwrap(), sink.clone());
    assert_eq!(finish(&mut trx, &id).await.state, TransferState::Done);
    let down = 64.0 / t1.elapsed().as_secs_f64();
    println!("throughput over loopback: upload {up:.0} MiB/s, download {down:.0} MiB/s");

    // Clean up remote tree (files, then dirs deepest-first).
    for f in ["speed.bin", "a.txt", "sub/b.bin", "sub/deeper/c.txt"] {
        let _ = s.remove_file(format!("{rdir}/{f}")).await;
    }
    for d in ["sub/deeper", "sub", ""] {
        let _ = s.remove_dir(if d.is_empty() { rdir.clone() } else { format!("{rdir}/{d}") }).await;
    }
    assert!(!s.try_exists(rdir.clone()).await.unwrap());
    cmd.send(SessionCommand::Close).await.unwrap();
    task.await.unwrap();
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
                profile_kind: ProfileKind::Windows, group_name: None, tags: vec![], notes: String::new(), favorite: false, auto_reconnect: false, keepalive: true, jump_host_id: None,
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
