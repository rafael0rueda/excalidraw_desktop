use base64::Engine;

/// Copies a PNG to the system clipboard through GTK rather than the web
/// clipboard API: WebKitGTK gates `navigator.clipboard.write()` behind user
/// activation that an app-initiated copy does not reliably carry.
#[tauri::command]
pub fn copy_image_to_clipboard(app: tauri::AppHandle, data: String) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map_err(|e| format!("bad image payload: {e}"))?;

    #[cfg(target_os = "linux")]
    {
        use tauri::Manager;
        let (tx, rx) = std::sync::mpsc::channel();
        // GTK is not thread-safe; clipboard work must happen on the main loop.
        app.run_on_main_thread(move || {
            let _ = tx.send(set_clipboard_image(&bytes));
        })
        .map_err(|e| e.to_string())?;
        let _ = &app;
        return rx.recv().map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, bytes);
        Err("clipboard image copy is only implemented for Linux".into())
    }
}

#[cfg(target_os = "linux")]
fn set_clipboard_image(bytes: &[u8]) -> Result<(), String> {
    use gdk_pixbuf::prelude::PixbufLoaderExt;
    use gtk::prelude::*;

    let loader = gdk_pixbuf::PixbufLoader::new();
    loader.write(bytes).map_err(|e| e.to_string())?;
    loader.close().map_err(|e| e.to_string())?;
    let pixbuf = loader.pixbuf().ok_or("decoded image was empty")?;

    let display = gtk::gdk::Display::default().ok_or("no display available")?;
    let clipboard = gtk::Clipboard::default(&display).ok_or("no clipboard available")?;
    clipboard.set_image(&pixbuf);
    // Ask the clipboard manager to keep the image once we exit.
    clipboard.store();
    Ok(())
}
