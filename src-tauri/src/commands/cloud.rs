use serde::Deserialize;
use tauri::State;
use zeroize::Zeroizing;

use crate::cloud::{self, CloudVm, Provider};
use crate::commands::servers::{create_impl, SecretUpdate};
use crate::database::activity::{self, ActivityKind};
use crate::database::servers::{self, AuthMethod, ProfileKind, ServerInput};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// List VMs. `token` overrides a saved one; `remember` saves it in Credential Manager.
#[tauri::command]
pub async fn cloud_list(state: State<'_, AppState>, provider: Provider, token: Option<String>, remember: bool, region: Option<String>) -> AppResult<Vec<CloudVm>> {
    let token = match token.filter(|t| !t.trim().is_empty()).map(Zeroizing::new) {
        Some(t) => Some(t),
        None if provider.uses_token() => state.secrets.get(&provider.token_account())?,
        None => None,
    };
    let vms = cloud::list(provider, token.as_deref().map(|t| t.as_str()), region.as_deref()).await?;
    if remember && provider.uses_token() {
        if let Some(t) = &token {
            state.secrets.set(&provider.token_account(), t)?;
        }
    }
    Ok(vms)
}

#[tauri::command]
pub fn cloud_has_token(state: State<'_, AppState>, provider: Provider) -> AppResult<bool> {
    Ok(provider.uses_token() && state.secrets.get(&provider.token_account())?.is_some())
}

#[tauri::command]
pub fn cloud_forget_token(state: State<'_, AppState>, provider: Provider) -> AppResult<()> {
    state.secrets.delete(&provider.token_account())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub name: String,
    pub host: String,
    pub username: String,
    pub windows: bool,
    pub os: Option<String>,
    pub region: String,
    pub tags: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub created: u32,
    pub skipped: Vec<String>,
}

/// Create server profiles for the selected VMs (duplicates by host+user are skipped).
#[tauri::command]
pub fn cloud_import(state: State<'_, AppState>, provider: Provider, items: Vec<ImportItem>, auth_method: AuthMethod, key_path: Option<String>) -> AppResult<ImportResult> {
    if items.len() > 500 {
        return Err(AppError::Validation("Import at most 500 servers at once".into()));
    }
    let existing = state.db.with(servers::list)?;
    let group = match provider {
        Provider::DigitalOcean => "DigitalOcean",
        Provider::Hetzner => "Hetzner",
        Provider::Linode => "Linode",
        Provider::Aws => "AWS",
        Provider::Azure => "Azure",
    };
    let mut res = ImportResult { created: 0, skipped: vec![] };
    for it in items {
        if existing.iter().any(|s| s.input.host.eq_ignore_ascii_case(&it.host) && s.input.username == it.username) {
            res.skipped.push(format!("{} (already saved)", it.name));
            continue;
        }
        let os = it.os.clone().unwrap_or_default().to_lowercase();
        let kind = if it.windows {
            ProfileKind::Windows
        } else if os.contains("ubuntu") || os.contains("debian") {
            ProfileKind::Debian
        } else {
            ProfileKind::Linux
        };
        let mut tags: Vec<String> = vec![provider.key().into()];
        if !it.region.is_empty() {
            tags.push(it.region.to_lowercase());
        }
        tags.extend(it.tags.into_iter().take(8));
        let input = ServerInput {
            name: it.name.clone(),
            host: it.host.clone(),
            port: 22,
            username: it.username.clone(),
            auth_method,
            key_path: key_path.clone(),
            profile_kind: kind,
            group_name: Some(group.into()),
            tags,
            notes: format!("Imported from {group}"),
            favorite: false,
            auto_reconnect: false,
            keepalive: true,
            jump_host_id: None,
        };
        match create_impl(&state.db, state.secrets.as_ref(), &input, &SecretUpdate::default()) {
            Ok(_) => res.created += 1,
            Err(e) => res.skipped.push(format!("{}: {e}", it.name)),
        }
    }
    let _ = state.db.with(|c| activity::record(c, None, None, ActivityKind::ServerCreated, &format!("Imported {} servers from {group}", res.created)));
    Ok(res)
}
