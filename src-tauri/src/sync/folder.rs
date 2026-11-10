//! Sync through a folder the user already syncs (OneDrive, Dropbox, a NAS…).
//!
//! Layout: <folder>/yoru-sync/<device-id>.yorusync — one encrypted file per
//! PC. Each PC only ever writes its own file (no write conflicts, no partial
//! overwrites by the sync client) and merges everyone else's. The merge is
//! order-independent, so all PCs converge.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::bundle::{self, Bundle, MergeStats};
use super::crypto;
use crate::credentials::SecretStore;
use crate::database::Database;
use crate::error::{AppError, AppResult};

pub const DIR: &str = "yoru-sync";
pub const EXT: &str = "yorusync";
const MAX_FILE: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub written_at: String,
    pub is_self: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub stats: MergeStats,
    pub devices: Vec<DeviceInfo>,
    pub errors: Vec<String>,
    pub at: String,
}

pub fn sync_dir(folder: &Path) -> PathBuf {
    folder.join(DIR)
}

pub fn sync_folder(db: &Database, store: &dyn SecretStore, folder: &Path, pass: &str, include_secrets: bool, device_name: &str) -> AppResult<SyncReport> {
    sync_folder_with(db, store, folder, pass, include_secrets, device_name, crypto::DEFAULT_COST)
}

/// Same, with an explicit key-derivation cost (tests use a cheap one).
pub fn sync_folder_with(
    db: &Database,
    store: &dyn SecretStore,
    folder: &Path,
    pass: &str,
    include_secrets: bool,
    device_name: &str,
    cost: crypto::KdfCost,
) -> AppResult<SyncReport> {
    if !folder.is_dir() {
        return Err(AppError::Validation(format!("Sync folder not found: {}", folder.display())));
    }
    let dir = sync_dir(folder);
    std::fs::create_dir_all(&dir)?;
    let my_id = db.with(bundle::device_id)?;
    let mut stats = MergeStats::default();
    let mut devices = Vec::new();
    let mut errors = Vec::new();

    // 1. Pull: merge every other device's file.
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(EXT) {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if stem == my_id || uuid::Uuid::parse_str(&stem).is_err() {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let result = (|| -> Result<Bundle, String> {
            if entry.metadata().map(|m| m.len()).unwrap_or(0) > MAX_FILE {
                return Err("file too large".into());
            }
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            let plain = crypto::open(pass, &data)?;
            let b: Bundle = serde_json::from_slice(&plain).map_err(|e| format!("unreadable contents: {e}"))?;
            if b.device_id != stem {
                return Err("file name does not match its device id".into());
            }
            Ok(b)
        })();
        match result {
            Ok(b) => match db.with(|c| bundle::apply(c, store, &b)) {
                Ok(s) => {
                    stats.add(&s);
                    devices.push(DeviceInfo { device_id: b.device_id.clone(), device_name: b.device_name.clone(), written_at: b.created_at.clone(), is_self: false });
                }
                Err(e) => errors.push(format!("{name}: {e}")),
            },
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }

    // 2. Push: write our merged state atomically (temp file + rename).
    let mine = db.with(|c| bundle::export(c, store, include_secrets, device_name))?;
    let json = zeroize::Zeroizing::new(serde_json::to_vec(&mine)?);
    let sealed = crypto::seal_with(pass, &json, cost).map_err(AppError::Internal)?;
    let target = dir.join(format!("{my_id}.{EXT}"));
    let tmp = dir.join(format!("{my_id}.{EXT}.tmp"));
    std::fs::write(&tmp, &sealed)?;
    std::fs::rename(&tmp, &target)?;
    devices.insert(0, DeviceInfo { device_id: my_id, device_name: mine.device_name.clone(), written_at: mine.created_at.clone(), is_self: true });

    Ok(SyncReport { stats, devices, errors, at: crate::database::now() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryStore;
    use crate::database::servers;

    #[test]
    fn three_pcs_through_one_folder() {
        let folder = tempfile::tempdir().unwrap();
        let pcs: Vec<(Database, MemoryStore)> = (0..3).map(|_| (Database::open_in_memory().unwrap(), MemoryStore::default())).collect();
        let pass = "shared sync passphrase";
        // Each PC adds one server.
        for (i, (db, _)) in pcs.iter().enumerate() {
            let mut inp = servers::sample_input();
            inp.name = format!("server-from-pc{i}");
            db.with(|c| servers::insert(c, &inp)).unwrap();
        }
        // Two rounds: everyone ends up with all three.
        for _ in 0..2 {
            for (i, (db, st)) in pcs.iter().enumerate() {
                let r = sync_folder_with(db, st, folder.path(), pass, false, &format!("pc{i}"), crypto::TEST_COST).unwrap();
                assert!(r.errors.is_empty(), "{:?}", r.errors);
            }
        }
        for (db, _) in &pcs {
            let mut names: Vec<_> = db.with(servers::list).unwrap().into_iter().map(|s| s.input.name).collect();
            names.sort();
            assert_eq!(names, vec!["server-from-pc0", "server-from-pc1", "server-from-pc2"]);
        }
        // Exactly one file per PC; nothing readable in plaintext.
        let files: Vec<_> = std::fs::read_dir(sync_dir(folder.path())).unwrap().flatten().collect();
        assert_eq!(files.len(), 3);
        for f in files {
            let bytes = std::fs::read(f.path()).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("server-from"), "encrypted at rest");
        }
    }

    #[test]
    fn wrong_passphrase_is_reported_not_applied() {
        let folder = tempfile::tempdir().unwrap();
        let a = (Database::open_in_memory().unwrap(), MemoryStore::default());
        let b = (Database::open_in_memory().unwrap(), MemoryStore::default());
        a.0.with(|c| servers::insert(c, &servers::sample_input())).unwrap();
        sync_folder_with(&a.0, &a.1, folder.path(), "passphrase-one", false, "a", crypto::TEST_COST).unwrap();
        let r = sync_folder_with(&b.0, &b.1, folder.path(), "passphrase-two", false, "b", crypto::TEST_COST).unwrap();
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].contains("Wrong passphrase"));
        assert!(b.0.with(servers::list).unwrap().is_empty());
    }
}
