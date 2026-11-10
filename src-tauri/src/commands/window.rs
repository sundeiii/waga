//! Native window backdrop (Windows 11 Acrylic / Mica) behind the translucent webview.

use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{Runtime, WebviewWindow};

use crate::error::{AppError, AppResult};

pub fn apply_material<R: Runtime>(window: &WebviewWindow<R>, material: &str) -> AppResult<()> {
    let effects = match material {
        "acrylic" => Some(EffectsBuilder::new().effect(Effect::Acrylic).state(EffectState::Active).build()),
        "mica" => Some(EffectsBuilder::new().effect(Effect::MicaDark).state(EffectState::Active).build()),
        "none" => None,
        _ => return Err(AppError::Validation("Unknown window material".into())),
    };
    window.set_effects(effects).map_err(|e| AppError::Internal(format!("Could not apply window material: {e}")))
}

#[tauri::command]
pub fn set_window_material(window: WebviewWindow, material: String) -> AppResult<()> {
    apply_material(&window, &material)
}
