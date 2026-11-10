//! File and folder transfers with progress and cancellation.
//!
//! Safety:
//! * every file is written to `<target>.yoru-partial` and renamed into place
//!   only after a complete copy — a cancelled/failed transfer never leaves a
//!   truncated file under the real name;
//! * an existing file or folder is only replaced/merged when the user
//!   explicitly chose to overwrite;
//! * symlinks are never followed (no loops, no escaping the chosen folder);
//! * folder walks are bounded (files and depth).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileType, OpenFlags};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::paths::{local_join, remote_join, remote_name, sanitize_for_windows, validate_name, PARTIAL_SUFFIX};
use super::remote::{is_not_found, sftp_err};
use crate::error::{AppError, AppResult};

const CHUNK: usize = 256 * 1024;
const PROGRESS_EVERY: Duration = Duration::from_millis(120);
pub const MAX_FILES: usize = 50_000;
pub const MAX_DEPTH: usize = 64;

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
    pub is_dir: bool,
    pub files_done: u32,
    pub files_total: u32,
    /// Symlinks / special files that were skipped in a folder transfer.
    pub skipped: u32,
}

pub trait TransferSink: Send + Sync + 'static {
    fn update(&self, e: &TransferEvent);
}

#[derive(Default)]
pub struct TransferManager {
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Debug, Clone)]
pub struct FileJob {
    pub src: String,
    pub dest: String,
    pub size: u64,
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
    pub is_dir: bool,
    /// Destination folders to create, parents first (folder transfers).
    pub dirs: Vec<String>,
    pub files: Vec<FileJob>,
    pub skipped: u32,
}

fn too_many() -> AppError {
    AppError::Validation(format!("Folder has more than {MAX_FILES} files; transfer smaller parts"))
}

/// Walk a local folder without following symlinks.
fn walk_local(root: &Path, dest_root: &str) -> AppResult<(Vec<String>, Vec<FileJob>, u32)> {
    let mut dirs = vec![dest_root.to_string()];
    let mut files = Vec::new();
    let mut skipped = 0u32;
    let mut stack = vec![(root.to_path_buf(), dest_root.to_string(), 0usize)];
    while let Some((dir, dest, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(AppError::Validation("Folder is nested too deeply".into()));
        }
        let rd = std::fs::read_dir(&dir).map_err(|e| AppError::Validation(format!("Cannot read {}: {e}", dir.display())))?;
        for item in rd.flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            let Ok(ft) = item.file_type() else { continue };
            if ft.is_symlink() || validate_name(&name).is_err() {
                skipped += 1;
                continue;
            }
            let child_dest = remote_join(&dest, &name).map_err(AppError::Validation)?;
            if ft.is_dir() {
                dirs.push(child_dest.clone());
                stack.push((item.path(), child_dest, depth + 1));
            } else if ft.is_file() {
                let size = item.metadata().map(|m| m.len()).unwrap_or(0);
                files.push(FileJob { src: item.path().to_string_lossy().into_owned(), dest: child_dest, size });
                if files.len() > MAX_FILES {
                    return Err(too_many());
                }
            } else {
                skipped += 1;
            }
        }
    }
    Ok((dirs, files, skipped))
}

/// Walk a remote folder without following symlinks; names are made Windows-safe.
async fn walk_remote(sftp: &SftpSession, root: &str, dest_root: &Path) -> AppResult<(Vec<String>, Vec<FileJob>, u32)> {
    let mut dirs = vec![dest_root.to_string_lossy().into_owned()];
    let mut files = Vec::new();
    let mut skipped = 0u32;
    let mut stack = vec![(root.to_string(), dest_root.to_path_buf(), 0usize)];
    while let Some((dir, dest, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(AppError::Validation("Folder is nested too deeply".into()));
        }
        for entry in sftp.read_dir(dir.clone()).await.map_err(sftp_err)? {
            let name = entry.file_name();
            let meta = entry.metadata();
            let Ok(remote_path) = remote_join(&dir, &name) else {
                skipped += 1;
                continue;
            };
            let Ok(safe) = sanitize_for_windows(&name) else {
                skipped += 1;
                continue;
            };
            let Ok(local) = local_join(&dest, &safe) else {
                skipped += 1;
                continue;
            };
            match meta.file_type() {
                FileType::Dir => {
                    dirs.push(local.to_string_lossy().into_owned());
                    stack.push((remote_path, local, depth + 1));
                }
                FileType::File => {
                    files.push(FileJob { src: remote_path, dest: local.to_string_lossy().into_owned(), size: meta.size.unwrap_or(0) });
                    if files.len() > MAX_FILES {
                        return Err(too_many());
                    }
                }
                _ => skipped += 1, // symlinks and specials are not followed
            }
        }
    }
    Ok((dirs, files, skipped))
}

