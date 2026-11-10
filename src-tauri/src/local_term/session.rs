//! Local shell in a Windows pseudo console (ConPTY via portable-pty).
//! Uses the same SessionCommand/SessionSink contract as SSH sessions so the
//! frontend terminal code and write/resize/close commands are shared.

use std::io::{Read, Write};
use std::sync::Arc;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::sync::{mpsc, oneshot};

use super::profiles::LocalProfile;
use crate::ssh::events::{SessionEvent, SessionSink, SessionState};
use crate::ssh::session::SessionCommand;

fn size(cols: u32, rows: u32) -> PtySize {
    PtySize { rows: rows.clamp(2, 500) as u16, cols: cols.clamp(10, 1000) as u16, pixel_width: 0, pixel_height: 0 }
}

pub async fn run_local(profile: LocalProfile, cols: u32, rows: u32, sink: Arc<dyn SessionSink>, mut commands: mpsc::Receiver<SessionCommand>) {
    let emit = |state, msg: Option<String>| sink.event(SessionEvent::status(state, msg));
    emit(SessionState::Connecting, Some(format!("Starting {}", profile.name)));

    let pair = match native_pty_system().openpty(size(cols, rows)) {
        Ok(p) => p,
        Err(e) => return emit(SessionState::Error, Some(format!("Could not create pseudo console: {e}"))),
    };
    let mut cmd = CommandBuilder::new(&profile.argv[0]);
    cmd.args(&profile.argv[1..]);
    let cwd = profile
        .cwd
        .clone()
        .filter(|d| d.is_dir())
        .or_else(|| std::env::var_os("USERPROFILE").map(Into::into));
    if let Some(d) = cwd {
        cmd.cwd(d);
    }
    cmd.env("TERM_PROGRAM", "yoru");

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => return emit(SessionState::Error, Some(format!("Could not start {}: {e}", profile.program))),
    };
    drop(pair.slave);
    let master = pair.master;
    let (mut reader, mut writer) = match (master.try_clone_reader(), master.take_writer()) {
        (Ok(r), Ok(w)) => (r, w),
        _ => {
            let _ = child.kill();
            return emit(SessionState::Error, Some("Could not attach to pseudo console".into()));
        }
    };

    // Announce before output starts flowing so the UI accepts input
    // (ConPTY immediately asks for the cursor position).
    emit(SessionState::Connected, Some(format!("{} ({})", profile.name, profile.program)));

    // Blocking reader -> sink (dedicated thread; ConPTY pipes are blocking).
    let out_sink = sink.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => out_sink.output(buf[..n].to_vec()),
            }
        }
    });

    // Blocking wait for process exit.
    let mut killer = child.clone_killer();
    let (exit_tx, mut exit_rx) = oneshot::channel();
    std::thread::spawn(move || {
        let status = child.wait().map(|s| s.exit_code());
        let _ = exit_tx.send(status);
    });

    let reason = loop {
        tokio::select! {
            status = &mut exit_rx => {
                break match status {
                    Ok(Ok(0)) => "Process exited".to_string(),
                    Ok(Ok(code)) => format!("Process exited with code {code}"),
                    _ => "Process ended".to_string(),
                };
            }
            cmd = commands.recv() => match cmd {
                Some(SessionCommand::Data(bytes)) => {
                    if writer.write_all(&bytes).and_then(|_| writer.flush()).is_err() {
                        break "Process input closed".to_string();
                    }
                }
                Some(SessionCommand::Resize { cols, rows }) => {
                    let _ = master.resize(size(cols, rows));
                }
                Some(SessionCommand::Close) | None => {
                    let _ = killer.kill();
                    break "Closed".to_string();
                }
            },
        }
    };
    // Dropping the master closes the pseudo console, ending the reader thread.
    drop(writer);
    drop(master);
    emit(SessionState::Disconnected, Some(reason));
}
