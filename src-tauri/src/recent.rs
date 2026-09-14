use crate::files::write_atomic;
use crate::scope::Allowed;
use crate::store::{config_dir, now};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::State;

const MAX_RECENT: usize = 10;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RecentEntry {
    pub path: String,
    pub name: String,
    pub opened_at: u64,
}

fn recent_path() -> PathBuf {
    config_dir().join("recent.json")
}

fn load() -> Vec<RecentEntry> {
    std::fs::read_to_string(recent_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn store(entries: &[RecentEntry]) {
    if let Ok(json) = serde_json::to_string_pretty(entries) {
        // Atomic like every other file here: a crash mid-write would leave a
        // file that no longer parses, and so an empty list.
        let _ = write_atomic(&recent_path(), json.as_bytes());
    }
}

#[tauri::command]
pub fn list_recent(allowed: State<'_, Allowed>) -> Vec<RecentEntry> {
    // Drop entries whose files have since been deleted or moved.
    let entries: Vec<_> = load()
        .into_iter()
        .filter(|e| Path::new(&e.path).exists())
        .collect();
    // Every entry got onto the list through `push_recent`, which takes only
    // allowed paths, so choosing one from the menu may open it.
    for entry in &entries {
        allowed.allow(Path::new(&entry.path));
    }
    entries
}

#[tauri::command]
pub fn push_recent(allowed: State<'_, Allowed>, path: String) -> Vec<RecentEntry> {
    // The list is what `list_recent` allows, so the renderer must not be able
    // to put just any path on it.
    if allowed.check(&path).is_err() {
        return load();
    }
    let name = Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&path)
        .to_string();

    let mut entries = load();
    entries.retain(|e| e.path != path);
    entries.insert(
        0,
        RecentEntry {
            path,
            name,
            opened_at: now(),
        },
    );
    entries.truncate(MAX_RECENT);
    store(&entries);
    entries
}

#[tauri::command]
pub fn clear_recent() -> Vec<RecentEntry> {
    store(&[]);
    Vec::new()
}
