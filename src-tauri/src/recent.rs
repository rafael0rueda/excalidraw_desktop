use crate::store::{config_dir, now};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    let _ = std::fs::create_dir_all(config_dir());
    if let Ok(json) = serde_json::to_string_pretty(entries) {
        let _ = std::fs::write(recent_path(), json);
    }
}

#[tauri::command]
pub fn list_recent() -> Vec<RecentEntry> {
    // Drop entries whose files have since been deleted or moved.
    let entries: Vec<_> = load()
        .into_iter()
        .filter(|e| std::path::Path::new(&e.path).exists())
        .collect();
    entries
}

#[tauri::command]
pub fn push_recent(path: String) -> Vec<RecentEntry> {
    let name = std::path::Path::new(&path)
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
