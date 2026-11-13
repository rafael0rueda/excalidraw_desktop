//! The Export menu, as the Tauri app has it (src/lib/export*.ts): PNG of
//! the drawing or the selection, SVG, the image on the clipboard, and three
//! remembered choices (core's `export.json`): transparent background, the
//! drawing embedded in the file, PNG scale.
//!
//! Drawing an image takes a while for a large drawing, so it happens on a
//! thread of its own, from a copy of the scene.

use crate::{App, Message, Prompt};
use excalidraw_desktop_core::settings::{self, ExportPreferences};
use iced::Task;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Png,
    PngSelection,
    Svg,
}

impl Kind {
    fn extension(self) -> &'static str {
        match self {
            Kind::Png | Kind::PngSelection => "png",
            Kind::Svg => "svg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Transparent,
    Embed,
    Scale(u8),
}

/// What one export needs, owned, to take to another thread.
pub(crate) struct Job {
    scene: scene::Scene,
    only: Option<HashSet<String>>,
    canvas: String,
    prefs: ExportPreferences,
}

impl Job {
    fn export(&self) -> export::Export<'_> {
        export::Export {
            scene: &self.scene,
            only: self.only.as_ref(),
            canvas: &self.canvas,
            transparent: self.prefs.transparent,
            font: &crate::paint::export_font,
        }
    }

    fn payload(&self) -> Option<String> {
        self.prefs.embed_scene.then(|| export::payload(&self.scene, self.only.as_ref()))
    }

    pub(crate) fn bytes(&self, kind: Kind) -> Result<Vec<u8>, String> {
        let x = self.export();
        let payload = self.payload();
        match kind {
            Kind::Svg => x.svg(payload.as_deref()).map(String::into_bytes),
            Kind::Png | Kind::PngSelection => x.png(f64::from(self.prefs.scale), payload.as_deref()),
        }
        .map_err(|e| e.to_string())
    }
}

/// Runs `f` on a thread of its own.
pub(crate) fn off_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl std::future::Future<Output = Option<T>> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    async move { rx.await.ok() }
}

/// Kept for as long as the app runs: on X11 the image is served from this
/// process until something else takes the clipboard.
static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

fn copy_image(width: u32, height: u32, rgba: Vec<u8>) -> Result<(), String> {
    let mut slot = CLIPBOARD.lock().map_err(|e| e.to_string())?;
    if slot.is_none() {
        *slot = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
    }
    let clipboard = slot.as_mut().expect("just set");
    clipboard
        .set_image(arboard::ImageData { width: width as usize, height: height as usize, bytes: rgba.into() })
        .map_err(|e| e.to_string())
}

/// `path` with its extension swapped for `ext`.
fn beside(path: &std::path::Path, ext: &str) -> PathBuf {
    path.with_extension(ext)
}

impl App {
    pub fn load_export_preferences(&mut self) {
        if !self.dev {
            self.export_prefs = settings::load_export_preferences();
        }
    }

    pub fn export_choice(&mut self, choice: Choice) {
        let p = &mut self.export_prefs;
        match choice {
            Choice::Transparent => p.transparent = !p.transparent,
            Choice::Embed => p.embed_scene = !p.embed_scene,
            Choice::Scale(s) => p.scale = s.clamp(1, 3),
        }
        if !self.dev {
            if let Err(e) = settings::save_export_preferences(self.export_prefs.clone()) {
                self.status = Some(format!("Could not save the export choices: {e}"));
            }
        }
    }

    /// The drawing (or the selection) as it is now, ready to export.
    pub(crate) fn job(&mut self, selection: bool) -> Result<Job, String> {
        let canvas = self.canvas().unwrap_or_else(|| self.tab().doc.editor.scene.view_background_color().unwrap_or("#ffffff").to_owned());
        let editor = &mut self.tab_mut().doc.editor;
        editor.finish_text();
        let only = if selection {
            let ids = editor.export_selection();
            if ids.is_empty() {
                return Err("Select something to export first.".into());
            }
            Some(ids)
        } else {
            None
        };
        let mut scene = editor.scene.clone();
        // The canvas colour stored inside the image is the one it shows.
        scene.set_view_background_color(&canvas);
        if scene.visible().next().is_none() {
            return Err("Nothing to export.".into());
        }
        Ok(Job { scene, only, canvas, prefs: self.export_prefs.clone() })
    }

