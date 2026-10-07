mod chrome;
mod clipboard;
mod dialogs;
mod files;
mod library;
mod recent;
mod scope;
mod session;
mod settings;
mod store;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{Emitter, Manager, Url};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_window_state::StateFlags;

/// Drawings waiting for the renderer to open them: those named on the command
/// line — a file-manager double click arrives this way, and the desktop
/// entry's `%F` may name several at once — and those a second launch has
/// handed over since. Taken, not read, so a reload does not reopen them over
/// the user's current work.
#[derive(Default)]
struct StartupFiles(Mutex<Vec<String>>);

impl StartupFiles {
    fn add(&self, files: Vec<String>) {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .extend(files);
    }
}

#[tauri::command]
fn startup_files(state: tauri::State<'_, StartupFiles>) -> Vec<String> {
    state
        .0
        .lock()
        .map(|mut files| std::mem::take(&mut *files))
        .unwrap_or_default()
}

/// Tells the window already open that a second launch has added to
/// `StartupFiles`. It carries no paths: an event sent before the renderer is
/// listening is simply lost, and a drawing double-clicked while the app was
/// still starting went with it.
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

/// Whether the webview may follow a navigation: to the app's own page — the
/// bundled `tauri://` origin, or Vite's dev server under `npm start` — or to a
/// `blob:` URL. A query string is refused even on the app's origin: an
/// Excalidraw element link is the app's own URL plus `?element=`, and
/// following it would reload the page and every tab with it.
///
/// `blob:` is let through because WebKitGTK routes a download link through
/// here too, and Excalidraw's "Export library" is one: `<a download>` on a
/// blob URL. Only the page itself can create one.
///
/// Frames come through here as well as the page (checked in WebKitGTK 2.54),
/// so refusing `about:srcdoc` is what stops a drawing's HTML from running: an
/// Excalidraw `iframe` element renders whatever `customData` a file carries as
/// a srcdoc frame, scripts allowed. A remote frame never gets this far — the
/// CSP refuses it first.
fn allowed_navigation(url: &Url, dev_url: Option<&Url>) -> bool {
    if url.scheme() == "blob" {
        return true;
    }
    // The bundled page is `tauri://localhost/` and nothing else on that scheme.
    let bundled = url.scheme() == "tauri" && url.host_str() == Some("localhost");
    let ours = bundled || dev_url.is_some_and(|dev| dev.origin() == url.origin());
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
    // `dev_url` is compiled into the config of a release build too, where
    // nothing of ours is listening on it: left in, whatever does listen on
    // that port would have been a page this window agreed to load.
    let dev_url = if cfg!(dev) {
        app.config().build.dev_url.clone()
    } else {
        None
    };
    let navigating = app.handle().clone();
    let opening = app.handle().clone();
    tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
        .on_navigation(move |url| {
            if allowed_navigation(url, dev_url.as_ref()) {
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

/// The window is created hidden and shown by the renderer once its theme is on
/// screen, so a dark theme does not open as a white flash (see `App.tsx`). If
/// that never comes — a renderer that failed to load — it is shown anyway after
/// a few seconds, rather than leaving an app with no window at all.
fn show_eventually(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        if let Some(window) = app.get_webview_window("main") {
            if !window.is_visible().unwrap_or(true) {
                let _ = window.show();
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First, as the plugin requires. Without it a double-clicked drawing
        // would start a second app against the same config directory, and the
        // two would prune each other's session snapshots.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let files = cli_drawings(&argv, Path::new(&cwd));
            let allowed = app.state::<scope::Allowed>();
            for file in &files {
                allowed.allow(Path::new(file));
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            if !files.is_empty() {
                app.state::<StartupFiles>().add(files);
                let _ = app.emit(OPEN_FILES_EVENT, ());
            }
        }))
        // Here rather than in `setup`, so that a second launch arriving while
        // this one is still starting finds them.
        .manage(scope::Allowed::default())
        .manage(StartupFiles::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // Everything but visibility: the window stays hidden until the theme is
        // painted, and restoring visibility would show it straight away.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .build(),
        )
        .setup(|app| {
            // Before anything writes into it: `save_settings` creating it
            // first would otherwise leave the directory holding `recent.json`
            // — every path the user has opened — at whatever the umask makes
            // it. Best effort: the writes themselves still report their own
            // failures, and a config directory we cannot tighten is not a
            // reason to refuse to start.
            let _ = store::ensure_private_dir(&store::config_dir());
            // Whatever a run that died mid-write left behind. This is the
            // only instance and nothing is writing yet.
            for dir in ["", "session", "themes"] {
                files::sweep_temp_files(&store::config_dir().join(dir));
            }
            // Named on the command line by the user, so the renderer may read them.
            let files = startup_drawings();
            let allowed = app.state::<scope::Allowed>();
            for file in &files {
                allowed.allow(Path::new(file));
            }
            app.state::<StartupFiles>().add(files);
            create_main_window(app)?;
            // Setup runs on the GTK main thread, where the signal has to be subscribed.
            #[cfg(target_os = "linux")]
            settings::watch_color_scheme(app.handle().clone());
            show_eventually(app.handle().clone());
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
            session::keep_unreadable_snapshot,
            settings::load_settings,
            settings::save_settings,
            settings::themes_dir_path,
            settings::list_user_themes,
            settings::system_color_scheme,
            settings::save_user_theme,
            settings::delete_user_theme,
            settings::load_export_preferences,
            settings::save_export_preferences,
            library::load_library,
            library::save_library,
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
        let page = |s: &str| allowed_navigation(&Url::parse(s).unwrap(), Some(&dev));
        assert!(page("tauri://localhost/"));
        assert!(!page("tauri://elsewhere/"), "only the bundled page lives on that scheme");
        assert!(page("http://localhost:1420/"));
        assert!(page("blob:tauri://localhost/2f6c0e1a"), "a library export is a blob download");
        assert!(!page("tauri://localhost/?element=abc"), "an element link would reload the app");
        assert!(!page("https://example.com/"));
        assert!(!page("http://localhost:8080/"));
        assert!(!page("file:///home/rafa/.bashrc"));
        assert!(!page("about:srcdoc"), "a drawing's embedded HTML must not load");
        assert!(!page("about:blank"));
        let dev_page = Url::parse("http://localhost:1420/").unwrap();
        assert!(!allowed_navigation(&dev_page, None), "no dev server in a release build");
    }
}
