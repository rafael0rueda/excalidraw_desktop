//! Persisted preferences and user-supplied themes.

use crate::files::write_atomic;
use crate::store::{config_dir, safe_id};
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

#[tauri::command(async)]
pub fn load_settings() -> Settings {
    // A corrupt or half-written file must not stop the app from starting, and
    // `#[serde(default)]` means a file from an older version still loads.
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command(async)]
pub fn save_settings(settings: Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    write_atomic(&settings_path(), json.as_bytes())
}

#[tauri::command]
pub fn themes_dir_path() -> String {
    themes_dir().to_string_lossy().into_owned()
}

/// One theme file: either the JSON it held, or why it could not even be read
/// or parsed. The renderer owns *schema* validation (a missing key, say) and
/// reports that itself; this covers the layer beneath it, which used to be
/// dropped with a silent `.ok()` — a file that was not valid JSON at all had
/// no way to reach the renderer's own error reporting.
#[derive(Serialize)]
pub struct ThemeFile {
    pub value: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Every `*.json` under `dir`. The renderer validates the schema of whatever
/// parses; this only reports what stops a file from reaching that point.
/// Split out from the command so a test can point it at a scratch directory
/// instead of `config_dir()`'s process-wide `XDG_CONFIG_HOME`.
fn list_theme_files_at(dir: &std::path::Path) -> Vec<ThemeFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
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
        .map(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("<unknown>");
            let result = std::fs::read_to_string(p)
                .map_err(|e| e.to_string())
                .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()));
            match result {
                Ok(value) => ThemeFile {
                    value: Some(value),
                    error: None,
                },
                Err(e) => ThemeFile {
                    value: None,
                    error: Some(format!("{name}: {e}")),
                },
            }
        })
        .collect()
}

#[tauri::command(async)]
pub fn list_user_themes() -> Vec<ThemeFile> {
    list_theme_files_at(&themes_dir())
}

#[cfg(target_os = "linux")]
const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
#[cfg(target_os = "linux")]
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
#[cfg(target_os = "linux")]
const PORTAL_SETTINGS: &str = "org.freedesktop.portal.Settings";
#[cfg(target_os = "linux")]
const APPEARANCE: &str = "org.freedesktop.appearance";
/// `color-scheme` under `org.freedesktop.appearance`: 1 prefers dark, 2 prefers
/// light, 0 states no preference.
#[cfg(target_os = "linux")]
const PORTAL_PREFERS_DARK: u32 = 1;

/// Tells the renderer that the desktop switched between light and dark.
#[cfg(target_os = "linux")]
const COLOR_SCHEME_EVENT: &str = "color-scheme-changed";

/// A portal value arrives boxed in one `v`, or in two from the older `Read`.
#[cfg(target_os = "linux")]
fn unbox(mut value: gtk::glib::Variant) -> gtk::glib::Variant {
    // Type checked first: `as_variant` on anything but a `v` does return None,
    // but only after GLib logs a CRITICAL about it.
    while value.is_type(gtk::glib::VariantTy::VARIANT) {
        match value.as_variant() {
            Some(inner) => value = inner,
            None => break,
        }
    }
    value
}

/// The desktop portal's colour scheme. Unlike GNOME's gsettings key, every
/// desktop shipping a portal — GNOME, KDE and the rest — answers this the same
/// way. None when there is no portal to ask.
#[cfg(target_os = "linux")]
fn portal_prefers_dark() -> Option<bool> {
    use gtk::gio;
    use gtk::glib::ToVariant;
    let bus = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).ok()?;
    let args = (APPEARANCE, "color-scheme").to_variant();
    // `ReadOne` arrived in version 2 of the interface; older portals only have `Read`.
    let reply = ["ReadOne", "Read"].into_iter().find_map(|method| {
        bus.call_sync(
            Some(PORTAL_BUS),
            PORTAL_PATH,
            PORTAL_SETTINGS,
            method,
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            1000,
            None::<&gio::Cancellable>,
        )
        .ok()
    })?;
    let scheme = unbox(reply.try_child_value(0)?).get::<u32>()?;
    Some(scheme == PORTAL_PREFERS_DARK)
}

#[cfg(not(target_os = "linux"))]
fn portal_prefers_dark() -> Option<bool> {
    None
}

/// Follows the portal's change signal, so a desktop-wide switch between light
/// and dark repaints the app at once instead of on the next window focus.
/// Call on the GTK main thread, whose main loop delivers the signal.
#[cfg(target_os = "linux")]
pub fn watch_color_scheme(app: tauri::AppHandle) {
    use gtk::gio;
    use tauri::Emitter;
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else {
        return;
    };
    // Never unsubscribed: it is wanted for as long as the app runs.
    let _ = bus.signal_subscribe(
        Some(PORTAL_BUS),
        Some(PORTAL_SETTINGS),
        Some("SettingChanged"),
        Some(PORTAL_PATH),
        Some(APPEARANCE),
        gio::DBusSignalFlags::NONE,
        move |_, _, _, _, _, params| {
            let key = params.try_child_value(1).and_then(|key| key.get::<String>());
            if key.as_deref() != Some("color-scheme") {
                return;
            }
            let scheme = params.try_child_value(2).map(unbox).and_then(|v| v.get::<u32>());
            let dark = scheme == Some(PORTAL_PREFERS_DARK);
            let _ = app.emit(COLOR_SCHEME_EVENT, if dark { "dark" } else { "light" });
        },
    );
}

