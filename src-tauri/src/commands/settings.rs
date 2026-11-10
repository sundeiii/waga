use std::path::Path;

use tauri::State;

use crate::credentials::delete_server_secrets;
use crate::database::settings::{self, AppSettings};
use crate::database::servers;
use crate::error::{AppError, AppResult};
use crate::ssh::session::SessionCommand;
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    state.db.with(settings::load)
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, settings: AppSettings) -> AppResult<AppSettings> {
    state.db.with(|c| settings::save(c, &settings))?;
    Ok(settings)
}

#[tauri::command]
pub fn reset_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    let s = AppSettings::default();
    state.db.with(|c| settings::save(c, &s))?;
    Ok(s)
}

/// Write non-sensitive preferences to a JSON file chosen in the native save dialog.
/// Contains no servers, hosts, credentials or history.
#[tauri::command]
pub fn export_settings(state: State<'_, AppState>, path: String) -> AppResult<()> {
    let p = Path::new(&path);
    if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("json")) != Some(true) {
        return Err(AppError::Validation("Choose a .json file".into()));
    }
    if !p.parent().is_some_and(|d| d.is_dir()) {
        return Err(AppError::Validation("Folder does not exist".into()));
    }
    let s = state.db.with(settings::load)?;
    let doc = serde_json::json!({
        "app": "yoru",
        "kind": "settings",
        "version": 1,
        "exportedAt": crate::database::now(),
        "settings": s,
    });
    std::fs::write(p, serde_json::to_vec_pretty(&doc)?)?;
    Ok(())
}

/// Privacy: end all sessions, remove every stored credential, and wipe all
/// local data (servers, trusted host keys, snippets, history, settings).
#[tauri::command]
pub async fn delete_all_data(state: State<'_, AppState>) -> AppResult<()> {
    for tx in state.sessions.all_senders() {
        let _ = tx.send(SessionCommand::Close).await;
    }
    let all = state.db.with(servers::list)?;
    for p in &all {
        delete_server_secrets(state.secrets.as_ref(), &p.id)?;
    }
    state.db.with(|c| {
        c.execute_batch(
            "BEGIN;
             DELETE FROM servers;
             DELETE FROM known_hosts;
             DELETE FROM activity;
             DELETE FROM snippets;
             DELETE FROM settings;
             COMMIT;",
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use crate::credentials::{password_account, MemoryStore, SecretStore};
    use crate::database::{known_hosts, servers, snippets, Database};

    /// Same SQL as delete_all_data, against a populated database.
    #[test]
    fn wipe_sql_clears_everything() {
        let db = Database::open_in_memory().unwrap();
        let store = MemoryStore::default();
        let s = db.with(|c| servers::insert(c, &servers::sample_input())).unwrap();
        store.set(&password_account(&s.id), "pw").unwrap();
        db.with(|c| known_hosts::add(c, "h", 22, "a", "a k", "f")).unwrap();
        crate::credentials::delete_server_secrets(&store, &s.id).unwrap();
        db.with(|c| {
            c.execute_batch("BEGIN; DELETE FROM servers; DELETE FROM known_hosts; DELETE FROM activity; DELETE FROM snippets; DELETE FROM settings; COMMIT;")?;
            assert!(servers::list(c)?.is_empty());
            assert!(known_hosts::list_all(c)?.is_empty());
            assert!(snippets::list(c)?.is_empty());
            Ok(())
        })
        .unwrap();
        assert_eq!(store.len(), 0);
    }
}
