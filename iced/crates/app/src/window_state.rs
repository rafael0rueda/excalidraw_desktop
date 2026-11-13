//! The window's size and whether it was maximized, kept between runs in
//! `window.json`, as the Tauri app's window-state plugin keeps them.
//! Position is left to the desktop: Wayland gives apps no say in it.

use excalidraw_desktop_core::{files, store};
use iced::Size;
use serde_json::{json, Value};

const FILE: &str = "window.json";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowState {
    /// The size when not maximized, in logical pixels.
    pub size: Size,
    pub maximized: bool,
}

/// Within reason: a damaged file cannot ask for a window of no size or of
/// a million pixels.
fn sane(w: f64, h: f64) -> Option<Size> {
    let ok = |v: f64, min: f64| v.is_finite() && v >= min && v <= 16_384.0;
    (ok(w, 400.0) && ok(h, 300.0)).then(|| Size::new(w as f32, h as f32))
}

pub fn parse(text: &str) -> Option<WindowState> {
    let v: Value = serde_json::from_str(text).ok()?;
    let size = sane(v.get("width")?.as_f64()?, v.get("height")?.as_f64()?)?;
    Some(WindowState { size, maximized: v.get("maximized").and_then(Value::as_bool).unwrap_or(false) })
}

pub fn load() -> Option<WindowState> {
    parse(&std::fs::read_to_string(store::config_dir().join(FILE)).ok()?)
}

pub fn save(state: WindowState) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&json!({ "width": state.size.width.round(), "height": state.size.height.round(), "maximized": state.maximized })).map_err(|e| e.to_string())?;
    files::write_atomic(&store::config_dir().join(FILE), text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_read_within_reason() {
        assert_eq!(parse(r#"{"width":1200,"height":800,"maximized":true}"#), Some(WindowState { size: Size::new(1200.0, 800.0), maximized: true }));
        assert_eq!(parse(r#"{"width":1200,"height":800}"#).map(|s| s.maximized), Some(false));
        assert_eq!(parse(r#"{"width":10,"height":800}"#), None);
        assert_eq!(parse(r#"{"width":1e9,"height":800}"#), None);
        assert_eq!(parse("not json"), None);
    }
}
