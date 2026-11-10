//! File transfers with progress and cancellation.
//!
//! Safety: data is written to `<target>.yoru-partial` and renamed into place
//! only after a complete copy, so a cancelled/failed transfer never leaves a
//! truncated file under the real name, and an existing file is only replaced
//! when the user explicitly chose to overwrite.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::paths::{local_join, remote_join, remote_name, sanitize_for_windows, validate_name, PARTIAL_SUFFIX};
use super::remote::{is_not_found, sftp_err};
use crate::error::{AppError, AppResult};

const CHUNK: usize = 256 * 1024;
const PROGRESS_EVERY: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TransferState {
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferEvent {
    pub id: String,
    pub server_id: String,
    pub direction: Direction,
    pub name: String,
    pub source: String,
    pub dest: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub state: TransferState,
    pub error: Option<String>,
}

pub trait TransferSink: Send + Sync + 'static {
    fn update(&self, e: &TransferEvent);
}

#[derive(Default)]
pub struct TransferManager {
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

/// A validated transfer, ready to run.
pub struct Plan {
    pub direction: Direction,
    pub server_id: String,
    pub name: String,
    pub source: String,
    pub dest: String,
    pub total: Option<u64>,
    pub overwrite: bool,
}

pub async fn plan_upload(sftp: &SftpSession, server_id: &str, local_path: &Path, remote_dir: &str, overwrite: bool) -> AppResult<Plan> {
    let meta = tokio::fs::metadata(local_path).await.map_err(|e| AppError::Validation(format!("Cannot read {}: {e}", local_path.display())))?;
    if meta.is_dir() {
        return Err(AppError::Validation("Folder transfers aren't supported yet. Transfer the files inside it instead.".into()));
    }
    let name = local_path.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or_else(|| AppError::Validation("Invalid file".into()))?;
    validate_name(&name).map_err(AppError::Validation)?;
    let dest = remote_join(remote_dir, &name).map_err(AppError::Validation)?;
    if !overwrite && sftp.try_exists(dest.clone()).await.map_err(sftp_err)? {
        return Err(AppError::Exists(format!("'{name}' already exists on the server")));
    }
    Ok(Plan { direction: Direction::Upload, server_id: server_id.into(), name, source: local_path.to_string_lossy().into_owned(), dest, total: Some(meta.len()), overwrite })
}

pub async fn plan_download(sftp: &SftpSession, server_id: &str, remote_path: &str, local_dir: &Path, overwrite: bool) -> AppResult<Plan> {
    let meta = sftp.metadata(remote_path.to_string()).await.map_err(sftp_err)?;
    if meta.is_dir() {
        return Err(AppError::Validation("Folder transfers aren't supported yet. Transfer the files inside it instead.".into()));
    }
    if !local_dir.is_dir() {
        return Err(AppError::Validation(format!("{} is not a folder", local_dir.display())));
    }
    let remote = remote_name(remote_path).to_string();
    let safe = sanitize_for_windows(&remote).map_err(AppError::Validation)?;
    let dest = local_join(local_dir, &safe).map_err(AppError::Validation)?;
    if !overwrite && dest.exists() {
        return Err(AppError::Exists(format!("'{safe}' already exists in this folder")));
    }
    Ok(Plan {
        direction: Direction::Download,
        server_id: server_id.into(),
        name: safe,
        source: remote_path.to_string(),
        dest: dest.to_string_lossy().into_owned(),
        total: meta.size,
        overwrite,
    })
}

impl TransferManager {
    pub fn cancel(&self, id: &str) -> bool {
        match self.cancels.lock().unwrap().get(id) {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }

    pub fn active(&self) -> usize {
        self.cancels.lock().unwrap().len()
    }

    /// Spawn the transfer; returns its id immediately.
    pub fn start(self: &Arc<Self>, sftp: Arc<SftpSession>, plan: Plan, sink: Arc<dyn TransferSink>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancels.lock().unwrap().insert(id.clone(), cancel.clone());
        let me = self.clone();
        let tid = id.clone();
        tokio::spawn(async move {
            let mut ev = TransferEvent {
                id: tid.clone(),
                server_id: plan.server_id.clone(),
                direction: plan.direction,
                name: plan.name.clone(),
                source: plan.source.clone(),
                dest: plan.dest.clone(),
                bytes: 0,
                total: plan.total,
                state: TransferState::Running,
                error: None,
            };
            sink.update(&ev);
            let result = match plan.direction {
                Direction::Upload => upload(&sftp, &plan, &cancel, &mut ev, sink.as_ref()).await,
                Direction::Download => download(&sftp, &plan, &cancel, &mut ev, sink.as_ref()).await,
            };
            match result {
                Ok(()) => ev.state = TransferState::Done,
                Err(_) if cancel.load(Ordering::SeqCst) => ev.state = TransferState::Cancelled,
                Err(e) => {
                    ev.state = TransferState::Failed;
                    ev.error = Some(e.to_string());
                }
            }
            sink.update(&ev);
            me.cancels.lock().unwrap().remove(&tid);
        });
        id
    }
}

fn cancelled(flag: &AtomicBool) -> AppResult<()> {
    if flag.load(Ordering::SeqCst) {
        Err(AppError::Validation("Cancelled".into()))
    } else {
        Ok(())
    }
}

async fn upload(sftp: &SftpSession, plan: &Plan, cancel: &AtomicBool, ev: &mut TransferEvent, sink: &dyn TransferSink) -> AppResult<()> {
    let tmp = format!("{}{PARTIAL_SUFFIX}", plan.dest);
    let copy = async {
        let mut src = tokio::fs::File::open(&plan.source).await?;
        let mut dst = sftp.open_with_flags(tmp.clone(), OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE).await.map_err(sftp_err)?;
        let mut buf = vec![0u8; CHUNK];
        let mut last = Instant::now();
        loop {
            cancelled(cancel)?;
            let n = src.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n]).await.map_err(|e| AppError::Ssh(format!("Write failed: {e}")))?;
            ev.bytes += n as u64;
            if last.elapsed() >= PROGRESS_EVERY {
                sink.update(ev);
                last = Instant::now();
            }
        }
        dst.flush().await.map_err(|e| AppError::Ssh(format!("Write failed: {e}")))?;
        dst.shutdown().await.map_err(|e| AppError::Ssh(format!("Close failed: {e}")))?;
        cancelled(cancel)?;
        // Move into place.
        if plan.overwrite {
            if let Err(e) = sftp.remove_file(plan.dest.clone()).await {
                if !is_not_found(&e) {
                    return Err(sftp_err(e));
                }
            }
        } else if sftp.try_exists(plan.dest.clone()).await.unwrap_or(false) {
            return Err(AppError::Exists(format!("'{}' appeared on the server during upload; not overwritten", plan.name)));
        }
        sftp.rename(tmp.clone(), plan.dest.clone()).await.map_err(sftp_err)
    };
    let res: AppResult<()> = copy.await;
    if res.is_err() {
        let _ = sftp.remove_file(tmp).await;
    }
    res
}