pub async fn plan_upload(sftp: &SftpSession, server_id: &str, local_path: &Path, remote_dir: &str, overwrite: bool) -> AppResult<Plan> {
    let meta = tokio::fs::symlink_metadata(local_path).await.map_err(|e| AppError::Validation(format!("Cannot read {}: {e}", local_path.display())))?;
    if meta.file_type().is_symlink() {
        return Err(AppError::Validation("Symbolic links aren't transferred".into()));
    }
    let name = local_path.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or_else(|| AppError::Validation("Invalid file".into()))?;
    validate_name(&name).map_err(AppError::Validation)?;
    let dest = remote_join(remote_dir, &name).map_err(AppError::Validation)?;
    if !overwrite && sftp.try_exists(dest.clone()).await.map_err(sftp_err)? {
        let what = if meta.is_dir() { "folder" } else { "file" };
        return Err(AppError::Exists(format!("A {what} named '{name}' already exists on the server")));
    }
    let source = local_path.to_string_lossy().into_owned();
    if meta.is_dir() {
        let root = local_path.to_path_buf();
        let d = dest.clone();
        let (dirs, files, skipped) = tokio::task::spawn_blocking(move || walk_local(&root, &d))
            .await
            .map_err(|e| AppError::Internal(e.to_string()))??;
        let total = files.iter().map(|f| f.size).sum();
        return Ok(Plan { direction: Direction::Upload, server_id: server_id.into(), name, source, dest, total: Some(total), overwrite, is_dir: true, dirs, files, skipped });
    }
    Ok(Plan {
        direction: Direction::Upload,
        server_id: server_id.into(),
        files: vec![FileJob { src: source.clone(), dest: dest.clone(), size: meta.len() }],
        name,
        source,
        dest,
        total: Some(meta.len()),
        overwrite,
        is_dir: false,
        dirs: vec![],
        skipped: 0,
    })
}

pub async fn plan_download(sftp: &SftpSession, server_id: &str, remote_path: &str, local_dir: &Path, overwrite: bool) -> AppResult<Plan> {
    let meta = sftp.symlink_metadata(remote_path.to_string()).await.map_err(sftp_err)?;
    if meta.is_symlink() {
        // Resolve one level for a symlink the user explicitly picked.
        let target = sftp.metadata(remote_path.to_string()).await.map_err(sftp_err)?;
        if target.is_dir() {
            return Err(AppError::Validation("Symlinked folders aren't transferred; open the link and pick its contents".into()));
        }
    }
    if !local_dir.is_dir() {
        return Err(AppError::Validation(format!("{} is not a folder", local_dir.display())));
    }
    let remote = remote_name(remote_path).to_string();
    let safe = sanitize_for_windows(&remote).map_err(AppError::Validation)?;
    let dest = local_join(local_dir, &safe).map_err(AppError::Validation)?;
    if !overwrite && dest.exists() {
        let what = if meta.is_dir() { "folder" } else { "file" };
        return Err(AppError::Exists(format!("A {what} named '{safe}' already exists in this folder")));
    }
    let dest_s = dest.to_string_lossy().into_owned();
    if meta.is_dir() {
        let (dirs, files, skipped) = walk_remote(sftp, remote_path, &dest).await?;
        let total = files.iter().map(|f| f.size).sum();
        return Ok(Plan { direction: Direction::Download, server_id: server_id.into(), name: safe, source: remote_path.into(), dest: dest_s, total: Some(total), overwrite, is_dir: true, dirs, files, skipped });
    }
    let size = sftp.metadata(remote_path.to_string()).await.map_err(sftp_err)?.size;
    Ok(Plan {
        direction: Direction::Download,
        server_id: server_id.into(),
        files: vec![FileJob { src: remote_path.into(), dest: dest_s.clone(), size: size.unwrap_or(0) }],
        name: safe,
        source: remote_path.into(),
        dest: dest_s,
        total: size,
        overwrite,
        is_dir: false,
        dirs: vec![],
        skipped: 0,
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
                is_dir: plan.is_dir,
                files_done: 0,
                files_total: plan.files.len() as u32,
                skipped: plan.skipped,
            };
            sink.update(&ev);
            let mut progress = Progress { ev: &mut ev, sink: sink.as_ref(), last: Instant::now() };
            let result = run_plan(&sftp, &plan, &cancel, &mut progress).await;
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

struct Progress<'a> {
    ev: &'a mut TransferEvent,
    sink: &'a dyn TransferSink,
    last: Instant,
}

impl Progress<'_> {
    fn add(&mut self, n: u64) {
        self.ev.bytes += n;
        if self.last.elapsed() >= PROGRESS_EVERY {
            self.sink.update(self.ev);
            self.last = Instant::now();
        }
    }
    fn file_done(&mut self) {
        self.ev.files_done += 1;
    }
}

fn cancelled(flag: &AtomicBool) -> AppResult<()> {
    if flag.load(Ordering::SeqCst) {
        Err(AppError::Validation("Cancelled".into()))
    } else {
        Ok(())
    }
}

