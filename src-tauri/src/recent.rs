//! Commands over `excalidraw_desktop_core::recent`.

use crate::scope::Allowed;
use excalidraw_desktop_core::recent::{self, RecentEntry};
use tauri::State;

#[tauri::command(async)]
pub fn list_recent(allowed: State<'_, Allowed>) -> Vec<RecentEntry> {
    recent::list(&allowed)
}

#[tauri::command(async)]
pub fn push_recent(allowed: State<'_, Allowed>, path: String) -> Result<Vec<RecentEntry>, String> {
    recent::push(&allowed, path)
}

#[tauri::command(async)]
pub fn clear_recent() -> Result<Vec<RecentEntry>, String> {
    recent::clear()
}
