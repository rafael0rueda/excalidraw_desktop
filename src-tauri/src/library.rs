//! Commands over `excalidraw_desktop_core::library`.

use excalidraw_desktop_core::library;

#[tauri::command(async)]
pub fn load_library() -> Result<Option<String>, String> {
    library::load_library()
}

#[tauri::command(async)]
pub fn save_library(contents: String) -> Result<(), String> {
    library::save_library(contents)
}
