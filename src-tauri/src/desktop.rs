//! Windows desktop integration: tray icon, close-to-tray, start with Windows,
//! single instance and notifications. All of it runs in Rust; the webview gets
//! no extra plugin permissions.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, State, Window, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_notification::NotificationExt as _;

use crate::database::settings::{self, AppSettings};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const MINIMIZED_ARG: &str = "--minimized";
pub const NEW_LOCAL_EVENT: &str = "tray://new-local";

static TRAY_HINT_SHOWN: AtomicBool = AtomicBool::new(false);

fn load_settings<R: Runtime>(app: &AppHandle<R>) -> AppSettings {
    app.try_state::<AppState>().and_then(|s| s.db.with(settings::load).ok()).unwrap_or_default()
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn toggle_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_main(app);
        }
    }
}

pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "Open yoru").build(app)?;
    let local = MenuItemBuilder::with_id("new_local", "New local terminal").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit yoru").build(app)?;
    let menu = MenuBuilder::new(app).item(&open).item(&local).item(&PredefinedMenuItem::separator(app)?).item(&quit).build()?;

    let mut tray = TrayIconBuilder::with_id("main").tooltip("yoru — SSH & Server Manager").menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, ev| match ev.id().as_ref() {
        "open" => show_main(app),
        "new_local" => {
            show_main(app);
            let _ = app.emit(NEW_LOCAL_EVENT, ());
        }
        "quit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(|tray, ev| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
            toggle_main(tray.app_handle());
        }
    })
    .build(app)?;
    Ok(())
}

/// Close button: hide to tray when enabled (sessions keep running).
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        let s = load_settings(window.app_handle());
        if s.system.close_to_tray {
            api.prevent_close();
            let _ = window.hide();
            if s.system.notifications && !TRAY_HINT_SHOWN.swap(true, Ordering::SeqCst) {
                let _ = window
                    .app_handle()
                    .notification()
                    .builder()
                    .title("yoru is still running")
                    .body("Your sessions stay connected. Open yoru from the tray icon, or use Quit there.")
                    .show();
            }
        }
    }
}

/// Was this process started by "start with Windows" and should stay in the tray?
pub fn started_minimized<R: Runtime>(app: &AppHandle<R>) -> bool {
    std::env::args().any(|a| a == MINIMIZED_ARG) && load_settings(app).system.start_minimized
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> AppResult<()> {
    let al = app.autolaunch();
    let r = if enabled { al.enable() } else { al.disable() };
    r.map_err(|e| AppError::Internal(format!("Could not change start-with-Windows: {e}")))
}

#[tauri::command]
pub fn autostart_enabled(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Native notification. Respects the user's setting; text is length-limited.
#[tauri::command]
pub fn notify(app: AppHandle, state: State<'_, AppState>, title: String, body: String) -> AppResult<()> {
    if !state.db.with(settings::load)?.system.notifications {
        return Ok(());
    }
    let clip = |s: &str, n: usize| s.chars().filter(|c| !c.is_control()).take(n).collect::<String>();
    app.notification()
        .builder()
        .title(clip(&title, 80))
        .body(clip(&body, 240))
        .show()
        .map_err(|e| AppError::Internal(format!("Notification failed: {e}")))
}
