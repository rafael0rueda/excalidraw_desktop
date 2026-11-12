//! Excalidraw Desktop on Iced — phase 2: editing.
//!
//!   excalidraw-iced drawing.excalidraw
//!   excalidraw-iced --bench 10 drawing.excalidraw
//!
//! Draw with the toolbar or Excalidraw's keys (V R D O A L P, H to pan, Q to
//! keep the tool); select, move, resize, rotate; Ctrl+Z/Y, Ctrl+C/X/V/D,
//! Ctrl+G, Ctrl+[ ], Delete; Ctrl+S saves to the file it was opened from.
//! Pan with the wheel, a middle drag or Space+drag; Ctrl+wheel zooms,
//! Shift+1 fits the drawing.
//!
//! `--bench SECONDS` pans, then zooms, for that long and prints frame timings,
//! memory and start-up time, for the go/no-go gate in docs/iced-port/PLAN.md.
//! `--screenshot out.png [--zoom F]` saves the window once drawn, and exits:
//! the drawing fitted to the window, then zoomed F times about its centre.
//! `--script FILE` replays editor actions first (see script.rs).
//! `NO_MSAA=1` turns antialiasing off, to separate what MSAA does to thin
//! lines (it thickens them; see PLAN.md) from what the geometry is.

mod bench;
mod images;
mod layers;
mod paint;
mod script;
mod toolbar;
mod view;

include!(concat!(env!("OUT_DIR"), "/fonts.rs"));

use editor::{Editor, Tool};
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{column, container, stack, text};
use iced::{clipboard, window, Color, Element, Length, Point, Size, Subscription, Task, Vector};
use layers::Layers;
use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use view::{Input, SceneView, Timings, View};

const WINDOW: Size = Size::new(1280.0, 800.0);

/// A new drawing, as Excalidraw writes an empty one.
const EMPTY: &str = r##"{"type":"excalidraw","version":2,"source":"excalidraw-desktop","elements":[],"appState":{"viewBackgroundColor":"#ffffff"},"files":{}}"##;

#[derive(Clone, Debug)]
struct Args {
    path: Option<String>,
    bench: Option<f32>,
    screenshot: Option<String>,
    zoom: f32,
    script: Option<String>,
}

fn args() -> Args {
    let mut args = Args {
        path: None,
        bench: None,
        screenshot: None,
        zoom: 1.0,
        script: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--bench" => args.bench = it.next().and_then(|s| s.parse().ok()),
            "--screenshot" => args.screenshot = it.next(),
            "--zoom" => args.zoom = it.next().and_then(|s| s.parse().ok()).unwrap_or(1.0),
            "--script" => args.script = it.next(),
            _ => args.path = Some(a),
        }
    }
    args
}

#[derive(Debug, Clone)]
pub enum Message {
    Input(Input),
    Tool(Tool),
    ToggleLock,
    Undo,
    Redo,
    Key(keyboard::Event),
    Paste(Option<String>),
    Resized(Size),
    Tick(Instant),
    Shot(window::Screenshot),
}

struct Document {
    editor: Editor,
    layers: Layers,
    background: Color,
}

pub struct App {
    path: Option<PathBuf>,
    doc: Result<Document, String>,
    view: View,
    size: Size,
    built_at: RefCell<Option<f32>>,
    timings: RefCell<Timings>,
    bench: Option<bench::Bench>,
    launched: Instant,
    load_time: Duration,
    screenshot: Option<String>,
    zoom: f32,
    ticks: u32,
    /// Space held: dragging pans.
    space: bool,
    modifiers: keyboard::Modifiers,
    /// The last pointer position, in the scene: where pastes land.
    pointer: [f64; 2],
    status: Option<String>,
}

fn load(path: &str) -> Result<Document, String> {
    let path = std::path::Path::new(path);
    excalidraw_desktop_core::files::readable(path, excalidraw_desktop_core::files::MAX_TEXT_BYTES)?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let scene = scene::Scene::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(document(scene))
}

