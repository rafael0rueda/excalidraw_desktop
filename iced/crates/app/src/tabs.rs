//! A tab: one drawing, where it is on disk, and where it is in view.

use crate::doc::Document;
use crate::view::View;
use iced::widget::text_editor;
use std::cell::RefCell;
use std::path::PathBuf;

pub struct Tab {
    /// Names its autosave snapshot, so it is a `safe_id`: lower-case letters,
    /// digits and dashes.
    pub id: String,
    pub path: Option<PathBuf>,
    pub doc: Document,
    /// Each tab keeps its own viewport.
    pub view: View,
    /// The zoom bucket its caches were built at.
    pub built_at: RefCell<Option<f32>>,
    /// The text box over the text being typed: (element id, its contents).
    pub text_box: Option<(String, text_editor::Content)>,
    /// Changed since its last snapshot.
    pub unsnapshotted: bool,
    /// The theme stroke and fill last given to its new elements.
    pub themed: Option<(String, String)>,
}

/// A fresh tab id: 32 hex digits, as `safe_id` accepts.
pub fn new_id() -> String {
    let a = scene::edit::random_integer() as u64;
    let b = scene::edit::random_integer() as u64;
    let c = scene::edit::now_ms() as u64;
    format!("{:08x}{:08x}{:016x}", a & 0xffff_ffff, b & 0xffff_ffff, c ^ (a << 32))
}

impl Tab {
    pub fn new(id: String, path: Option<PathBuf>, doc: Document, view: View) -> Self {
        Self { id, path, doc, view, built_at: RefCell::new(None), text_box: None, unsnapshotted: true, themed: None }
    }

    /// The file's name, or what an unsaved drawing is called.
    pub fn title(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_name()).map_or("Untitled".into(), |n| n.to_string_lossy().into_owned())
    }

    pub fn dirty(&self) -> bool {
        self.doc.editor.is_dirty()
    }
}