    /// Export…: asks where, suggesting a name beside the drawing.
    pub fn export_start(&mut self, kind: Kind) -> Task<Message> {
        if let Err(e) = self.job(kind == Kind::PngSelection) {
            self.prompt = Some(Prompt::Error(e));
            return Task::none();
        }
        let ext = kind.extension();
        let suggested = self.tab().path.as_deref().map(|p| beside(p, ext));
        let name = suggested
            .as_deref()
            .and_then(|p| p.file_name())
            .map_or_else(|| format!("Untitled.{ext}"), |n| n.to_string_lossy().into_owned());
        let dir = suggested.as_deref().and_then(|p| p.parent()).map(PathBuf::from);
        let filter = if ext == "png" { "PNG image" } else { "SVG image" };
        Task::perform(
            async move {
                let mut dialog = rfd::AsyncFileDialog::new().add_filter(filter, &[ext]).set_file_name(name);
                if let Some(dir) = dir {
                    dialog = dialog.set_directory(dir);
                }
                dialog.save_file().await.map(|f| f.path().to_path_buf())
            },
            move |path| Message::ExportTo(kind, path),
        )
    }

    /// The place is picked: draws the image and writes it.
    pub fn export_to(&mut self, kind: Kind, path: Option<PathBuf>) -> Task<Message> {
        let Some(mut path) = path else { return Task::none() };
        let ext = kind.extension();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            path.as_mut_os_string().push(format!(".{ext}"));
        }
        let job = match self.job(kind == Kind::PngSelection) {
            Ok(job) => job,
            Err(e) => {
                self.prompt = Some(Prompt::Error(e));
                return Task::none();
            }
        };
        self.status = Some("Exporting…".into());
        Task::perform(
            off_thread(move || {
                let bytes = job.bytes(kind)?;
                excalidraw_desktop_core::files::write_atomic(&path, &bytes)?;
                Ok(format!("Exported {}", path.display()))
            }),
            |r| Message::Exported(r.unwrap_or_else(|| Err("the export stopped".into()))),
        )
    }

    /// Copy image to clipboard: the whole drawing, at the PNG scale, never
    /// with the drawing inside (an image pasted elsewhere has no use for it).
    pub fn copy_image(&mut self) -> Task<Message> {
        let job = match self.job(false) {
            Ok(job) => job,
            Err(e) => {
                self.prompt = Some(Prompt::Error(e));
                return Task::none();
            }
        };
        Task::perform(
            off_thread(move || {
                let (w, h, rgba) = job.export().rgba(f64::from(job.prefs.scale)).map_err(|e| e.to_string())?;
                copy_image(w, h, rgba)?;
                Ok("Copied the image".to_owned())
            }),
            |r| Message::Exported(r.unwrap_or_else(|| Err("the copy stopped".into()))),
        )
    }

    pub fn exported(&mut self, result: Result<String, String>) {
        match result {
            Ok(done) => self.status = Some(done),
            Err(e) => {
                self.status = None;
                self.prompt = Some(Prompt::Error(format!("Export failed: {e}")));
            }
        }
    }
}

/// `--export OUT.png|OUT.svg DRAWING`: the drawing exported with the saved
/// choices' defaults (or `--theme`'s canvas), without a window. For
/// comparing with Excalidraw's own exports.
pub fn export_cli(out: &str, drawing: Option<&std::path::Path>, theme: Option<&str>) -> Result<(), String> {
    let path = drawing.ok_or("--export needs a drawing")?;
    let scene = crate::doc::read(path)?;
    let canvas = match theme {
        Some(id) => crate::theme::Themes::fixed(id).chosen().colors.canvas,
        None => scene.view_background_color().unwrap_or("#ffffff").to_owned(),
    };
    let transparent = std::env::var_os("TRANSPARENT").is_some();
    let scale = std::env::var("SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(2u8);
    let embed = std::env::var_os("EMBED").is_some();
    let job = Job { scene, only: None, canvas, prefs: ExportPreferences { scale, transparent, embed_scene: embed } };
    let kind = if out.ends_with(".svg") { Kind::Svg } else { Kind::Png };
    let started = std::time::Instant::now();
    let bytes = job.bytes(kind)?;
    std::fs::write(out, &bytes).map_err(|e| e.to_string())?;
    eprintln!("wrote {out}: {} bytes in {:.0} ms", bytes.len(), started.elapsed().as_secs_f64() * 1e3);
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Run by hand on a desktop (`cargo test -p excalidraw-iced -- --ignored
    /// clipboard`): an image copied here is what another app pastes.
    /// Needs `wl-paste` (wl-clipboard).
    #[test]
    #[ignore]
    fn clipboard_image_reaches_the_desktop() {
        let (w, h) = (4u32, 3u32);
        let rgba: Vec<u8> = (0..w * h).flat_map(|_| [255u8, 0, 0, 255]).collect();
        super::copy_image(w, h, rgba).unwrap();
        // Let the X11 bridge see the new owner.
        std::thread::sleep(std::time::Duration::from_millis(300));
        let types = std::process::Command::new("wl-paste").arg("--list-types").output().unwrap();
        let types = String::from_utf8_lossy(&types.stdout);
        assert!(types.contains("image/png"), "offered: {types}");
        let png = std::process::Command::new("wl-paste").args(["--type", "image/png"]).output().unwrap().stdout;
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (w, h));
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0, 255]);
    }
}
