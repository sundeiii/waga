//! Yoru — SSH & Server Manager (Rust backend).

pub mod commands;
pub mod credentials;
pub mod database;
pub mod error;
pub mod local_term;
pub mod monitoring;
pub mod security;
pub mod sftp;
pub mod ssh;
pub mod state;

use std::sync::Arc;

use tauri::Manager;

use crate::credentials::KeyringStore;
use crate::database::Database;
use crate::state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = Database::open(&dir.join("yoru.db"))?;
            // Apply the saved window backdrop before the window is shown.
            let material = db.with(database::settings::load).map(|s| s.appearance.window_material).unwrap_or_else(|_| "acrylic".into());
            if let Some(w) = app.get_webview_window("main") {
                if let Err(e) = commands::window::apply_material(&w, &material) {
                    log::warn!("{e}");
                }
            }
            app.manage(AppState::new(db, Arc::new(KeyringStore)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::reset_settings,
            commands::servers::list_servers,
            commands::servers::create_server,
            commands::servers::update_server,
            commands::servers::delete_server,
            commands::servers::duplicate_server,
            commands::servers::set_server_favorite,
            commands::servers::delete_server_credentials,
            commands::servers::delete_all_credentials,
            commands::servers::validate_host,
            commands::activity::list_activity,
            commands::activity::clear_activity,
            commands::activity::list_known_hosts,
            commands::activity::remove_known_host,
            commands::ssh::ssh_open,
            commands::ssh::ssh_write,
            commands::ssh::ssh_resize,
            commands::ssh::ssh_close,
            commands::ssh::ssh_host_key_decision,
            commands::ssh::ssh_provide_secret,
            commands::ssh::ssh_active_count,
            commands::window::set_window_material,
            commands::local::local_profiles,
            commands::local::local_open,
            commands::local::sessions_close_all,
            commands::sftp::sftp_home,
            commands::sftp::sftp_list,
            commands::sftp::sftp_mkdir,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_delete,
            commands::sftp::sftp_upload,
            commands::sftp::sftp_download,
            commands::sftp::sftp_cancel,
            commands::sftp::local_home,
            commands::sftp::local_list,
            commands::sftp::local_mkdir,
            commands::sftp::local_rename,
            commands::sftp::local_delete,
            commands::monitoring::monitor_start,
            commands::monitoring::monitor_stop,
            commands::monitoring::monitor_active,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Yoru");
}
