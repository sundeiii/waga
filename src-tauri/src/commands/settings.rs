use tauri::State;

use crate::database::settings::{self, AppSettings};
use crate::error::AppResult;
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
