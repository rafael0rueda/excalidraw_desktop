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

/// Resolves a theme id to its file, refusing anything that is not a plain
/// slug. Ids reach us from the renderer and end up in a path, so `../` and
/// absolute paths have to be impossible rather than merely unlikely.
fn theme_file(id: &str) -> Result<PathBuf, String> {
    let valid = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(format!(
            "invalid theme id {id:?} — use lower-case letters, digits and dashes"
        ));
    }
    Ok(themes_dir().join(format!("{id}.json")))
}

/// Writes a theme to the themes directory, returning the file it landed in.
/// The value is stored as given: the renderer owns the schema, and a theme it
/// wrote is a theme it can read back.
#[tauri::command]
pub fn save_user_theme(theme: serde_json::Value) -> Result<String, String> {
    let id = theme
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "theme has no id".to_string())?;
    let path = theme_file(id)?;
    let json = serde_json::to_string_pretty(&theme).map_err(|e| e.to_string())?;
    write_atomic(&path, json.as_bytes())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Removes a user theme. A missing file is success: the caller wanted it gone.
#[tauri::command]
pub fn delete_user_theme(id: String) -> Result<(), String> {
    let path = theme_file(&id)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::theme_file;

    #[test]
    fn theme_ids_stay_inside_the_themes_directory() {
        assert!(theme_file("kanagawa-wave").is_ok());
        assert!(theme_file("solarized-light-2").is_ok());

        for bad in ["", "../escape", "/etc/passwd", "Upper", "with space", "dot.dot"] {
            assert!(theme_file(bad).is_err(), "{bad:?} should be rejected");
        }
        assert!(theme_file(&"a".repeat(65)).is_err());
    }
}