async fn download(sftp: &SftpSession, plan: &Plan, cancel: &AtomicBool, ev: &mut TransferEvent, sink: &dyn TransferSink) -> AppResult<()> {
    let dest = PathBuf::from(&plan.dest);
    let tmp = PathBuf::from(format!("{}{PARTIAL_SUFFIX}", plan.dest));
    let copy = async {
        let mut src = sftp.open_with_flags(plan.source.clone(), OpenFlags::READ).await.map_err(sftp_err)?;
        let mut dst = tokio::fs::File::create(&tmp).await?;
        let mut buf = vec![0u8; CHUNK];
        let mut last = Instant::now();
        loop {
            cancelled(cancel)?;
            let n = src.read(&mut buf).await.map_err(|e| AppError::Ssh(format!("Read failed: {e}")))?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n]).await?;
            ev.bytes += n as u64;
            if last.elapsed() >= PROGRESS_EVERY {
                sink.update(ev);
                last = Instant::now();
            }
        }
        dst.flush().await?;
        dst.sync_all().await?;
        drop(dst);
        cancelled(cancel)?;
        if !plan.overwrite && dest.exists() {
            return Err(AppError::Exists(format!("'{}' appeared locally during download; not overwritten", plan.name)));
        }
        // std::fs::rename replaces an existing file on Windows (only reached when overwrite was chosen).
        tokio::fs::rename(&tmp, &dest).await?;
        Ok(())
    };
    let res: AppResult<()> = copy.await;
    if res.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    res
}
