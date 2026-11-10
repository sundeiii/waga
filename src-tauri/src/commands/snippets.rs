use tauri::State;

use crate::database::snippets::{self, Snippet, SnippetInput};
use crate::error::AppResult;
use crate::state::AppState;

#[tauri::command]
pub fn list_snippets(state: State<'_, AppState>) -> AppResult<Vec<Snippet>> {
    state.db.with(snippets::list)
}

#[tauri::command]
pub fn create_snippet(state: State<'_, AppState>, input: SnippetInput) -> AppResult<Snippet> {
    state.db.with(|c| snippets::insert(c, &input))
}

#[tauri::command]
pub fn update_snippet(state: State<'_, AppState>, id: String, input: SnippetInput) -> AppResult<Snippet> {
    state.db.with(|c| snippets::update(c, &id, &input))
}

#[tauri::command]
pub fn delete_snippet(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.with(|c| snippets::delete(c, &id))
}

#[tauri::command]
pub fn set_snippet_favorite(state: State<'_, AppState>, id: String, favorite: bool) -> AppResult<()> {
    state.db.with(|c| snippets::set_favorite(c, &id, favorite))
}

#[tauri::command]
pub fn mark_snippet_used(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.with(|c| snippets::mark_used(c, &id))
}
