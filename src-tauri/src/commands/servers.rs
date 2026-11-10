use serde::{Deserialize, Serialize};
use tauri::State;
use zeroize::Zeroize;

use crate::credentials::{delete_server_secrets, passphrase_account, password_account, SecretStore};
use crate::database::activity::{self, ActivityKind};
use crate::database::servers::{self, AuthMethod, ServerInput, ServerProfile};
use crate::database::Database;
use crate::error::{AppError, AppResult};
use crate::security::validation::validate_server_input;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerView {
    #[serde(flatten)]
    pub profile: ServerProfile,
    pub has_password: bool,
    pub has_passphrase: bool,
}

/// A secret string that is wiped from memory on drop and never printed.
#[derive(Deserialize, Default)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SecretString {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString(<redacted>)")
    }
}

/// Secret changes submitted with a create/update.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SecretUpdate {
    pub password: Option<SecretString>,
    pub passphrase: Option<SecretString>,
    pub clear_password: bool,
    pub clear_passphrase: bool,
}

fn view(secrets: &dyn SecretStore, profile: ServerProfile) -> ServerView {
    let has = |acct: String| secrets.get(&acct).ok().flatten().is_some();
    ServerView {
        has_password: has(password_account(&profile.id)),
        has_passphrase: has(passphrase_account(&profile.id)),
        profile,
    }
}

fn apply_secrets(secrets: &dyn SecretStore, id: &str, method: AuthMethod, s: &SecretUpdate) -> AppResult<()> {
    // Secrets that don't apply to the chosen auth method are removed.
    if s.clear_password || method != AuthMethod::Password {
        secrets.delete(&password_account(id))?;
    } else if let Some(p) = s.password.as_ref().map(SecretString::as_str).filter(|p| !p.is_empty()) {
        secrets.set(&password_account(id), p)?;
    }
    if s.clear_passphrase || method != AuthMethod::Key {
        secrets.delete(&passphrase_account(id))?;
    } else if let Some(p) = s.passphrase.as_ref().map(SecretString::as_str).filter(|p| !p.is_empty()) {
        secrets.set(&passphrase_account(id), p)?;
    }
    Ok(())
}

pub fn create_impl(db: &Database, secrets: &dyn SecretStore, input: &ServerInput, s: &SecretUpdate) -> AppResult<ServerView> {
    let input = validate_server_input(input)?;
    let profile = db.with(|c| servers::insert(c, &input))?;
    if let Err(e) = apply_secrets(secrets, &profile.id, input.auth_method, s) {
        // Don't leave a half-created profile behind.
        let _ = db.with(|c| servers::delete(c, &profile.id));
        let _ = delete_server_secrets(secrets, &profile.id);
        return Err(e);
    }
    let _ = db.with(|c| activity::record(c, Some(&profile.id), Some(&profile.input.name), ActivityKind::ServerCreated, "Server profile created"));
    Ok(view(secrets, profile))
}

pub fn update_impl(db: &Database, secrets: &dyn SecretStore, id: &str, input: &ServerInput, s: &SecretUpdate) -> AppResult<ServerView> {
    let input = validate_server_input(input)?;
    let profile = db.with(|c| servers::update(c, id, &input))?;
    apply_secrets(secrets, id, input.auth_method, s)?;
    let _ = db.with(|c| activity::record(c, Some(id), Some(&profile.input.name), ActivityKind::ServerUpdated, "Server profile updated"));
    Ok(view(secrets, profile))
}

pub fn delete_impl(db: &Database, secrets: &dyn SecretStore, id: &str) -> AppResult<()> {
    let profile = db.with(|c| servers::get(c, id))?;
    // Remove secrets first: if this fails the profile still exists and the user can retry.
    delete_server_secrets(secrets, id)?;
    db.with(|c| servers::delete(c, id))?;
    let _ = db.with(|c| activity::record(c, None, Some(&profile.input.name), ActivityKind::ServerDeleted, "Server profile and stored credentials deleted"));
    Ok(())
}

pub fn duplicate_impl(db: &Database, secrets: &dyn SecretStore, id: &str) -> AppResult<ServerView> {
    let src = db.with(|c| servers::get(c, id))?;
    let mut input = src.input.clone();
    let base: String = input.name.chars().take(92).collect();
    input.name = format!("{base} (copy)");
    input.favorite = false;
    // Credentials are intentionally NOT copied; the user re-enters them.
    let profile = db.with(|c| servers::insert(c, &input))?;
    Ok(view(secrets, profile))
}

#[tauri::command]
pub fn list_servers(state: State<'_, AppState>) -> AppResult<Vec<ServerView>> {
    let profiles = state.db.with(servers::list)?;
    Ok(profiles.into_iter().map(|p| view(state.secrets.as_ref(), p)).collect())
}

#[tauri::command]
pub fn create_server(state: State<'_, AppState>, input: ServerInput, secrets: SecretUpdate) -> AppResult<ServerView> {
    create_impl(&state.db, state.secrets.as_ref(), &input, &secrets)
}

#[tauri::command]
pub fn update_server(state: State<'_, AppState>, id: String, input: ServerInput, secrets: SecretUpdate) -> AppResult<ServerView> {
    update_impl(&state.db, state.secrets.as_ref(), &id, &input, &secrets)
}

