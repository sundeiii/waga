//! In-app updates from GitHub Releases (tauri-plugin-updater).
//!
//! Every installer is verified against the public key compiled into the app
//! (tauri.conf.json → plugins.updater.pubkey) before it runs; an update not
//! signed with the matching private key is rejected. Driven from Rust, so the
//! webview gets no updater permissions.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::error::{AppError, AppResult};

pub const PROGRESS_EVENT: &str = "update://progress";

/// The update found by the last check, kept until the user installs it.
#[derive(Default)]
pub struct PendingUpdate(pub Mutex<Option<Update>>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub version: String,
    /// Release notes from latest.json (untrusted text; control chars removed).
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

fn clean_notes(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\n').take(5000).collect::<String>().trim().to_string()
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Ask the release endpoint whether a newer signed version exists.
#[tauri::command]
pub async fn check_for_update(app: AppHandle, pending: State<'_, PendingUpdate>) -> AppResult<Option<UpdateInfo>> {
    let updater = app.updater().map_err(|e| AppError::Internal(format!("Updater unavailable: {e}")))?;
    let found = updater.check().await.map_err(|e| AppError::Internal(format!("Could not check for updates: {e}")))?;
    let info = found.as_ref().map(|u| UpdateInfo {
        current_version: u.current_version.clone(),
        version: u.version.clone(),
        notes: u.body.as_deref().map(clean_notes).filter(|n| !n.is_empty()),
        date: u.date.map(|d| d.to_string()),
    });
    *pending.0.lock().unwrap() = found;
    Ok(info)
}

/// Download, verify the signature, run the installer, then restart.
#[tauri::command]
pub async fn install_update(app: AppHandle, pending: State<'_, PendingUpdate>) -> AppResult<()> {
    let update = pending.0.lock().unwrap().take().ok_or_else(|| AppError::Validation("No update to install; check again".into()))?;
    let mut downloaded: u64 = 0;
    let emitter = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = emitter.emit(PROGRESS_EVENT, Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| AppError::Internal(format!("Update failed (nothing was changed): {e}")))?;
    // On Windows the installer usually replaces and relaunches the app itself;
    // restart in case it didn't.
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_sanitized() {
        assert_eq!(clean_notes("  Fixes\n- a\u{1b}[31m red\u{7}  "), "Fixes\n- a[31m red");
        assert_eq!(clean_notes(&"x".repeat(6000)).len(), 5000);
    }
}