fn document(scene: scene::Scene) -> Document {
    // This app's theme owns the canvas colour (PROGRESS.md); until themes
    // arrive in phase 5 the drawing's own is used.
    let css = scene
        .view_background_color()
        .unwrap_or("#ffffff")
        .to_owned();
    let layers = Layers::new(&scene, &css);
    let bg = render::color::parse(&css);
    Document {
        editor: Editor::new(scene),
        layers,
        background: Color::from_rgba(bg.r, bg.g, bg.b, bg.a),
    }
}

/// The view that shows the whole drawing, with a margin.
fn fit(bounds: Option<[f64; 4]>, size: Size) -> View {
    let Some(b) = bounds else {
        return View {
            pan: Vector::new(size.width / 2.0, size.height / 2.0),
            zoom: 1.0,
        };
    };
    let (w, h) = ((b[2] - b[0]) as f32, (b[3] - b[1]) as f32);
    let zoom = ((size.width - 80.0) / w.max(1.0))
        .min((size.height - 80.0) / h.max(1.0))
        .clamp(View::MIN_ZOOM, 1.0);
    let pan = Vector::new(
        (size.width - w * zoom) / 2.0 - b[0] as f32 * zoom,
        (size.height - h * zoom) / 2.0 - b[1] as f32 * zoom,
    );
    View { pan, zoom }
}

/// The editor's own key, for a key press, if it has one.
fn editor_key(key: &Key) -> Option<editor::Key> {
    Some(match key.as_ref() {
        Key::Named(Named::Delete) => editor::Key::Delete,
        Key::Named(Named::Backspace) => editor::Key::Backspace,
        Key::Named(Named::Escape) => editor::Key::Escape,
        Key::Named(Named::ArrowLeft) => editor::Key::Left,
        Key::Named(Named::ArrowRight) => editor::Key::Right,
        Key::Named(Named::ArrowUp) => editor::Key::Up,
        Key::Named(Named::ArrowDown) => editor::Key::Down,
        Key::Character(c) => editor::Key::Char(c.chars().next()?.to_ascii_lowercase()),
        _ => return None,
    })
}

impl App {
    fn boot(args: Args) -> (Self, Task<Message>) {
        let launched = Instant::now();
        let doc = match &args.path {
            Some(path) if std::path::Path::new(path).exists() => load(path),
            // A new drawing, saved to that name with Ctrl+S.
            _ => Ok(document(
                scene::Scene::parse(EMPTY).expect("an empty drawing parses"),
            )),
        };
        let mut doc = doc;
        if let (Some(file), Ok(d)) = (&args.script, &mut doc) {
            match std::fs::read_to_string(file)
                .map_err(|e| e.to_string())
                .and_then(|t| script::run(&mut d.editor, &t))
            {
                Ok(()) => {
                    let changes = d.editor.take_changes();
                    d.layers.apply(&d.editor.scene, changes);
                }
                Err(e) => eprintln!("--script {file}: {e}"),
            }
        }
        let load_time = launched.elapsed();
        let centre = Point::new(WINDOW.width / 2.0, WINDOW.height / 2.0);
        let view = doc.as_ref().map_or(
            View {
                pan: Vector::ZERO,
                zoom: 1.0,
            },
            |d| fit(d.layers.bounds(), WINDOW).zoomed(args.zoom, centre),
        );
        if let Ok(d) = &doc {
            eprintln!(
                "loaded {} elements in {:.1} ms",
                d.layers.prepared.len(),
                load_time.as_secs_f64() * 1e3
            );
            if let Some(b) = d.layers.bounds() {
                // For iced/bench/webkit.py, which shows Excalidraw at the same view.
                eprintln!("bounds {} {} {} {}", b[0], b[1], b[2], b[3]);
            }
        }
        let app = Self {
            path: args.path.map(PathBuf::from),
            doc,
            view,
            size: WINDOW,
            built_at: RefCell::new(None),
            timings: RefCell::new(Timings::default()),
            bench: args.bench.map(|s| bench::Bench::new(s, view)),
            launched,
            load_time,
            screenshot: args.screenshot,
            zoom: args.zoom,
            ticks: 0,
            space: false,
            modifiers: keyboard::Modifiers::default(),
            pointer: [0.0, 0.0],
            status: None,
        };
        (app, Task::none())
    }

    /// Brings the caches up to date after the editor changed something.
    fn sync(&mut self) {
        if let Ok(d) = &mut self.doc {
            let changes = d.editor.take_changes();
            d.layers.apply(&d.editor.scene, changes);
            d.editor.zoom = f64::from(self.view.zoom);
        }
    }

