//! The Excalidraw library: the shapes in its Library sidebar.
//!
//! The package keeps the library in memory only — the web app's persistence
//! lives in excalidraw.com's own code, not in the component — so without this
//! every item was gone on the next launch.

use crate::files::write_atomic;
use crate::store::config_dir;
use std::path::PathBuf;

fn library_path() -> PathBuf {
    config_dir().join("library.excalidrawlib")
}

/// The saved library as text, or None if none was ever saved. A file that
/// exists but cannot be read is an error rather than None: the renderer seeds
/// a library that is missing, and would then save that over the user's own.
#[tauri::command(async)]
pub fn load_library() -> Result<Option<String>, String> {
    match std::fs::read_to_string(library_path()) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", library_path().display())),
    }
}

/// Writes the library. Parsed only to refuse something that is not JSON;
/// the caller's text is what reaches the disk.
#[tauri::command(async)]
pub fn save_library(contents: String) -> Result<(), String> {
    serde_json::from_str::<serde_json::Value>(&contents).map_err(|e| format!("not valid JSON: {e}"))?;
    write_atomic(&library_path(), contents.as_bytes())
}
