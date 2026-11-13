//! Putting things on the canvas from outside it: the image tool (a file
//! from the desktop's picker, in the middle of the view), and Ctrl+V of
//! Excalidraw shapes, plain text or an image.

use crate::exporting::off_thread;
use crate::{images, App, Message, Prompt};
use editor::ImageFile;
use iced::{clipboard, Point, Task};
use std::path::PathBuf;

impl App {
    /// Where the middle of the view is in the drawing.
    fn view_centre(&self) -> [f64; 2] {
        self.tab().view.to_scene(Point::new(self.size.width / 2.0, self.size.height / 2.0))
    }

    /// The image tool (9): picks a file.
    pub fn pick_image(&mut self) -> Task<Message> {
        self.tab_mut().doc.editor.finish_text();
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .add_filter("Image", &images::EXTENSIONS)
                    .pick_file()
                    .await
                    .map(|f| f.path().to_path_buf())
            },
            Message::ImagePicked,
        )
    }

    /// The file is picked: read and prepared on a thread of its own.
    pub fn image_picked(&mut self, path: Option<PathBuf>) -> Task<Message> {
        let Some(path) = path else { return Task::none() };
        let at = self.view_centre();
        self.status = Some("Adding the image…".into());
        Task::perform(
            off_thread(move || {
                use std::io::Read;
                let file = std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let mut bytes = Vec::new();
                file.take(images::MAX_READ + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                if bytes.len() as u64 > images::MAX_READ {
                    return Err("The file is too large to be an image for a drawing.".to_owned());
                }
                images::prepare(bytes)
            }),
            move |r| Message::ImageReady(r.unwrap_or_else(|| Err("reading the image stopped".into())), at, false),
        )
    }

    /// The image is ready: on the canvas, centred on `at`. A paste that
    /// found nothing says so in the status line rather than a dialog.
    pub fn image_ready(&mut self, result: Result<ImageFile, String>, at: [f64; 2], pasted: bool) {
        self.status = None;
        match result {
            Ok(file) => {
                // `initializeImageDimensions`: no taller than about half the view.
                let zoom = f64::from(self.tab().view.zoom);
                let h = f64::from(self.size.height);
                let max = (h - 120.0).max(160.0).min((h * 0.5).floor() / zoom);
                self.tab_mut().doc.editor.insert_image(&file, at, max);
            }
            Err(e) if pasted => self.status = Some(e),
            Err(e) => self.prompt = Some(Prompt::Error(e)),
        }
    }

    /// Ctrl+V: Excalidraw shapes, else text, else an image on the clipboard.
    pub fn paste(&mut self, text: Option<String>) -> Task<Message> {
        let at = self.pointer;
        let text = text.unwrap_or_default();
        let editor = &mut self.tabs[self.active].doc.editor;
        if editor.paste(&text, at) {
            return Task::none();
        }
        if !text.trim().is_empty() {
            // `addTextFromPaste`: half the view wide, within 200 to 800.
            let visible = f64::from(self.size.width / self.tabs[self.active].view.zoom);
            let max = (visible * 0.5).clamp(200.0, 800.0);
            self.tabs[self.active].doc.editor.paste_text(&text, at, max);
            return Task::none();
        }
        Task::perform(
            off_thread(|| {
                let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
                let image = clipboard.get_image().map_err(|_| "There is nothing on the clipboard to paste.".to_owned())?;
                images::from_rgba(image.width as u32, image.height as u32, image.bytes.into_owned())
            }),
            move |r| Message::ImageReady(r.unwrap_or_else(|| Err("the paste stopped".into())), at, true),
        )
    }
}

/// Reads the clipboard's text for a paste.
pub fn read_clipboard() -> Task<Message> {
    clipboard::read().map(Message::Paste)
}
