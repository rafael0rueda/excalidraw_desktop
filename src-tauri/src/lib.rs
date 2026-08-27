mod clipboard;
mod files;
mod recent;
mod session;
mod store;

use std::sync::Mutex;
use tauri::Manager;

/// A drawing path handed to us on the command line (file-manager double click).
/// Taken once, so a reload does not reopen it over the user's current work.
#[derive(Default)]
struct StartupFile(Mutex<Option<String>>);

#[tauri::command]
fn startup_file(state: tauri::State<'_, StartupFile>) -> Option<String> {
    state.0.lock().ok().and_then(|mut slot| slot.take())
}

/// Sets the window title.
///
/// KNOWN LIMITATION on GNOME/Wayland: the titlebar keeps showing the title from
/// tauri.conf.json and does not reflect this call. The change is genuinely
/// applied — the single visible, decorated GtkApplicationWindow reports the new
/// title — but Mutter does not pick up the runtime update. Setting it on the
/// GTK window from the GTK main thread and flushing the display did not help
/// either. See PROGRESS.md ("Parked: window title on GNOME/Wayland").
///
/// Kept because it is correct, costs nothing, and works on other platforms.
#[tauri::command]
fn set_window_title(window: tauri::Window, title: String) -> Result<(), String> {
    window.set_title(&title).map_err(|e| e.to_string())
}

fn cli_drawing() -> Option<String> {
    std::env::args()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .filter(|arg| std::path::Path::new(arg).is_file())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            app.manage(StartupFile(Mutex::new(cli_drawing())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup_file,
            set_window_title,
            files::read_text_file,
            files::write_text_file,
            files::write_binary_file,
            clipboard::copy_image_to_clipboard,
            recent::list_recent,
            recent::push_recent,
            recent::clear_recent,
            session::save_session,
            session::load_session,
            session::mark_clean_exit,
            session::clear_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Excalidraw Desktop");
}
