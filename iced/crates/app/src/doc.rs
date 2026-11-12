//! One open drawing: the editor and its caches.

use crate::layers::Layers;
use crate::view::View;
use editor::Editor;
use iced::{Color, Size, Vector};
use std::path::Path;

/// A new drawing, as Excalidraw writes an empty one.
pub const EMPTY: &str = r##"{"type":"excalidraw","version":2,"source":"excalidraw-desktop","elements":[],"appState":{"viewBackgroundColor":"#ffffff"},"files":{}}"##;

pub struct Document {
    pub editor: Editor,
    pub layers: Layers,
    pub background: Color,
}

impl Document {
    pub fn new(scene: scene::Scene) -> Self {
        // This app's theme owns the canvas colour (PROGRESS.md); until themes
        // arrive in phase 5 the drawing's own is used.
        let css = scene.view_background_color().unwrap_or("#ffffff").to_owned();
        let layers = Layers::new(&scene, &css);
        let mut editor = Editor::new(scene);
        editor.set_measure(Box::new(crate::measure::Shaped));
        let bg = render::color::parse(&css);
        Self { editor, layers, background: Color::from_rgba(bg.r, bg.g, bg.b, bg.a) }
    }

    pub fn empty() -> Self {
        Self::new(scene::Scene::parse(EMPTY).expect("the empty drawing parses"))
    }

    /// Nothing drawn and nothing to save: a tab Open may reuse.
    pub fn is_blank(&self) -> bool {
        !self.editor.is_dirty() && self.editor.scene.visible().next().is_none()
    }
}

/// Reads a drawing, refusing what is not a regular file of a sane size.
pub fn read(path: &Path) -> Result<scene::Scene, String> {
    excalidraw_desktop_core::files::readable(path, excalidraw_desktop_core::files::MAX_TEXT_BYTES)?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    scene::Scene::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The view that shows the whole drawing, with a margin.
pub fn fit(bounds: Option<[f64; 4]>, size: Size) -> View {
    let Some(b) = bounds else {
        return View { pan: Vector::new(size.width / 2.0, size.height / 2.0), zoom: 1.0 };
    };
    let (w, h) = ((b[2] - b[0]) as f32, (b[3] - b[1]) as f32);
    let zoom = ((size.width - 80.0) / w.max(1.0)).min((size.height - 80.0) / h.max(1.0)).clamp(View::MIN_ZOOM, 1.0);
    let pan = Vector::new((size.width - w * zoom) / 2.0 - b[0] as f32 * zoom, (size.height - h * zoom) / 2.0 - b[1] as f32 * zoom);
    View { pan, zoom }
}