    fn set_view(&mut self, view: View) {
        self.view = view;
        if let Ok(d) = &mut self.doc {
            d.editor.zoom = f64::from(view.zoom);
        }
    }

    fn save(&mut self) {
        let (Some(path), Ok(d)) = (&self.path, &mut self.doc) else {
            return;
        };
        let json = d.editor.scene.to_file_json();
        self.status = Some(
            match excalidraw_desktop_core::files::write_atomic(path, json.as_bytes()) {
                Ok(()) => {
                    d.editor.mark_saved();
                    format!("Saved {}", path.display())
                }
                Err(e) => format!("Could not save: {e}"),
            },
        );
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.sync();
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Input(Input::View(view)) => self.set_view(view),
            Message::Input(input) => {
                let Ok(d) = &mut self.doc else {
                    return Task::none();
                };
                match input {
                    Input::Down(p, m) => d.editor.pointer_down(p, m),
                    Input::Move(p, m) => {
                        self.pointer = p;
                        d.editor.pointer_move(p, m);
                    }
                    Input::Up(p, m) => d.editor.pointer_up(p, m),
                    Input::View(_) => {}
                }
            }
            Message::Tool(tool) => {
                if let Ok(d) = &mut self.doc {
                    d.editor.tool = tool;
                    if tool != Tool::Selection {
                        d.editor.selection.clear();
                    }
                }
            }
            Message::ToggleLock => {
                if let Ok(d) = &mut self.doc {
                    d.editor.tool_locked = !d.editor.tool_locked;
                }
            }
            Message::Undo => {
                if let Ok(d) = &mut self.doc {
                    d.editor.undo();
                }
            }
            Message::Redo => {
                if let Ok(d) = &mut self.doc {
                    d.editor.redo();
                }
            }
            Message::Key(event) => return self.key(event),
            Message::Paste(text) => {
                if let (Some(text), Ok(d)) = (text, &mut self.doc) {
                    if !d.editor.paste(&text, self.pointer) {
                        self.status = Some(
                            "Only Excalidraw shapes can be pasted yet; text comes in phase 3"
                                .into(),
                        );
                    }
                }
            }
            Message::Resized(size) => {
                let first = self.size == WINDOW;
                self.size = size;
                if first {
                    if let Ok(d) = &self.doc {
                        let view = fit(d.layers.bounds(), size)
                            .zoomed(self.zoom, Point::new(size.width / 2.0, size.height / 2.0));
                        self.set_view(view);
                        if let Some(b) = &mut self.bench {
                            b.base = view;
                        }
                    }
                }
            }
            Message::Shot(shot) => {
                if let Some(path) = &self.screenshot {
                    match image::RgbaImage::from_raw(
                        shot.size.width,
                        shot.size.height,
                        shot.rgba.to_vec(),
                    )
                    .map(|img| img.save(path))
                    {
                        Some(Ok(())) => eprintln!("saved {path}"),
                        _ => eprintln!("could not save {path}"),
                    }
                }
                return iced::exit();
            }
            Message::Tick(now) => {
                if self.screenshot.is_some() {
                    self.ticks += 1;
                    // A few frames in, so the window has its size and the fonts are in.
                    if self.ticks == 5 {
                        return window::latest()
                            .and_then(window::screenshot)
                            .map(Message::Shot);
                    }
                    return Task::none();
                }
                let Some(b) = &mut self.bench else {
                    return Task::none();
                };
                let launched = self.launched;
                match b.tick(now, launched, self.size) {
                    Some(view) => self.set_view(view),
                    None => {
                        let elements = self.doc.as_ref().map_or(0, |d| d.layers.prepared.len());
                        b.report(&self.timings.borrow(), elements, self.load_time);
                        return iced::exit();
                    }
                }
            }
        }
        Task::none()
    }

    fn key(&mut self, event: keyboard::Event) -> Task<Message> {
        match event {
            keyboard::Event::ModifiersChanged(m) => self.modifiers = m,
            keyboard::Event::KeyReleased {
                key: Key::Named(Named::Space),
                ..
            } => self.space = false,
            keyboard::Event::KeyPressed { key, modifiers, .. } => {
                let ctrl = modifiers.command();
                match key.as_ref() {
                    Key::Named(Named::Space) => self.space = true,
                    Key::Character("s") if ctrl => self.save(),
                    Key::Character("c") if ctrl => {
                        if let Some(text) = self.doc.as_ref().ok().and_then(|d| d.editor.copy()) {
                            return clipboard::write(text);
                        }
                    }
                    Key::Character("x") if ctrl => {
                        if let Some(text) = self.doc.as_mut().ok().and_then(|d| d.editor.cut()) {
                            return clipboard::write(text);
                        }
                    }
                    Key::Character("v") if ctrl => return clipboard::read().map(Message::Paste),
                    // Shift+1: the whole drawing in view, as in Excalidraw.
                    Key::Character("!" | "1") if modifiers.shift() => {
                        if let Ok(d) = &self.doc {
                            let view = fit(d.layers.bounds(), self.size);
                            self.set_view(view);
                        }
                    }
                    Key::Character("=" | "+") if ctrl => self.set_view(self.view.zoomed(
                        1.25,
                        Point::new(self.size.width / 2.0, self.size.height / 2.0),
                    )),
                    Key::Character("-") if ctrl => self.set_view(self.view.zoomed(
                        0.8,
                        Point::new(self.size.width / 2.0, self.size.height / 2.0),
                    )),
                    _ => {
                        if let (Some(k), Ok(d)) = (editor_key(&key), &mut self.doc) {
                            let m = editor::Modifiers {
                                shift: modifiers.shift(),
                                alt: modifiers.alt(),
                                ctrl,
                            };
                            d.editor.key(k, m);
                        }
                    }
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let d = match &self.doc {
            Ok(d) => d,
            Err(e) => return container(text(e.clone())).center(Length::Fill).into(),
        };
        let scene = SceneView {
            layers: &d.layers,
            editor: &d.editor,
            built_at: &self.built_at,
            view: self.view,
            background: d.background,
            panning: self.space || d.editor.tool == Tool::Hand,
            timings: self.bench.as_ref().map(|_| &self.timings),
            on_input: Box::new(Message::Input),
        };
        if self.bench.is_some() || self.screenshot.is_some() {
            return scene.into();
        }
        let status = text(self.status.clone().unwrap_or_default())
            .size(12)
            .color(Color::from_rgb(0.4, 0.4, 0.4));
        stack![
            Element::from(scene),
            column![toolbar::toolbar(&d.editor), container(status).padding(8)].width(Length::Fill)
        ]
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let resize = window::resize_events().map(|(_, size)| Message::Resized(size));
        if self.bench.is_some() || self.screenshot.is_some() {
            Subscription::batch([resize, window::frames().map(Message::Tick)])
        } else {
            Subscription::batch([resize, keyboard::listen().map(Message::Key)])
        }
    }

    fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or("Untitled".into(), |n| n.to_string_lossy().into_owned());
        let dirty = self.doc.as_ref().is_ok_and(|d| d.editor.is_dirty());
        format!(
            "{}{name} — Excalidraw (Iced)",
            if dirty { "• " } else { "" }
        )
    }
}

/// Puts the bundled fonts straight into iced's font database, borrowed from
/// the binary. `Application::font` copies them onto the heap first, which
/// for Excalifont with its CJK glyphs (22 MB) cost as much memory again;
/// borrowed, only the pages of glyphs actually drawn are ever read in.
fn load_fonts() {
    use iced::advanced::graphics::text::cosmic_text::fontdb;
    let mut system = iced::advanced::graphics::text::font_system()
        .write()
        .expect("the font system lock is not poisoned");
    for (_, bytes) in FONTS {
        let source: std::sync::Arc<dyn AsRef<[u8]> + Send + Sync> = std::sync::Arc::new(*bytes);
        system
            .raw()
            .db_mut()
            .load_font_source(fontdb::Source::Binary(source));
    }
}

fn main() -> iced::Result {
    let args = args();
    let app = iced::application(move || App::boot(args.clone()), App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .window_size(WINDOW)
        .antialiasing(std::env::var_os("NO_MSAA").is_none());
    load_fonts();
    app.run()
}
