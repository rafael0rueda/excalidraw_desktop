//! Commands over `excalidraw_desktop_core::settings`, plus the desktop's
//! light/dark preference, which needs GTK's D-Bus and so stays in this crate.

use excalidraw_desktop_core::settings::{self, ExportPreferences, Settings, ThemeFile};

#[tauri::command(async)]
pub fn load_settings() -> Settings {
    settings::load_settings()
}

#[tauri::command(async)]
pub fn save_settings(settings: Settings) -> Result<(), String> {
    settings::save_settings(settings)
}

#[tauri::command]
pub fn themes_dir_path() -> String {
    settings::themes_dir_path()
}

#[tauri::command(async)]
pub fn list_user_themes() -> Vec<ThemeFile> {
    settings::list_user_themes()
}

#[tauri::command(async)]
pub fn save_user_theme(id: String, contents: String) -> Result<String, String> {
    settings::save_user_theme(id, contents)
}

#[tauri::command(async)]
pub fn delete_user_theme(id: String) -> Result<(), String> {
    settings::delete_user_theme(id)
}

#[tauri::command(async)]
pub fn load_export_preferences() -> ExportPreferences {
    settings::load_export_preferences()
}

#[tauri::command(async)]
pub fn save_export_preferences(preferences: ExportPreferences) -> Result<(), String> {
    settings::save_export_preferences(preferences)
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