async fn run_plan(sftp: &SftpSession, plan: &Plan, cancel: &AtomicBool, p: &mut Progress<'_>) -> AppResult<()> {
    // Folders first (parents before children).
    for d in &plan.dirs {
        cancelled(cancel)?;
        match plan.direction {
            Direction::Upload => {
                if !sftp.try_exists(d.clone()).await.unwrap_or(false) {
                    sftp.create_dir(d.clone()).await.map_err(sftp_err)?;
                }
            }
            Direction::Download => tokio::fs::create_dir_all(d).await?,
        }
    }
    // In a folder transfer the user already agreed to merge, so files inside
    // may be replaced; for a single file `overwrite` is the explicit choice.
    let overwrite = plan.overwrite || plan.is_dir;
    for f in &plan.files {
        cancelled(cancel)?;
        match plan.direction {
            Direction::Upload => upload_file(sftp, f, overwrite, cancel, p).await?,
            Direction::Download => download_file(sftp, f, overwrite, cancel, p).await?,
        }
        p.file_done();
    }
    Ok(())
}

async fn upload_file(sftp: &SftpSession, f: &FileJob, overwrite: bool, cancel: &AtomicBool, p: &mut Progress<'_>) -> AppResult<()> {
    let tmp = format!("{}{PARTIAL_SUFFIX}", f.dest);
    let copy = async {
        let mut src = tokio::fs::File::open(&f.src).await?;
        let mut dst = sftp.open_with_flags(tmp.clone(), OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE).await.map_err(sftp_err)?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            cancelled(cancel)?;
            let n = src.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n]).await.map_err(|e| AppError::Ssh(format!("Write failed: {e}")))?;
            p.add(n as u64);
        }
        dst.flush().await.map_err(|e| AppError::Ssh(format!("Write failed: {e}")))?;
        dst.shutdown().await.map_err(|e| AppError::Ssh(format!("Close failed: {e}")))?;
        cancelled(cancel)?;
        if overwrite {
            if let Err(e) = sftp.remove_file(f.dest.clone()).await {
                if !is_not_found(&e) {
                    return Err(sftp_err(e));
                }
            }
        } else if sftp.try_exists(f.dest.clone()).await.unwrap_or(false) {
            return Err(AppError::Exists(format!("'{}' appeared on the server during upload; not overwritten", remote_name(&f.dest))));
        }
        sftp.rename(tmp.clone(), f.dest.clone()).await.map_err(sftp_err)
    };
    let res: AppResult<()> = copy.await;
    if res.is_err() {
        let _ = sftp.remove_file(tmp).await;
    }
    res
}

async fn download_file(sftp: &SftpSession, f: &FileJob, overwrite: bool, cancel: &AtomicBool, p: &mut Progress<'_>) -> AppResult<()> {
    let dest = PathBuf::from(&f.dest);
    let tmp = PathBuf::from(format!("{}{PARTIAL_SUFFIX}", f.dest));
    let copy = async {
        let mut src = sftp.open_with_flags(f.src.clone(), OpenFlags::READ).await.map_err(sftp_err)?;
        let mut dst = tokio::fs::File::create(&tmp).await?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            cancelled(cancel)?;
            let n = src.read(&mut buf).await.map_err(|e| AppError::Ssh(format!("Read failed: {e}")))?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n]).await?;
            p.add(n as u64);
        }
        dst.flush().await?;
        dst.sync_all().await?;
        drop(dst);
        cancelled(cancel)?;
        if !overwrite && dest.exists() {
            return Err(AppError::Exists(format!("'{}' appeared locally during download; not overwritten", dest.display())));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_walk_skips_nothing_unexpected_and_maps_paths() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("a/b")).unwrap();
        std::fs::write(d.path().join("top.txt"), b"12345").unwrap();
        std::fs::write(d.path().join("a/b/deep.bin"), vec![0u8; 100]).unwrap();
        let (dirs, files, skipped) = walk_local(d.path(), "/srv/up").unwrap();
        assert!(dirs.contains(&"/srv/up".to_string()));
        assert!(dirs.contains(&"/srv/up/a".to_string()));
        assert!(dirs.contains(&"/srv/up/a/b".to_string()));
        let dests: Vec<_> = files.iter().map(|f| f.dest.as_str()).collect();
        assert!(dests.contains(&"/srv/up/top.txt"));
        assert!(dests.contains(&"/srv/up/a/b/deep.bin"));
        assert_eq!(files.iter().map(|f| f.size).sum::<u64>(), 105);
        assert_eq!(skipped, 0);
        // Parents always listed before their children.
        let pa = dirs.iter().position(|x| x == "/srv/up/a").unwrap();
        let pb = dirs.iter().position(|x| x == "/srv/up/a/b").unwrap();
        assert!(pa < pb);
    }
}
