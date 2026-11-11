//! Commands over `excalidraw_desktop_core::session`, the autosave snapshots.

use crate::scope::Allowed;
use excalidraw_desktop_core::session::{self, Session, TabInput};
use tauri::State;

#[tauri::command(async)]
pub fn save_session(allowed: State<'_, Allowed>, tabs: Vec<TabInput>, active: Option<String>) -> Result<(), String> {
    session::save(&allowed, tabs, active)
}

#[tauri::command(async)]
pub fn load_session(allowed: State<'_, Allowed>) -> Option<Session> {
    session::load(&allowed)
}

/// Records that we are shutting down on purpose, so the next launch reopens the
/// last drawings instead of offering to recover from them.
#[tauri::command(async)]
pub fn mark_clean_exit() -> Result<(), String> {
    session::mark_clean_exit()
}

#[tauri::command(async)]
pub fn keep_unreadable_snapshot(id: String) -> Result<String, String> {
    session::keep_unreadable_snapshot(id)
}
