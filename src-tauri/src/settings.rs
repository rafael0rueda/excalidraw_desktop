//! Persisted preferences and user-supplied themes.

use crate::files::write_atomic;
use crate::store::config_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// `theme` is either a theme id or the literal `"system"`, in which case
/// `light_theme` / `dark_theme` are picked by the desktop's colour scheme.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub light_theme: String,
    pub dark_theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            light_theme: "kanagawa-lotus".into(),
            dark_theme: "kanagawa-wave".into(),
        }
    }
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

#[tauri::command]
pub fn load_settings() -> Settings {
    // A corrupt or half-written file must not stop the app from starting, and
    // `#[serde(default)]` means a file from an older version still loads.
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn save_settings(settings: Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    write_atomic(&settings_path(), json.as_bytes())
}

#[tauri::command]
pub fn themes_dir_path() -> String {
    themes_dir().to_string_lossy().into_owned()
}

/// Every `*.json` under the themes directory, returned raw — the renderer owns
/// the schema, so it does the validating and reports what it rejected.
#[tauri::command]
pub fn list_user_themes() -> Vec<serde_json::Value> {
    let Ok(entries) = std::fs::read_dir(themes_dir()) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();

    paths
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect()
}

/// The desktop's light/dark preference. GNOME exposes it through gsettings;
/// anything else falls back to light rather than guessing.
#[tauri::command]
pub fn system_color_scheme() -> String {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output();
    match out {
        Ok(o) if String::from_utf8_lossy(&o.stdout).contains("prefer-dark") => "dark".into(),
        _ => "light".into(),
    }
}
