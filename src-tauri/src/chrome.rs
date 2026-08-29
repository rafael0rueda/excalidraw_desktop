//! Colours for the parts of the window GTK draws rather than the webview.
//!
//! The menu bar and the menus that drop out of it are real GTK widgets, so they
//! keep the desktop's widget theme — a white strip above a dark canvas — and no
//! stylesheet in the page can reach them. A CSS provider on the default screen
//! is the hook GTK offers for that, and it applies at
//! `STYLE_PROVIDER_PRIORITY_APPLICATION`, which outranks the widget theme.

use serde::Deserialize;

/// The slice of a theme the GTK chrome needs. Named for the roles rather than
/// the theme's own keys, because these six do the work of ten here.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MenuColors {
    pub surface: String,
    pub text: String,
    pub muted: String,
    pub accent: String,
    pub accent_text: String,
    pub border: String,
}

/// A colour is about to be interpolated into a stylesheet, so anything that is
/// not plainly one is refused rather than escaped. Themes are local files the
/// user wrote, but a value from a file should not be able to write CSS rules.
fn css_color(value: &str) -> Result<&str, String> {
    let hex = value.strip_prefix('#').is_some_and(|rest| {
        matches!(rest.len(), 3 | 4 | 6 | 8) && rest.chars().all(|c| c.is_ascii_hexdigit())
    });
    if hex || value == "transparent" {
        Ok(value)
    } else {
        Err(format!("not a colour: {value:?}"))
    }
}

fn stylesheet(c: &MenuColors) -> Result<String, String> {
    let surface = css_color(&c.surface)?;
    let text = css_color(&c.text)?;
    let muted = css_color(&c.muted)?;
    let accent = css_color(&c.accent)?;
    let accent_text = css_color(&c.accent_text)?;
    let border = css_color(&c.border)?;

    // `:backdrop` is spelled out because the widget theme has its own rules for
    // it, which would otherwise grey the bar out whenever the window loses focus.
    Ok(format!(
        "menubar, menubar > menuitem {{
  background-color: {surface};
  color: {text};
}}
menubar {{
  border-bottom: 1px solid {border};
}}
menubar > menuitem:hover, menubar > menuitem:selected {{
  background-color: {accent};
  color: {accent_text};
}}
menubar:backdrop, menubar > menuitem:backdrop {{
  background-color: {surface};
  color: {muted};
}}

/* The drop-downs — and the popup WebKit puts up for a <select>. */
menu, menu > menuitem {{
  background-color: {surface};
  color: {text};
}}
menu > menuitem:hover, menu > menuitem:selected {{
  background-color: {accent};
  color: {accent_text};
}}
menu > menuitem:disabled {{
  color: {muted};
}}
menu separator {{
  background-color: {border};
}}
menu menuitem check, menu menuitem radio {{
  color: {text};
}}
"
    ))
}

/// Paints the GTK chrome in a theme's colours. Repeated calls replace the last
/// stylesheet rather than stacking another one on top of it.
#[tauri::command]
pub fn set_menu_colors(app: tauri::AppHandle, colors: MenuColors) -> Result<(), String> {
    let css = stylesheet(&colors)?;

    #[cfg(target_os = "linux")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        // GTK is not thread-safe; style providers belong to the main loop.
        app.run_on_main_thread(move || {
            let _ = tx.send(install(css));
        })
        .map_err(|e| e.to_string())?;
        return rx.recv().map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, css);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn install(css: String) -> Result<(), String> {
    use gtk::prelude::CssProviderExt;

    // Thread-local rather than Tauri state: GTK types are `!Send`, and this only
    // ever runs on the GTK main thread. Holding the provider is what lets an
    // update reload it in place instead of adding a second one.
    thread_local! {
        static PROVIDER: std::cell::RefCell<Option<gtk::CssProvider>> =
            const { std::cell::RefCell::new(None) };
    }

    let screen = gtk::gdk::Screen::default().ok_or("no screen available")?;
    PROVIDER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let provider = match slot.as_ref() {
            Some(existing) => existing.clone(),
            None => {
                let created = gtk::CssProvider::new();
                gtk::StyleContext::add_provider_for_screen(
                    &screen,
                    &created,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
                slot.replace(created.clone());
                created
            }
        };
        provider
            .load_from_data(css.as_bytes())
            .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_colours_reach_the_stylesheet() {
        for good in ["#fff", "#1F1F28", "#1f1f28ff", "transparent"] {
            assert!(css_color(good).is_ok(), "{good:?} should be accepted");
        }
        for bad in [
            "",
            "red",
            "#12345",
            "#nothex",
            "#fff; } * { background-image: url(x)",
        ] {
            assert!(css_color(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn a_bad_colour_stops_the_whole_sheet() {
        let mut colors = MenuColors {
            surface: "#1F1F28".into(),
            text: "#DCD7BA".into(),
            muted: "#727169".into(),
            accent: "#7E9CD8".into(),
            accent_text: "#1F1F28".into(),
            border: "#363646".into(),
        };
        assert!(stylesheet(&colors).unwrap().contains("#7E9CD8"));
        colors.border = "}".into();
        assert!(stylesheet(&colors).is_err());
    }
}