#[tauri::command]
pub fn delete_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
    delete_impl(&state.db, state.secrets.as_ref(), &id)
}

#[tauri::command]
pub fn duplicate_server(state: State<'_, AppState>, id: String) -> AppResult<ServerView> {
    duplicate_impl(&state.db, state.secrets.as_ref(), &id)
}

#[tauri::command]
pub fn set_server_favorite(state: State<'_, AppState>, id: String, favorite: bool) -> AppResult<()> {
    state.db.with(|c| servers::set_favorite(c, &id, favorite))
}

#[tauri::command]
pub fn delete_server_credentials(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let p = state.db.with(|c| servers::get(c, &id))?;
    delete_server_secrets(state.secrets.as_ref(), &id)?;
    let _ = state.db.with(|c| activity::record(c, Some(&id), Some(&p.input.name), ActivityKind::CredentialsDeleted, "Stored credentials deleted"));
    Ok(())
}

/// Privacy: remove every Yoru credential from the OS store.
#[tauri::command]
pub fn delete_all_credentials(state: State<'_, AppState>) -> AppResult<u32> {
    let all = state.db.with(servers::list)?;
    let mut n = 0;
    for p in &all {
        delete_server_secrets(state.secrets.as_ref(), &p.id)?;
        n += 1;
    }
    let _ = state.db.with(|c| activity::record(c, None, None, ActivityKind::CredentialsDeleted, &format!("Deleted stored credentials for {n} servers")));
    Ok(n)
}

#[tauri::command]
pub fn validate_host(host: String) -> AppResult<()> {
    crate::security::validation::validate_host(&host).map_err(AppError::Validation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryStore;
    use crate::database::servers::sample_input;

    fn secrets(pw: Option<&str>) -> SecretUpdate {
        SecretUpdate { password: pw.map(SecretString::from), ..Default::default() }
    }

    #[test]
    fn create_stores_password_in_secret_store_only() {
        let db = Database::open_in_memory().unwrap();
        let store = MemoryStore::default();
        let v = create_impl(&db, &store, &sample_input(), &secrets(Some("hunter2"))).unwrap();
        assert!(v.has_password);
        assert_eq!(store.get(&password_account(&v.profile.id)).unwrap().unwrap().as_str(), "hunter2");
        // Not anywhere in the DB.
        let dump: String = db
            .with(|c| Ok(c.query_row("SELECT group_concat(name||host||username||notes||tags) FROM servers", [], |r| r.get(0))?))
            .unwrap();
        assert!(!dump.contains("hunter2"));
    }

    #[test]
    fn delete_removes_credentials() {
        let db = Database::open_in_memory().unwrap();
        let store = MemoryStore::default();
        let v = create_impl(&db, &store, &sample_input(), &secrets(Some("pw"))).unwrap();
        assert_eq!(store.len(), 1);
        delete_impl(&db, &store, &v.profile.id).unwrap();
        assert_eq!(store.len(), 0);
        assert!(db.with(|c| servers::get(c, &v.profile.id)).is_err());
    }

    #[test]
    fn switching_auth_method_drops_stale_secret() {
        let db = Database::open_in_memory().unwrap();
        let store = MemoryStore::default();
        let v = create_impl(&db, &store, &sample_input(), &secrets(Some("pw"))).unwrap();
        let mut i = sample_input();
        i.auth_method = AuthMethod::Agent;
        let v2 = update_impl(&db, &store, &v.profile.id, &i, &SecretUpdate::default()).unwrap();
        assert!(!v2.has_password);
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn update_keeps_password_when_not_provided() {
        let db = Database::open_in_memory().unwrap();
        let store = MemoryStore::default();
        let v = create_impl(&db, &store, &sample_input(), &secrets(Some("pw"))).unwrap();
        let v2 = update_impl(&db, &store, &v.profile.id, &sample_input(), &SecretUpdate::default()).unwrap();
        assert!(v2.has_password);
        let cleared = SecretUpdate { clear_password: true, ..Default::default() };
        let v3 = update_impl(&db, &store, &v.profile.id, &sample_input(), &cleared).unwrap();
        assert!(!v3.has_password);
    }

    #[test]
    fn duplicate_does_not_copy_credentials() {
        let db = Database::open_in_memory().unwrap();
        let store = MemoryStore::default();
        let v = create_impl(&db, &store, &sample_input(), &secrets(Some("pw"))).unwrap();
        let d = duplicate_impl(&db, &store, &v.profile.id).unwrap();
        assert_ne!(d.profile.id, v.profile.id);
        assert_eq!(d.profile.input.name, "web-01 (copy)");
        assert!(!d.has_password);
    }

    #[test]
    fn invalid_input_creates_nothing() {
        let db = Database::open_in_memory().unwrap();
        let store = MemoryStore::default();
        let mut i = sample_input();
        i.host = "-oProxyCommand=calc".into();
        assert!(matches!(create_impl(&db, &store, &i, &secrets(Some("pw"))), Err(AppError::Validation(_))));
        assert_eq!(store.len(), 0);
        assert!(db.with(servers::list).unwrap().is_empty());
    }
}
