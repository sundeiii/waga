//! Real local terminal test: spawns cmd.exe in a Windows pseudo console.
#![cfg(windows)]

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use yoru_lib::local_term::profiles::{load_profiles, LocalProfile};
use yoru_lib::local_term::session::run_local;
use yoru_lib::ssh::events::{SessionEvent, SessionSink, SessionState};
use yoru_lib::ssh::session::SessionCommand;

struct Sink(mpsc::UnboundedSender<SessionEvent>, mpsc::UnboundedSender<Vec<u8>>);
impl SessionSink for Sink {
    fn event(&self, e: SessionEvent) {
        let _ = self.0.send(e);
    }
    fn output(&self, b: Vec<u8>) {
        let _ = self.1.send(b);
    }
}

fn cmd_profile() -> LocalProfile {
    load_profiles().into_iter().find(|p| p.program.to_lowercase().ends_with("cmd.exe")).expect("a cmd.exe profile")
}

#[tokio::test]
async fn cmd_runs_in_conpty_echoes_and_exits() {
    let (etx, mut erx) = mpsc::unbounded_channel();
    let (otx, mut orx) = mpsc::unbounded_channel();
    let (ctx, crx) = mpsc::channel(16);
    let task = tokio::spawn(run_local(cmd_profile(), 100, 30, Arc::new(Sink(etx, otx)), crx));

    loop {
        match tokio::time::timeout(Duration::from_secs(10), erx.recv()).await.unwrap().unwrap() {
            SessionEvent::Status { state: SessionState::Connected, .. } => break,
            SessionEvent::Status { state, message } if state.is_terminal() => panic!("{state:?} {message:?}"),
            _ => {}
        }
    }
    ctx.send(SessionCommand::Resize { cols: 120, rows: 40 }).await.unwrap();
    ctx.send(SessionCommand::Data(b"echo yoru-local-%COMPUTERNAME%\r".to_vec())).await.unwrap();
    let want = format!("yoru-local-{}", std::env::var("COMPUTERNAME").unwrap());
    let mut out = String::new();
    while !out.contains(&want) {
        let chunk = tokio::time::timeout(Duration::from_secs(10), orx.recv()).await.unwrap_or_else(|_| panic!("got {out:?}")).unwrap();
        let s = String::from_utf8_lossy(&chunk);
        // ConPTY asks for the cursor position at startup; reply like a real
        // terminal (xterm.js does this automatically in the app).
        if s.contains("\x1b[6n") {
            ctx.send(SessionCommand::Data(b"\x1b[1;1R".to_vec())).await.unwrap();
        }
        out.push_str(&s);
    }

    ctx.send(SessionCommand::Data(b"exit 3\r".to_vec())).await.unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), erx.recv()).await.unwrap().unwrap() {
            SessionEvent::Status { state: SessionState::Disconnected, message } => {
                assert_eq!(message.as_deref(), Some("Process exited with code 3"));
                break;
            }
            _ => {}
        }
    }
    task.await.unwrap();
}

#[tokio::test]
async fn close_kills_the_process() {
    let (etx, mut erx) = mpsc::unbounded_channel();
    let (otx, _orx) = mpsc::unbounded_channel();
    let (ctx, crx) = mpsc::channel(16);
    let task = tokio::spawn(run_local(cmd_profile(), 80, 24, Arc::new(Sink(etx, otx)), crx));
    loop {
        if let SessionEvent::Status { state: SessionState::Connected, .. } = tokio::time::timeout(Duration::from_secs(10), erx.recv()).await.unwrap().unwrap() {
            break;
        }
    }
    ctx.send(SessionCommand::Close).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), task).await.expect("session ended").unwrap();
}