/// The desktop's light/dark preference: the portal where there is one, else
/// GNOME's gsettings key, else light rather than a guess. Off the main thread:
/// it may start a process, and runs on every window focus.
#[tauri::command(async)]
pub fn system_color_scheme() -> String {
    if let Some(dark) = portal_prefers_dark() {
        return if dark { "dark" } else { "light" }.into();
    }
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output();
    match out {
        Ok(o) if String::from_utf8_lossy(&o.stdout).contains("prefer-dark") => "dark".into(),
        _ => "light".into(),
    }
}

/// Resolves a theme id to its file, refusing anything that is not a plain slug.
fn theme_file(id: &str) -> Result<PathBuf, String> {
    safe_id(id)?;
    Ok(themes_dir().join(format!("{id}.json")))
}

/// Writes a theme to the themes directory, returning the file it landed in.
///
/// The caller hands us the finished text rather than a value to serialise:
/// these files are meant to be read and edited by hand, and the renderer holds
/// the keys in the order the schema documents them, which round-tripping
/// through `serde_json::Value` would sort alphabetically.
#[tauri::command(async)]
pub fn save_user_theme(id: String, contents: String) -> Result<String, String> {
    let path = theme_file(&id)?;
    // Parsed only to check it, never re-emitted.
    let value: serde_json::Value =
        serde_json::from_str(&contents).map_err(|e| format!("not valid JSON: {e}"))?;
    if value.get("id").and_then(|v| v.as_str()) != Some(id.as_str()) {
        return Err(format!("theme id does not match {}", path.display()));
    }
    write_atomic(&path, contents.as_bytes())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Removes a user theme. A missing file is success: the caller wanted it gone.
#[tauri::command(async)]
pub fn delete_user_theme(id: String) -> Result<(), String> {
    let path = theme_file(&id)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Choices from the Export menu. A file of their own, since `Settings` belongs
/// to the theme controller, which writes it whole.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ExportPreferences {
    /// PNG pixel density, 1 to 3.
    pub scale: u8,
    pub transparent: bool,
    /// Store the drawing inside exported images, so opening one in Excalidraw
    /// brings the scene back.
    pub embed_scene: bool,
}

impl Default for ExportPreferences {
    fn default() -> Self {
        Self {
            scale: 2,
            transparent: false,
            embed_scene: false,
        }
    }
}

impl ExportPreferences {
    fn sanitized(mut self) -> Self {
        self.scale = self.scale.clamp(1, 3);
        self
    }
}

fn export_path() -> PathBuf {
    config_dir().join("export.json")
}

#[tauri::command(async)]
pub fn load_export_preferences() -> ExportPreferences {
    std::fs::read_to_string(export_path())
        .ok()
        .and_then(|s| serde_json::from_str::<ExportPreferences>(&s).ok())
        .unwrap_or_default()
        .sanitized()
}

#[tauri::command(async)]
pub fn save_export_preferences(preferences: ExportPreferences) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&preferences.sanitized()).map_err(|e| e.to_string())?;
    write_atomic(&export_path(), json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{list_theme_files_at, theme_file, ExportPreferences};

    #[test]
    fn export_preferences_stay_in_range_and_fill_in_what_is_missing() {
        let partial: ExportPreferences = serde_json::from_str(r#"{"transparent":true}"#).unwrap();
        assert_eq!(partial, ExportPreferences { transparent: true, ..Default::default() });
        let zero = ExportPreferences { scale: 0, ..Default::default() };
        assert_eq!(zero.sanitized().scale, 1);
        let huge = ExportPreferences { scale: 9, ..Default::default() };
        assert_eq!(huge.sanitized().scale, 3);
    }

    /// The rule itself is covered in `store`; this is the theme layer applying it.
    #[test]
    fn theme_ids_stay_inside_the_themes_directory() {
        assert!(theme_file("solarized-light-2").is_ok());
        assert!(theme_file("../escape").is_err());
    }

    /// A file that is not valid JSON must still surface as *something* the
    /// renderer's `errors[]` reporting can show — `.ok()` used to drop it
    /// with no trace, silently at startup and even on an explicit reload.
    #[test]
    fn a_file_that_is_not_valid_json_is_reported_rather_than_dropped() {
        let dir = std::env::temp_dir().join(format!(
            "excalidraw-theme-files-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("good.json"), r#"{"id":"good"}"#).unwrap();
        std::fs::write(dir.join("broken.json"), "{ not json").unwrap();

        let files = list_theme_files_at(&dir);

        let good = files
            .iter()
            .find(|f| f.error.is_none())
            .expect("the valid file still parses");
        assert_eq!(good.value.as_ref().unwrap()["id"], "good");

        let broken = files
            .iter()
            .find(|f| f.value.is_none())
            .expect("the broken file is reported, not dropped");
        assert!(broken.error.as_ref().unwrap().starts_with("broken.json: "));
    }
}
