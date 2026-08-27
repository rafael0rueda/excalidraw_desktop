mod clipboard;
mod files;
mod recent;

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
/// tao's `set_title` queues the change and returns `Ok` before GTK has applied
/// it, and on GNOME/Wayland the update does not reach the compositor, so the
/// titlebar keeps showing the value from tauri.conf.json. Setting the title on
/// the GTK window directly — from the GTK main thread — does propagate.
#[tauri::command]
fn set_window_title(window: tauri::Window, title: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        // gtk::ApplicationWindow is !Send, so resolve it inside the closure.
        let target = window.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        window
            .app_handle()
            .run_on_main_thread(move || {
                use gtk::prelude::GtkWindowExt;
                let result = target
                    .gtk_window()
                    .map(|gtk_window| gtk_window.set_title(&title))
                    .map_err(|e| e.to_string());
                let _ = tx.send(result);
            })
            .map_err(|e| e.to_string())?;
        return rx.recv().map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "linux"))]
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Excalidraw Desktop");
}
