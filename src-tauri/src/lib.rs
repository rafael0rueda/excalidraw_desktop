mod chrome;
mod clipboard;
mod dialogs;
mod files;
mod recent;
mod scope;
mod session;
mod settings;
mod store;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{Emitter, Manager, Url};
use tauri_plugin_opener::OpenerExt;

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

/// Whether a navigation stays on the app's own page: the bundled `tauri://`
/// origin, or Vite's dev server under `npm start`. A query string is refused
/// even there — an Excalidraw element link is the app's own URL plus
/// `?element=`, and following it would reload the page and every tab with it.
fn is_app_page(url: &Url, dev_url: Option<&Url>) -> bool {
    let ours = url.scheme() == "tauri" || dev_url.is_some_and(|dev| dev.origin() == url.origin());
    ours && url.query().is_none()
}

/// Hands a web or mail link to the desktop's own handler. Anything else — a
/// `file:` URL, a custom scheme — is dropped.
fn open_externally(app: &tauri::AppHandle, url: &Url) {
    if matches!(url.scheme(), "http" | "https" | "mailto") {
        let _ = app.opener().open_url(url.as_str(), None::<&str>);
    }
}

/// Builds the main window from its `tauri.conf.json` entry (`create: false`
/// there) so that it can refuse to leave the app. Excalidraw follows links in
/// the webview itself: a link in a drawing someone sent, or one in the Help
/// dialog, would otherwise replace the app with a web page — unsaved work and
/// the close guard along with it.
fn create_main_window(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .ok_or("tauri.conf.json has no main window")?;
    let dev_url = app.config().build.dev_url.clone();
    let navigating = app.handle().clone();
    let opening = app.handle().clone();
    tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
        .on_navigation(move |url| {
            if is_app_page(url, dev_url.as_ref()) {
                return true;
            }
            open_externally(&navigating, url);
            false
        })
        .on_new_window(move |url, _features| {
            open_externally(&opening, &url);
            tauri::webview::NewWindowResponse::Deny
        })
        .build()?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First, as the plugin requires. Without it a double-clicked drawing
        // would start a second app against the same config directory, and the
        // two would prune each other's session snapshots.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let files = cli_drawings(&argv, Path::new(&cwd));
            if let Some(allowed) = app.try_state::<scope::Allowed>() {
                for file in &files {
                    allowed.allow(Path::new(file));
                }
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            if !files.is_empty() {
                let _ = app.emit(OPEN_FILES_EVENT, files);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            // Named on the command line by the user, so the renderer may read them.
            let files = startup_drawings();
            let allowed = scope::Allowed::default();
            for file in &files {
                allowed.allow(Path::new(file));
            }
            app.manage(allowed);
            app.manage(StartupFiles(Mutex::new(files)));
            create_main_window(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup_files,
            set_window_title,
            files::read_text_file,
            files::write_text_file,
            files::write_binary_file,
            dialogs::pick_open_path,
            dialogs::pick_save_path,
            chrome::set_menu_colors,
            chrome::set_prefer_dark_theme,
            clipboard::copy_image_to_clipboard,
            recent::list_recent,
            recent::push_recent,
            recent::clear_recent,
            session::save_session,
            session::load_session,
            session::mark_clean_exit,
            session::clear_session,
            session::keep_unreadable_snapshot,
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

    #[test]
    fn only_the_app_page_itself_may_be_navigated_to() {
        let dev = Url::parse("http://localhost:1420").unwrap();
        let page = |s: &str| is_app_page(&Url::parse(s).unwrap(), Some(&dev));
        assert!(page("tauri://localhost/"));
        assert!(page("http://localhost:1420/"));
        assert!(!page("tauri://localhost/?element=abc"), "an element link would reload the app");
        assert!(!page("https://example.com/"));
        assert!(!page("http://localhost:8080/"));
        let dev_page = Url::parse("http://localhost:1420/").unwrap();
        assert!(!is_app_page(&dev_page, None), "no dev server in a release build");
    }
}
