//! The renderer's file commands. The rules for reading and writing — atomic
//! replacement, size limits, extensions — live in `excalidraw_desktop_core::files`.

use crate::scope::Allowed;
use base64::Engine;
use excalidraw_desktop_core::files::{readable, require_extension, write_atomic, MAX_TEXT_BYTES, TEXT_EXTENSIONS};
use tauri::State;

// File commands run on Tauri's thread pool rather than inline in the IPC
// handler, which is the GTK main thread: a large drawing read or written there
// froze the whole window for as long as it took.
#[tauri::command(async)]
pub fn read_text_file(allowed: State<'_, Allowed>, path: String) -> Result<String, String> {
    let path = allowed.check(&path)?;
    readable(&path, MAX_TEXT_BYTES)?;
    std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

#[tauri::command(async)]
pub fn write_text_file(allowed: State<'_, Allowed>, path: String, contents: String) -> Result<(), String> {
    let target = allowed.check(&path)?;
    require_extension(&target, TEXT_EXTENSIONS)?;
    write_atomic(&target, contents.as_bytes())
}

/// Header carrying the target of a raw-body write, base64 so that any file name
/// survives header encoding.
const PATH_HEADER: &str = "x-path";

fn header_path(request: &tauri::ipc::Request<'_>) -> Result<String, String> {
    let encoded = request
        .headers()
        .get(PATH_HEADER)
        .ok_or("missing path")?
        .to_str()
        .map_err(|e| e.to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("bad path header: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("bad path header: {e}"))
}

/// A PNG export. The bytes are the raw request body: base64 inside JSON meant
/// encoding, copying and decoding a multi-megabyte image on every export.
#[tauri::command(async)]
pub fn write_binary_file(allowed: State<'_, Allowed>, request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let path = header_path(&request)?;
    let target = allowed.check(&path)?;
    // Only a PNG export is binary. Without this, an allowed drawing could be
    // overwritten with image bytes.
    require_extension(&target, &["png"])?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("expected the image as raw bytes".into());
    };
    write_atomic(&target, bytes)
}
