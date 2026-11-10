//! Native window appearance: Windows 11 backdrop (Acrylic / Mica) behind the
//! translucent webview, and the title bar's light/dark theme.

use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{Runtime, Theme, WebviewWindow};

use crate::error::{AppError, AppResult};

/// `theme`: "dark" | "light" | "system".
pub fn apply_appearance<R: Runtime>(window: &WebviewWindow<R>, material: &str, theme: &str) -> AppResult<()> {
    let native_theme = match theme {
        "dark" => Some(Theme::Dark),
        "light" => Some(Theme::Light),
        "system" => None,
        _ => return Err(AppError::Validation("Unknown theme".into())),
    };
    let _ = window.set_theme(native_theme);
    let light = match native_theme {
        Some(t) => t == Theme::Light,
        None => window.theme().map(|t| t == Theme::Light).unwrap_or(false),
    };
    let effects = match material {
        "acrylic" => Some(EffectsBuilder::new().effect(Effect::Acrylic).state(EffectState::Active).build()),
        "mica" => Some(EffectsBuilder::new().effect(if light { Effect::MicaLight } else { Effect::MicaDark }).state(EffectState::Active).build()),
        "none" => None,
        _ => return Err(AppError::Validation("Unknown window material".into())),
    };
    window.set_effects(effects).map_err(|e| AppError::Internal(format!("Could not apply window material: {e}")))
}

#[tauri::command]
pub fn set_window_appearance(window: WebviewWindow, material: String, theme: String) -> AppResult<()> {
    apply_appearance(&window, &material, &theme)
}
