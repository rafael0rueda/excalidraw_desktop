//! Things put on the canvas from outside: an image (from a file or the
//! clipboard), and plain text pasted (Excalidraw's `initializeImage`,
//! `initializeImageDimensions`, `newImageElement`, `addTextFromPaste`).

use crate::{text, Editor, Tool};
use rough::Point;
use scene::edit::{new_element, new_text_element, now_ms};
use serde_json::{json, Value};

/// An image file ready to go in a drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageFile {
    /// Excalidraw's: the SHA-1 of the file as it was picked, in hex.
    pub id: String,
    pub mime: String,
    pub data_url: String,
    /// Its own size, in pixels.
    pub width: f64,
    pub height: f64,
}

/// `addTextFromPaste`'s gap between pasted lines.
const LINE_GAP: f64 = 10.0;

impl Editor {
    /// Puts `file` in the drawing, centred on `at`, no taller than
    /// `max_height` (`initializeImageDimensions`: its natural size, kept
    /// within the view), selected. One undo step.
    pub fn insert_image(&mut self, file: &ImageFile, at: Point, max_height: f64) -> Option<String> {
        self.finish_text();
        self.scene.add_file(
            &file.id,
            json!({
                "mimeType": file.mime,
                "id": file.id,
                "dataURL": file.data_url,
                "created": now_ms(),
                "lastRetrieved": now_ms(),
            }),
        );
        let height = file.height.min(max_height).max(1.0);
        let width = height * (file.width / file.height.max(1.0));
        let mut raw = new_element("image", at[0] - width / 2.0, at[1] - height / 2.0, &self.style);
        raw.insert("width".into(), scene::edit::num(width));
        raw.insert("height".into(), scene::edit::num(height));
        raw.insert("roundness".into(), Value::Null);
        // `newImageElement`: no stroke, then its own fields.
        raw.insert("strokeColor".into(), json!("transparent"));
        raw.insert("status".into(), json!("pending"));
        raw.insert("fileId".into(), json!(file.id));
        raw.insert("scale".into(), json!([1, 1]));
        raw.insert("crop".into(), Value::Null);
        self.begin();
        let id = self.add(self.scene.elements.len(), raw);
        self.commit();
        self.tool = Tool::Selection;
        self.selection = id.iter().cloned().collect();
        id
    }

    /// Plain text pasted at `at`: a text element per non-empty line, each
    /// centred on `at` and below the last, wrapped at `max_width` if longer.
    /// Returns false if there was nothing to paste.
    pub fn paste_text(&mut self, pasted: &str, at: Point, max_width: f64) -> bool {
        self.finish_text();
        let lines: Vec<&str> = pasted.split('\n').collect();
        let line_height = scene::default_line_height(self.font_family);
        let font = text::Font {
            family: self.font_family,
            size: self.font_size,
            line_height,
        };
        let mut y = at[1];
        let mut made = Vec::new();
        self.begin();
        for (i, line) in lines.iter().enumerate() {
            let original = text::normalize(line).trim().to_owned();
            if original.is_empty() {
                if lines.get(i.wrapping_sub(1)).is_some_and(|p| !p.trim().is_empty()) {
                    y += self.font_size * line_height + LINE_GAP;
                }
                continue;
            }
            let (w, _) = text::measure(&original, font, self.measure.as_ref());
            let unwrapped = w > max_width;
            let shown = if unwrapped {
                text::wrap(&original, font, max_width, self.measure.as_ref())
            } else {
                original.clone()
            };
            let (w, h) = text::measure(&shown, font, self.measure.as_ref());
            let mut raw = new_text_element(
                at[0] - w / 2.0,
                y - h / 2.0,
                &self.style,
                self.font_family,
                self.font_size,
                "left",
                "top",
                None,
            );
            raw.insert("text".into(), json!(shown));
            raw.insert("originalText".into(), json!(original));
            raw.insert("autoResize".into(), json!(!unwrapped));
            raw.insert("width".into(), scene::edit::num(w));
            raw.insert("height".into(), scene::edit::num(h));
            if let Some(id) = self.add(self.scene.elements.len(), raw) {
                made.push(id);
            }
            y += h + LINE_GAP;
        }
        self.commit();
        if made.is_empty() {
            return false;
        }
        self.tool = Tool::Selection;
        self.selection = made;
        true
    }
}
