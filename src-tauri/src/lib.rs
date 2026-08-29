mod chrome;
mod clipboard;
mod files;
mod recent;
mod session;
mod settings;
mod store;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// Drawings named on the command line. A file-manager double click arrives this
/// way, and the desktop entry's `%F` may name several at once.
/// Taken once, so a reload does not reopen them over the user's current work.
#[derive(Default)]
struct StartupFiles(Mutex<Vec<String>>);

#[tauri::command]
fn startup_files(state: tauri::State<'_, StartupFiles>) -> Vec<String> {
    state
        .0
        .lock()
        .map(|mut files| std::mem::take(&mut *files))
        .unwrap_or_default()
}

/// Carries the drawings from a second launch to the window already open.
const OPEN_FILES_EVENT: &str = "open-files";

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

/// The drawings in an argument list, in the order they were given.
///
/// Relative paths resolve against `cwd` rather than ours, because a second
/// launch hands its arguments to the instance already running, which may sit in
/// a different directory entirely. Paths are canonicalised so that the same
/// drawing reached two ways lands in one tab rather than two.
fn cli_drawings(args: &[String], cwd: &Path) -> Vec<String> {
    args.iter()
        .skip(1) // the program itself
        .filter(|arg| !arg.starts_with('-'))
        .map(|arg| {
            let path = Path::new(arg);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            }
        })
        .filter(|path| path.is_file())
        .map(|path| {
            std::fs::canonicalize(&path)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// What this process was asked to open when it started.
fn startup_drawings() -> Vec<String> {
    let args: Vec<String> = std::env::args().collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    cli_drawings(&args, &cwd)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First, as the plugin requires. Without it a double-clicked drawing
        // would start a second app against the same config directory, and the
        // two would prune each other's session snapshots.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let files = cli_drawings(&argv, Path::new(&cwd));
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            if !files.is_empty() {
                let _ = app.emit(OPEN_FILES_EVENT, files);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            app.manage(StartupFiles(Mutex::new(startup_drawings())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup_files,
            set_window_title,
            files::read_text_file,
            files::write_text_file,
            files::write_binary_file,
            chrome::set_menu_colors,
            clipboard::copy_image_to_clipboard,
            recent::list_recent,
            recent::push_recent,
            recent::clear_recent,
            session::save_session,
            session::load_session,
            session::mark_clean_exit,
            session::clear_session,
            settings::load_settings,
            settings::save_settings,
            settings::themes_dir_path,
            settings::list_user_themes,
            settings::system_color_scheme,
            settings::save_user_theme,
            settings::delete_user_theme,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Excalidraw Desktop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawings_are_picked_out_of_an_argument_list() {
        let dir = std::env::temp_dir().join(format!("excalidraw-cli-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.excalidraw");
        std::fs::write(&file, "{}").unwrap();
        let expected = std::fs::canonicalize(&file)
            .unwrap()
            .to_string_lossy()
            .into_owned();

        let args = [
            "/usr/bin/excalidraw-desktop", // the program itself is not a drawing
            "--some-switch",               // nor is a switch
            "a.excalidraw",                // relative to the caller's directory
            &file.to_string_lossy(),       // absolute
            &dir.join("gone.excalidraw").to_string_lossy(), // deleted since
        ]
        .map(String::from);

        let found = cli_drawings(&args, &dir);
        assert_eq!(found, vec![expected.clone(), expected], "both spellings resolve to one file");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
