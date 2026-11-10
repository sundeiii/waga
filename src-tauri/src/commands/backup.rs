//! Encrypted backup/restore and folder sync commands.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use zeroize::Zeroizing;

use crate::credentials::SecretStore;
use crate::database::activity::{self, ActivityKind};
use crate::database::{settings, Database};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::sync::bundle::{self, Bundle, MergeStats};
use crate::sync::crypto;
use crate::sync::folder::{self, SyncReport};

pub const SYNC_PASSPHRASE_ACCOUNT: &str = "sync/passphrase";
pub const BACKUP_EXT: &str = "yorubackup";

static SYNCING: AtomicBool = AtomicBool::new(false);

fn log(db: &Database, msg: &str) {
    let _ = db.with(|c| activity::record(c, None, None, ActivityKind::Sync, msg));
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| AppError::Internal(e.to_string()))?
}

/// Encrypted backup of everything syncable (optionally with saved credentials).
#[tauri::command]
pub async fn export_backup(state: State<'_, AppState>, path: String, passphrase: String, include_secrets: bool) -> AppResult<()> {
    let pass = Zeroizing::new(passphrase);
    crypto::check_passphrase(&pass).map_err(AppError::Validation)?;
    let p = PathBuf::from(&path);
    if p.extension().and_then(|e| e.to_str()) != Some(BACKUP_EXT) {
        return Err(AppError::Validation(format!("Backups use the .{BACKUP_EXT} extension")));
    }
    let (db, store) = (state.db.clone(), state.secrets.clone());
    blocking(move || {
        let device = db.with(settings::load)?.sync.device_name;
        let b = db.with(|c| bundle::export(c, store.as_ref(), include_secrets, &device))?;
        let json = Zeroizing::new(serde_json::to_vec(&b)?);
        let sealed = crypto::seal(&pass, &json).map_err(AppError::Internal)?;
        let tmp = p.with_extension(format!("{BACKUP_EXT}.tmp"));
        std::fs::write(&tmp, sealed)?;
        std::fs::rename(&tmp, &p)?;
        log(&db, &format!("Encrypted backup exported{}", if include_secrets { " (with credentials)" } else { "" }));
        Ok(())
    })
    .await
}

/// Restore merges into existing data (nothing local is lost; newer wins).
#[tauri::command]
pub async fn import_backup(state: State<'_, AppState>, path: String, passphrase: String) -> AppResult<MergeStats> {
    let pass = Zeroizing::new(passphrase);
    let (db, store) = (state.db.clone(), state.secrets.clone());
    blocking(move || {
        let p = Path::new(&path);
        if std::fs::metadata(p)?.len() > 100 * 1024 * 1024 {
            return Err(AppError::Validation("File too large to be a yoru backup".into()));
        }
        let data = std::fs::read(p)?;
        let plain = crypto::open(&pass, &data).map_err(AppError::Validation)?;
        let b: Bundle = serde_json::from_slice(&plain).map_err(|_| AppError::Validation("Backup contents are unreadable".into()))?;
        let stats = db.with(|c| bundle::apply(c, store.as_ref(), &b))?;
        log(&db, &format!("Backup restored: {} added, {} updated, {} removed", stats.added, stats.updated, stats.deleted));
        Ok(stats)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub has_passphrase: bool,
    pub device_id: String,
}

#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> AppResult<SyncStatus> {
    Ok(SyncStatus {
        has_passphrase: state.secrets.get(SYNC_PASSPHRASE_ACCOUNT)?.is_some(),
        device_id: state.db.with(bundle::device_id)?,
    })
}

/// Save (or with None, forget) this PC's sync passphrase in Credential Manager.
#[tauri::command]
pub fn set_sync_passphrase(state: State<'_, AppState>, passphrase: Option<String>) -> AppResult<()> {
    match passphrase.map(Zeroizing::new) {
        Some(p) => {
            crypto::check_passphrase(&p).map_err(AppError::Validation)?;
            state.secrets.set(SYNC_PASSPHRASE_ACCOUNT, &p)
        }
        None => state.secrets.delete(SYNC_PASSPHRASE_ACCOUNT),
    }
}

pub fn run_sync(db: Arc<Database>, store: Arc<dyn SecretStore>) -> AppResult<SyncReport> {
    if SYNCING.swap(true, Ordering::SeqCst) {
        return Err(AppError::Validation("A sync is already running".into()));
    }
    let result = (|| {
        let cfg = db.with(settings::load)?.sync;
        if cfg.folder.trim().is_empty() {
            return Err(AppError::Validation("Choose a sync folder first".into()));
        }
        let pass = store.get(SYNC_PASSPHRASE_ACCOUNT)?.ok_or_else(|| AppError::Validation("Set a sync passphrase first".into()))?;
        let report = folder::sync_folder(&db, store.as_ref(), Path::new(&cfg.folder), &pass, cfg.include_secrets, &cfg.device_name)?;
        if !report.errors.is_empty() {
            log(&db, &format!("Sync finished with problems: {}", report.errors.join("; ")));
        }
        Ok(report)
    })();
    SYNCING.store(false, Ordering::SeqCst);
    if let Err(e) = &result {
        log(&db, &format!("Sync failed: {e}"));
    }
    result
}

#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> AppResult<SyncReport> {
    let (db, store) = (state.db.clone(), state.secrets.clone());
    blocking(move || run_sync(db, store)).await
}
