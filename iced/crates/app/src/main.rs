//! Excalidraw Desktop on Iced (docs/iced-port/PLAN.md).
//!
//!   excalidraw-iced [drawing.excalidraw…]
//!
//! Tabs (Ctrl+N, Ctrl+W, Ctrl+Tab, Ctrl+1…9), files (Ctrl+O, Ctrl+S,
//! Ctrl+Shift+S, the File menu with recent drawings), autosave and crash
//! recovery, and one running copy: a second launch hands its drawings over.
//! Draw with the tool bar or Excalidraw's keys; Ctrl+Q quits. Themes: the
//! Theme menu, and the theme editor (Ctrl+,), as the Tauri app has them.
//!
//! Until it replaces the Tauri app, this build keeps its state apart, in
//! `~/.config/excalidraw-desktop-iced`, so it cannot touch the real session.
//!
//! Development modes, which leave all of that alone and work on one drawing:
//! `--bench SECONDS` pans, then zooms, and prints timings (the phase 1 gate).
//! `--screenshot out.png [--zoom F]` saves the window once drawn, and exits.
//! `--chrome` keeps the tab bar and tools in that screenshot.
//! `--theme ID` draws in that theme (the light one otherwise, with the
//! drawing's own canvas colour, as Excalidraw shows it).
//! `--script FILE` replays editor actions first (see script.rs).
//! `--export OUT.png|OUT.svg DRAWING` exports without a window (`SCALE=`,
//! `TRANSPARENT=1`, `EMBED=1`; `--theme` for its canvas colour).
//! `NO_MSAA=1` turns antialiasing off, to separate what MSAA does to thin
//! lines (it thickens them; see PLAN.md) from what the geometry is.

mod bench;
mod chrome;
mod doc;
mod exporting;
mod icons;
mod images;
mod instance;
mod layers;
mod library;
mod measure;
mod paint;
mod properties;
mod script;
mod session;
mod tabs;
#[cfg(test)]
mod tests;
mod theme;
mod theme_editor;
mod toolbar;
mod view;

include!(concat!(env!("OUT_DIR"), "/fonts.rs"));

use chrome::{Answer, Menu, TAB_BAR};
use doc::{fit, Document};
use editor::Tool;
use excalidraw_desktop_core::recent::{self, RecentEntry};
use excalidraw_desktop_core::scope::Allowed;
use excalidraw_desktop_core::session::Session;
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{column, container, opaque, stack, text, text_editor};
use iced::{clipboard, window, Color, Element, Length, Point, Size, Subscription, Task};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tabs::Tab;
use view::{Input, SceneView, Timings, View};

const WINDOW: Size = Size::new(1280.0, 800.0);

/// This build's config directory, apart from the Tauri app's.
const APP_DIR: &str = "excalidraw-desktop-iced";
/// The Wayland app id and X11 class: the installed `.desktop` file's name,
/// which is how the desktop finds the window's icon and name.
const APP_ID: &str = "excalidraw-desktop-iced";

#[derive(Clone, Debug, Default)]
struct Args {
    paths: Vec<PathBuf>,
    bench: Option<f32>,
    screenshot: Option<String>,
    zoom: Option<f32>,
    script: Option<String>,
    /// With `--screenshot`: the window as it is, tab bar and tools included.
    chrome: bool,
    /// A development run's theme.
    theme: Option<String>,
    /// The desktop prefers dark, as it said before the window opened.
    system_dark: bool,
    /// `--export OUT`: export the drawing and exit, no window.
    export: Option<String>,
}

impl Args {
    /// A development run: one drawing, no session, no single-instance lock.
    fn dev(&self) -> bool {
        self.bench.is_some() || self.screenshot.is_some() || self.script.is_some()
    }
}

/// The command line. A name that is not valid UTF-8 is dropped with a word,
/// rather than the panic `std::env::args` gives (review 2026-11-04, BE-1).
fn args() -> Args {
    let mut args = Args::default();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let mut it = std::env::args_os().skip(1);
    while let Some(a) = it.next() {
        let Some(a) = a.to_str().map(str::to_owned) else {
            eprintln!("excalidraw-iced: not opening {}: the name is not valid UTF-8", Path::new(&a).display());
            continue;
        };
        let mut value = || it.next().and_then(|v| v.into_string().ok());
        match a.as_str() {
            "--bench" => args.bench = value().and_then(|s| s.parse().ok()),
            "--screenshot" => args.screenshot = value(),
            "--zoom" => args.zoom = value().and_then(|s| s.parse().ok()),
            "--script" => args.script = value(),
            "--chrome" => args.chrome = true,
            "--theme" => args.theme = value(),
            "--export" => args.export = value(),
            flag if flag.starts_with('-') => {}
            _ => {
                let p = PathBuf::from(&a);
                args.paths.push(if p.is_absolute() { p } else { cwd.join(p) });
            }
        }
    }
    args
}

/// What happens once an unsaved-changes question is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    Close,
    Quit,
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
    TextAction(text_editor::Action),
    TextDone,
    SelectTab(usize),
    CloseTab(String),
    NewTab,
    Open,
    Opened(Vec<PathBuf>),
    OpenPath(PathBuf),
    Save,
    SaveAs,
    SaveAsPicked(String, Option<PathBuf>, Option<After>),
    ClearRecent,
    ToggleMenu(Menu),
    Quit,
    Answer(Answer),
    Autosave(Instant),
    HandedOver(Vec<PathBuf>),
    /// From the Theme menu: a theme id, or `theme::SYSTEM`.
    SelectTheme(String),
    Theme(theme_editor::Edit),
    ReloadThemes,
    /// The desktop switched between light and dark.
    SystemDark(bool),
    Export(exporting::Kind),
    ExportTo(exporting::Kind, Option<PathBuf>),
    /// An export or copy finished: what to say, or what went wrong.
    Exported(Result<String, String>),
    CopyImage,
    ExportChoice(exporting::Choice),
    Library(library::Msg),
    Props(properties::Msg),
}

/// A question the app is waiting on.
pub enum Prompt {
    Unsaved { tab: String, title: String, then: After },
    Recover { what: String, session: Session },
    Error(String),
    /// Something to read, not a problem: what Reload user themes found.
    Info(String),
    /// Leaving a theme draft unsaved; `then` is what was asked for.
    DiscardTheme { name: String, then: theme_editor::Edit },
    DeleteTheme { name: String },
    RemoveLibraryItem { id: String, name: String },
}

/// A quit in progress: which tabs have been asked about, and which of them
/// the answer was to discard.
#[derive(Default)]
struct Quitting {
    asked: HashSet<String>,
    discarded: HashSet<String>,
}

pub struct App {
    tabs: Vec<Tab>,
    active: usize,
    /// Paths this app may read and write: picked in a dialog, named on the
    /// command line, or from its own lists. Recent files and the session
    /// only ever hold these (core's rule, shared with the Tauri app).
    allowed: Allowed,
    /// The drawing's area: the window less the tab bar, if shown.
    size: Size,
    /// The tab bar's height, or nothing when it is not shown.
    bar: f32,
    /// Whether the window has said its real size yet.
    sized: bool,
    recent: Vec<RecentEntry>,
    prompt: Option<Prompt>,
    menu: Option<Menu>,
    themes: theme::Themes,
    /// The theme editor, when open.
    panel: Option<theme_editor::Panel>,
    /// A development run without `--theme`: each drawing on its own canvas
    /// colour, as Excalidraw itself shows it, for comparing the two.
    file_background: bool,
    quitting: Option<Quitting>,
    /// Drawings named on the command line, opened once recovery is settled.
    pending_open: Vec<PathBuf>,
    /// When drawing last changed anything, for autosave.
    last_change: Option<Instant>,
    dev: bool,
    timings: std::cell::RefCell<Timings>,
    bench: Option<bench::Bench>,
    launched: Instant,
    load_time: Duration,
    screenshot: Option<String>,
    chrome: bool,
    zoom: f32,
    ticks: u32,
    /// Space held: dragging pans.
    space: bool,
    /// The last pointer position, in the scene: where pastes land.
    pointer: [f64; 2],
    status: Option<String>,
    export_prefs: excalidraw_desktop_core::settings::ExportPreferences,
    library: library::Library,
    /// The properties panel's open popup (a palette, the arrowheads, the
    /// fonts), and its hex field as typed.
    popup: Option<properties::Popup>,
    hex: String,
}

/// The text box's widget id, to focus it when typing starts.
const TEXT_BOX: &str = "text-box";

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
        Key::Named(Named::Enter) => editor::Key::Enter,
        Key::Character(c) => editor::Key::Char(c.chars().next()?.to_ascii_lowercase()),
        _ => return None,
    })
}

/// Saving writes back only to an `.excalidraw` file; anything else (a
/// drawing opened from `.json`) is saved under a new name instead.
fn saves_in_place(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("excalidraw"))
}

impl App {
    fn boot(args: Args) -> (Self, Task<Message>) {
        let launched = Instant::now();
        let bar = if args.bench.is_some() || (args.screenshot.is_some() && !args.chrome) { 0.0 } else { TAB_BAR };
        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            allowed: Allowed::default(),
            size: Size::new(WINDOW.width, WINDOW.height - bar),
            bar,
            sized: false,
            recent: Vec::new(),
            prompt: None,
            menu: None,
            themes: if args.dev() { theme::Themes::fixed(args.theme.as_deref().unwrap_or(theme::FALLBACK)) } else { theme::Themes::load(args.system_dark) },
            panel: None,
            file_background: args.dev() && args.theme.is_none(),
            quitting: None,
            pending_open: Vec::new(),
            last_change: None,
            dev: args.dev(),
            timings: Default::default(),
            bench: None,
            launched,
            load_time: Duration::ZERO,
            screenshot: args.screenshot.clone(),
            chrome: args.chrome,
            zoom: args.zoom.unwrap_or(1.0),
            ticks: 0,
            space: false,
            pointer: [0.0, 0.0],
            status: None,
            export_prefs: Default::default(),
            library: library::Library::load(false).0,
            popup: None,
            hex: String::new(),
        };
        if app.dev {
            app.boot_dev(&args);
        } else {
            app.boot_session(&args);
            app.load_export_preferences();
            let (library, problem) = library::Library::load(true);
            app.library = library;
            if let (Some(p), None) = (problem, &app.prompt) {
                app.prompt = Some(Prompt::Error(p));
            }
        }
        let task = app.sync();
        (app, task)
    }

    /// One drawing, edited by `--script`, measured by `--bench`.
    fn boot_dev(&mut self, args: &Args) {
        let path = args.paths.first().cloned();
        let mut d = match &path {
            Some(p) if p.exists() => match doc::read(p) {
                Ok(scene) => Document::new(scene),
                Err(e) => {
                    eprintln!("{e}");
                    Document::empty()
                }
            },
            _ => Document::empty(),
        };
        if let Some(file) = &args.script {
            match std::fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|t| script::run(&mut d.editor, &t)) {
                Ok(()) => {
                    let changes = d.editor.take_changes();
                    d.layers.apply(&d.editor.scene, changes);
                }
                Err(e) => eprintln!("--script {file}: {e}"),
            }
            // `popup stroke|background|start|end|fonts`: open in the screenshot.
            let text = std::fs::read_to_string(file).unwrap_or_default();
            if let Some(last) = text.lines().rev().find_map(|l| l.trim().strip_prefix("popup ")) {
                use properties::Popup;
                self.popup = match last.trim() {
                    "stroke" => Some(Popup::Stroke),
                    "background" => Some(Popup::Background),
                    "start" => Some(Popup::StartHead),
                    "end" => Some(Popup::EndHead),
                    "fonts" => Some(Popup::Fonts),
                    _ => None,
                };
                self.hex = "e03131".into();
            }
        }
        self.load_time = self.launched.elapsed();
        let centre = Point::new(self.size.width / 2.0, self.size.height / 2.0);
        let view = fit(d.layers.bounds(), self.size).zoomed(self.zoom, centre);
        eprintln!("loaded {} elements in {:.1} ms", d.layers.prepared.len(), self.load_time.as_secs_f64() * 1e3);
        if let Some(b) = d.layers.bounds() {
            // For iced/bench/webkit.py, which shows Excalidraw at the same view.
            eprintln!("bounds {} {} {} {}", b[0], b[1], b[2], b[3]);
        }
        self.bench = args.bench.map(|s| bench::Bench::new(s, view));
        self.tabs.push(Tab::new(tabs::new_id(), path, d, view));
    }

    /// The last session back, and the drawings named on the command line.
    fn boot_session(&mut self, args: &Args) {
        for p in &args.paths {
            self.allowed.allow(p);
        }
        self.pending_open = args.paths.iter().filter(|p| p.is_file()).cloned().collect();
        match session::load(self) {
            session::Restored::Tabs(tabs, active) => {
                self.tabs = tabs;
                self.active = active.and_then(|a| self.tabs.iter().position(|t| t.id == a)).unwrap_or(0);
                self.after_restore();
            }
            session::Restored::Ask(s) => {
                self.prompt = Some(Prompt::Recover { what: session::describe(&s), session: s });
                self.tabs.push(Tab::new(tabs::new_id(), None, Document::empty(), fit(None, self.size)));
            }
        }
        self.recent = recent::list(&self.allowed);
    }

    /// Once the session is settled: the command line's drawings, a blank tab
    /// if there is nothing else, and a first snapshot to mark the run open.
    fn after_restore(&mut self) {
        let pending = std::mem::take(&mut self.pending_open);
        self.open_paths(pending);
        if self.tabs.is_empty() {
            self.tabs.push(Tab::new(tabs::new_id(), None, Document::empty(), fit(None, self.size)));
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.snapshot();
    }

    fn snapshot(&mut self) {
        if self.dev {
            return;
        }
        let discarded = self.quitting.as_ref().map(|q| q.discarded.clone()).unwrap_or_default();
        if let Err(e) = session::snapshot(self, &discarded) {
            self.status = Some(format!("Autosave failed: {e}"));
        }
    }

    /// The theme on screen: the editor's draft while it is open.
    fn active_theme(&self) -> theme::Theme {
        match &self.panel {
            Some(p) => p.paintable(),
            None => self.themes.chosen(),
        }
    }

    /// The canvas colour on screen, as CSS.
    fn look_canvas_css(&self) -> String {
        self.active_theme().colors.canvas
    }

    pub fn look(&self) -> theme::Look {
        theme::Look::of(&self.active_theme())
    }

    fn theme(&self) -> iced::Theme {
        let t = self.active_theme();
        theme::Look::of(&t).iced(&t.name)
    }

    /// The canvas colour a drawing is saved with: the chosen theme's, as
    /// the Tauri app writes it (never a draft's).
    pub fn canvas(&self) -> Option<String> {
        (!self.dev).then(|| self.themes.chosen().colors.canvas)
    }

    fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// Brings the caches and the text box up to date with the editor.
    fn sync(&mut self) -> Task<Message> {
        let dev = self.dev;
        let tab = &mut self.tabs[self.active];
        let changes = tab.doc.editor.take_changes();
        if !changes.ids.is_empty() || changes.order {
            tab.unsnapshotted = true;
            if !dev {
                self.last_change = Some(Instant::now());
            }
        }
        // New elements draw in the theme's colours, unless the user picked
        // their own: a default still at Excalidraw's own or at the last
        // theme's is the theme's to change (apply.ts).
        let t = self.active_theme();
        let tab = &mut self.tabs[self.active];
        let wanted = (t.colors.stroke, t.colors.fill);
        if tab.themed.as_ref() != Some(&wanted) {
            let last = tab.themed.take();
            let style = &mut tab.doc.editor.style;
            if style.stroke_color == theme::FACTORY_STROKE || last.as_ref().is_none_or(|l| l.0 == style.stroke_color) {
                style.stroke_color = wanted.0.clone();
            }
            if style.background_color == theme::FACTORY_FILL || last.as_ref().is_none_or(|l| l.1 == style.background_color) {
                style.background_color = wanted.1.clone();
            }
            tab.themed = Some(wanted);
        }
        tab.doc.layers.apply(&tab.doc.editor.scene, changes);
        tab.doc.editor.zoom = f64::from(tab.view.zoom);
        let editing = tab.doc.editor.editing_text().map(|(e, typed)| (e.id.clone(), typed));
        match editing {
            Some((id, typed)) if tab.text_box.as_ref().is_none_or(|(box_id, _)| *box_id != id) => {
                let mut content = text_editor::Content::with_text(&typed);
                content.perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
                tab.text_box = Some((id, content));
                iced::widget::operation::focus(TEXT_BOX)
            }
            Some(_) => Task::none(),
            None => {
                tab.text_box = None;
                Task::none()
            }
        }
    }

    fn set_view(&mut self, view: View) {
        let tab = self.tab_mut();
        tab.view = view;
        tab.doc.editor.zoom = f64::from(view.zoom);
    }

    fn select_tab(&mut self, i: usize) {
        if i < self.tabs.len() && i != self.active {
            // Typing ends with the tab it was in.
            self.tab_mut().doc.editor.finish_text();
            let _ = self.sync();
            self.active = i;
            self.snapshot();
        }
    }

    fn new_tab(&mut self) {
        self.tabs.push(Tab::new(tabs::new_id(), None, Document::empty(), fit(None, self.size)));
        self.active = self.tabs.len() - 1;
        self.snapshot();
    }

    // --- files

    /// Opens each drawing: brought forward if already open, into the
    /// current tab if that is blank and untitled, else into a new one.
    fn open_paths(&mut self, paths: Vec<PathBuf>) {
        let mut problems = Vec::new();
        for p in paths {
            let p = std::fs::canonicalize(&p).unwrap_or(p);
            if let Some(i) = self.tabs.iter().position(|t| t.path.as_deref() == Some(p.as_path())) {
                self.active = i;
                continue;
            }
            match doc::read(&p) {
                Ok(scene) => {
                    self.allowed.allow(&p);
                    let d = Document::new(scene);
                    let view = fit(d.layers.bounds(), self.size);
                    let tab = Tab::new(tabs::new_id(), Some(p.clone()), d, view);
                    let reuse = self.tabs.get(self.active).is_some_and(|t| t.path.is_none() && t.doc.is_blank());
                    if reuse {
                        self.tabs[self.active] = tab;
                    } else {
                        self.tabs.push(tab);
                        self.active = self.tabs.len() - 1;
                    }
                    let _ = recent::push(&self.allowed, p.to_string_lossy().into_owned());
                }
                Err(e) => problems.push(e),
            }
        }
        self.recent = recent::list(&self.allowed);
        if !problems.is_empty() {
            self.prompt = Some(Prompt::Error(format!("Could not open:\n{}", problems.join("\n"))));
        }
        if !self.tabs.is_empty() {
            self.snapshot();
        }
    }

    fn pick_open() -> Task<Message> {
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .add_filter("Excalidraw drawing", &["excalidraw", "json"])
                    .pick_files()
                    .await
                    .map(|files| files.into_iter().map(|f| f.path().to_path_buf()).collect())
                    .unwrap_or_default()
            },
            Message::Opened,
        )
    }

    fn pick_save(&self, id: String, then: Option<After>) -> Task<Message> {
        let name = self.position(&id).map(|i| self.tabs[i].title()).unwrap_or_default();
        let name = if name == "Untitled" { "drawing.excalidraw".to_owned() } else { name };
        Task::perform(
            async move {
                rfd::AsyncFileDialog::new()
                    .add_filter("Excalidraw drawing", &["excalidraw"])
                    .set_file_name(name)
                    .save_file()
                    .await
                    .map(|f| f.path().to_path_buf())
            },
            move |path| Message::SaveAsPicked(id.clone(), path, then),
        )
    }

    /// Writes tab `id` to `path`, atomically, as Excalidraw saves a file.
    fn write_to(&mut self, id: &str, path: PathBuf) -> Result<(), String> {
        let Some(i) = self.position(id) else { return Err("the tab is gone".into()) };
        if let Some(canvas) = self.canvas() {
            self.tabs[i].doc.editor.scene.set_view_background_color(&canvas);
        }
        let json = self.tabs[i].doc.editor.scene.to_file_json();
        excalidraw_desktop_core::files::write_atomic(&path, json.as_bytes())?;
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        self.allowed.allow(&path);
        let tab = &mut self.tabs[i];
        tab.doc.editor.mark_saved();
        tab.path = Some(path.clone());
        tab.unsnapshotted = true;
        self.status = Some(format!("Saved {}", path.display()));
        let _ = recent::push(&self.allowed, path.to_string_lossy().into_owned());
        self.recent = recent::list(&self.allowed);
        self.snapshot();
        Ok(())
    }

    /// Saves tab `id`: in place if it has an `.excalidraw` file, else by
    /// asking where. `then` continues a close or quit once it is saved.
    fn save_tab(&mut self, id: String, then: Option<After>) -> Task<Message> {
        let Some(i) = self.position(&id) else { return Task::none() };
        self.tabs[i].doc.editor.finish_text();
        match self.tabs[i].path.clone().filter(|p| saves_in_place(p)) {
            Some(path) => match self.write_to(&id, path) {
                Ok(()) => then.map_or(Task::none(), |then| self.continue_after(&id, then, false)),
                Err(e) => {
                    self.quitting = None;
                    self.prompt = Some(Prompt::Error(format!("Could not save: {e}")));
                    Task::none()
                }
            },
            None => self.pick_save(id, then),
        }
    }

    fn save_as_picked(&mut self, id: String, path: Option<PathBuf>, then: Option<After>) -> Task<Message> {
        let Some(mut path) = path else {
            // The dialog was cancelled: so is whatever was waiting on it.
            self.quitting = None;
            return Task::none();
        };
        if !saves_in_place(&path) {
            path.as_mut_os_string().push(".excalidraw");
        }
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if let Some(other) = self.tabs.iter().find(|t| t.id != id && t.path.as_deref() == Some(canonical.as_path())) {
            self.quitting = None;
            self.prompt = Some(Prompt::Error(format!(
                "{} is open in another tab. Close that tab first, or save under a different name.",
                other.title()
            )));
            return Task::none();
        }
        match self.write_to(&id, path) {
            Ok(()) => then.map_or(Task::none(), |then| self.continue_after(&id, then, false)),
            Err(e) => {
                self.quitting = None;
                self.prompt = Some(Prompt::Error(format!("Could not save: {e}")));
                Task::none()
            }
        }
    }

    // --- closing and quitting

    /// Closes a tab, asking first if it has unsaved changes (and showing it,
    /// so the question is about something visible).
    fn close_tab(&mut self, id: &str) -> Task<Message> {
        let Some(i) = self.position(id) else { return Task::none() };
        if self.tabs[i].dirty() {
            self.select_tab(i);
            self.prompt = Some(Prompt::Unsaved { tab: id.to_owned(), title: self.tabs[i].title(), then: After::Close });
            return Task::none();
        }
        self.remove_tab(id);
        Task::none()
    }

    /// Closing the last tab leaves an empty one: quitting is Ctrl+Q.
    fn remove_tab(&mut self, id: &str) {
        let Some(i) = self.position(id) else { return };
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.tabs.push(Tab::new(tabs::new_id(), None, Document::empty(), fit(None, self.size)));
        }
        if self.active >= self.tabs.len() || i < self.active {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
        self.snapshot();
    }

    fn quit(&mut self) -> Task<Message> {
        if self.quitting.is_some() || self.prompt.is_some() {
            return Task::none();
        }
        if self.dev {
            return iced::exit();
        }
        self.tab_mut().doc.editor.finish_text();
        let _ = self.sync();
        self.quitting = Some(Quitting::default());
        self.quit_step()
    }

    /// Asks about the next tab with unsaved changes, or, with none left,
    /// ends the session and exits.
    fn quit_step(&mut self) -> Task<Message> {
        let Some(q) = &self.quitting else { return Task::none() };
        let next = self.tabs.iter().position(|t| t.dirty() && !q.asked.contains(&t.id));
        match next {
            Some(i) => {
                self.select_tab(i);
                let tab = &self.tabs[i];
                self.prompt = Some(Prompt::Unsaved { tab: tab.id.clone(), title: tab.title(), then: After::Quit });
                Task::none()
            }
            None => {
                let discarded = self.quitting.take().map(|q| q.discarded).unwrap_or_default();
                session::end(self, &discarded);
                instance::release();
                iced::exit()
            }
        }
    }

    /// Carries on after tab `id` was saved or discarded.
    fn continue_after(&mut self, id: &str, then: After, discarded: bool) -> Task<Message> {
        match then {
            After::Close => {
                self.remove_tab(id);
                Task::none()
            }
            After::Quit => {
                if let Some(q) = &mut self.quitting {
                    q.asked.insert(id.to_owned());
                    if discarded {
                        q.discarded.insert(id.to_owned());
                    }
                }
                self.quit_step()
            }
        }
    }

    fn answer(&mut self, answer: Answer) -> Task<Message> {
        match self.prompt.take() {
            Some(Prompt::Unsaved { tab, then, .. }) => match answer {
                Answer::Save => self.save_tab(tab, Some(then)),
                Answer::Discard => self.continue_after(&tab, then, true),
                Answer::Cancel | Answer::Confirm => {
                    self.quitting = None;
                    Task::none()
                }
            },
            Some(Prompt::Recover { session, .. }) => {
                let (tabs, problems) = session::open(self, session, answer == Answer::Save);
                if !tabs.is_empty() {
                    self.tabs = tabs;
                    self.active = 0;
                }
                if !problems.is_empty() {
                    self.prompt = Some(Prompt::Error(problems.join("\n")));
                }
                self.after_restore();
                Task::none()
            }
            Some(Prompt::DiscardTheme { then, .. }) => {
                if answer == Answer::Discard {
                    self.theme_edit(then, true);
                }
                Task::none()
            }
            Some(Prompt::DeleteTheme { .. }) => {
                if answer == Answer::Confirm {
                    self.theme_edit(theme_editor::Edit::Delete, true);
                }
                Task::none()
            }
            Some(Prompt::RemoveLibraryItem { id, .. }) => {
                if answer == Answer::Confirm {
                    self.remove_library_item(&id);
                }
                Task::none()
            }
            Some(Prompt::Error(_) | Prompt::Info(_)) | None => Task::none(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        Task::batch([task, self.sync()])
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        // While a question is up, it is the only thing that can be answered.
        if self.prompt.is_some() && !matches!(message, Message::Answer(_) | Message::Key(_) | Message::Resized(_) | Message::Autosave(_) | Message::Tick(_) | Message::Shot(_) | Message::HandedOver(_) | Message::SaveAsPicked(..) | Message::SystemDark(_) | Message::ExportTo(..) | Message::Exported(_) | Message::Library(library::Msg::Pictures(_) | library::Msg::Imported(_) | library::Msg::ExportTo(_))) {
            return Task::none();
        }
        // The menu closes on anything done in the window (a click outside it
        // only closes it), not on the clocks or the pointer passing over.
        let action = !matches!(
            message,
            Message::ToggleMenu(_) | Message::Key(_) | Message::Autosave(_) | Message::Tick(_) | Message::Shot(_) | Message::Resized(_) | Message::Paste(_) | Message::Opened(_) | Message::HandedOver(_) | Message::SaveAsPicked(..) | Message::SystemDark(_) | Message::ExportTo(..) | Message::Exported(_) | Message::Library(library::Msg::Pictures(_) | library::Msg::Imported(_) | library::Msg::ExportTo(_)) | Message::Input(Input::Move(..))
        );
        if self.menu.is_some() && action {
            self.menu = None;
            if matches!(message, Message::Input(Input::Down(..) | Input::DoubleClick(_))) {
                return Task::none();
            }
        }
        match message {
            Message::Input(Input::View(view)) => self.set_view(view),
            Message::Input(input) => {
                let pointer = &mut self.pointer;
                let editor = &mut self.tabs[self.active].doc.editor;
                if matches!(input, Input::Down(..)) {
                    self.popup = None;
                }
                match input {
                    Input::Down(p, m) => editor.pointer_down(p, m),
                    Input::Move(p, m) => {
                        *pointer = p;
                        editor.pointer_move(p, m);
                    }
                    Input::Up(p, m) => editor.pointer_up(p, m),
                    Input::DoubleClick(p) => editor.double_click(p),
                    Input::View(_) => {}
                }
            }
            Message::Tool(tool) => {
                let editor = &mut self.tab_mut().doc.editor;
                editor.tool = tool;
                if tool != Tool::Selection {
                    editor.selection.clear();
                }
            }
            Message::ToggleLock => {
                let editor = &mut self.tab_mut().doc.editor;
                editor.tool_locked = !editor.tool_locked;
            }
            Message::Props(m) => {
                let hex = matches!(m, properties::Msg::Hex(_));
                self.props(m);
                // Typing goes on in the text after a click on the panel
                // (but not while the hex field is being typed in).
                if !hex && self.tab().doc.editor.editing.is_some() {
                    return iced::widget::operation::focus(TEXT_BOX);
                }
            }
            Message::Undo => self.tab_mut().doc.editor.undo(),
            Message::Redo => self.tab_mut().doc.editor.redo(),
            Message::Key(event) => return self.key(event),
            Message::TextAction(action) => {
                let tab = self.tab_mut();
                if let Some((_, content)) = &mut tab.text_box {
                    let edit = action.is_edit();
                    content.perform(action);
                    if edit {
                        let typed = content.text();
                        tab.doc.editor.set_text(&typed);
                    }
                }
            }
            Message::TextDone => self.tab_mut().doc.editor.finish_text(),
            Message::Paste(text) => {
                let at = self.pointer;
                if let Some(text) = text {
                    if !self.tab_mut().doc.editor.paste(&text, at) {
                        self.status = Some("Only Excalidraw shapes can be pasted".into());
                    }
                }
            }
            Message::SelectTab(i) => self.select_tab(i),
            Message::CloseTab(id) => return self.close_tab(&id),
            Message::NewTab => self.new_tab(),
            Message::Open => return Self::pick_open(),
            Message::Opened(paths) => self.open_paths(paths),
            Message::OpenPath(path) => self.open_paths(vec![path]),
            Message::HandedOver(paths) => {
                for p in &paths {
                    self.allowed.allow(p);
                }
                if self.prompt.is_some() {
                    self.pending_open.extend(paths);
                } else {
                    self.open_paths(paths);
                }
                return window::latest().and_then(window::gain_focus);
            }
            Message::Save => {
                let id = self.tab().id.clone();
                return self.save_tab(id, None);
            }
            Message::SaveAs => {
                let id = self.tab().id.clone();
                self.tab_mut().doc.editor.finish_text();
                return self.pick_save(id, None);
            }
            Message::SaveAsPicked(id, path, then) => return self.save_as_picked(id, path, then),
            Message::ClearRecent => {
                let _ = recent::clear();
                self.recent.clear();
            }
            Message::ToggleMenu(m) => self.menu = if self.menu == Some(m) { None } else { Some(m) },
            Message::SelectTheme(id) => self.theme_edit(theme_editor::Edit::Use(id), false),
            Message::Theme(e) => self.theme_edit(e, false),
            Message::ReloadThemes => self.reload_themes(),
            Message::SystemDark(dark) => self.themes.system_dark = dark,
            Message::Export(kind) => return self.export_start(kind),
            Message::ExportTo(kind, path) => return self.export_to(kind, path),
            Message::Exported(result) => self.exported(result),
            Message::CopyImage => return self.copy_image(),
            Message::ExportChoice(c) => self.export_choice(c),
            Message::Library(m) => return self.library(m),
            Message::Quit => return self.quit(),
            Message::Answer(a) => return self.answer(a),
            Message::Autosave(now) => {
                if !self.dev && self.quitting.is_none() {
                    session::tick(self, now);
                }
            }
            Message::Resized(size) => {
                let first = !self.sized;
                self.sized = true;
                let size = Size::new(size.width, (size.height - self.bar).max(1.0));
                self.size = size;
                if first {
                    let zoom = self.zoom;
                    for tab in &mut self.tabs {
                        tab.view = fit(tab.doc.layers.bounds(), size).zoomed(zoom, Point::new(size.width / 2.0, size.height / 2.0));
                    }
                    let view = self.tab().view;
                    self.set_view(view);
                    if let Some(b) = &mut self.bench {
                        b.base = view;
                    }
                }
            }
            Message::Shot(shot) => {
                if let Some(path) = &self.screenshot {
                    match image::RgbaImage::from_raw(shot.size.width, shot.size.height, shot.rgba.to_vec()).map(|img| img.save(path)) {
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
                        return window::latest().and_then(window::screenshot).map(Message::Shot);
                    }
                    return Task::none();
                }
                let launched = self.launched;
                let size = self.size;
                let Some(b) = &mut self.bench else { return Task::none() };
                match b.tick(now, launched, size) {
                    Some(view) => self.set_view(view),
                    None => {
                        let elements = self.tab().doc.layers.prepared.len();
                        if let Some(b) = &self.bench {
                            b.report(&self.timings.borrow(), elements, self.load_time);
                        }
                        return iced::exit();
                    }
                }
            }
        }
        Task::none()
    }

    fn key(&mut self, event: keyboard::Event) -> Task<Message> {
        match event {
            keyboard::Event::KeyReleased { key: Key::Named(Named::Space), .. } => self.space = false,
            keyboard::Event::KeyPressed { key, modifiers, .. } => {
                if self.prompt.is_some() {
                    // Escape is Cancel, never Discard (review 2026-11-04, ST-1).
                    if key == Key::Named(Named::Escape) && !matches!(self.prompt, Some(Prompt::Recover { .. })) {
                        return self.answer(Answer::Cancel);
                    }
                    return Task::none();
                }
                if key == Key::Named(Named::Escape) {
                    if self.menu.take().is_some() || self.popup.take().is_some() {
                        return Task::none();
                    }
                    if self.panel.is_some() {
                        self.theme_edit(theme_editor::Edit::Close, false);
                        return Task::none();
                    }
                    if self.library.open {
                        self.library.open = false;
                        return Task::none();
                    }
                }
                let ctrl = modifiers.command();
                let shift = modifiers.shift();
                let lower = match key.as_ref() {
                    Key::Character(c) => c.to_lowercase(),
                    _ => String::new(),
                };
                let centre = Point::new(self.size.width / 2.0, self.size.height / 2.0);
                match (key.as_ref(), lower.as_str()) {
                    (Key::Named(Named::Space), _) => self.space = true,
                    (Key::Named(Named::Tab), _) if ctrl => {
                        let n = self.tabs.len();
                        let next = if shift { (self.active + n - 1) % n } else { (self.active + 1) % n };
                        self.select_tab(next);
                    }
                    (_, "s") if ctrl && shift => return self.handle(Message::SaveAs),
                    // Copy and paste styles; a tenth larger or smaller text.
                    (_, "c") if ctrl && modifiers.alt() => {
                        if self.tab_mut().doc.editor.copy_styles() {
                            self.status = Some("Copied styles".into());
                        }
                    }
                    (_, "v") if ctrl && modifiers.alt() => self.tab_mut().doc.editor.paste_styles(),
                    (_, "<" | ",") if ctrl && shift => self.tab_mut().doc.editor.step_font_size(false),
                    (_, ">" | ".") if ctrl && shift => self.tab_mut().doc.editor.step_font_size(true),
                    (_, "e") if ctrl && shift => return self.export_start(exporting::Kind::Png),
                    (_, "c") if ctrl && shift => return self.copy_image(),
                    (_, "s") if ctrl => return self.handle(Message::Save),
                    (_, "o") if ctrl => return Self::pick_open(),
                    (_, "n") if ctrl => self.new_tab(),
                    (_, "w") if ctrl => {
                        let id = self.tab().id.clone();
                        return self.close_tab(&id);
                    }
                    (_, "q") if ctrl => return self.quit(),
                    (_, ",") if ctrl => {
                        let e = if self.panel.is_some() { theme_editor::Edit::Close } else { theme_editor::Edit::Open };
                        self.theme_edit(e, false);
                    }
                    (_, "c") if ctrl => {
                        if let Some(text) = self.tab().doc.editor.copy() {
                            return clipboard::write(text);
                        }
                    }
                    (_, "x") if ctrl => {
                        if let Some(text) = self.tab_mut().doc.editor.cut() {
                            return clipboard::write(text);
                        }
                    }
                    (_, "v") if ctrl => return clipboard::read().map(Message::Paste),
                    // Ctrl+1…8: that tab; Ctrl+9: the last.
                    (_, d) if ctrl && d.len() == 1 && ("1"..="9").contains(&d) => {
                        let n: usize = d.parse().unwrap_or(1);
                        let i = if n == 9 { self.tabs.len() - 1 } else { n - 1 };
                        self.select_tab(i.min(self.tabs.len() - 1));
                    }
                    // Shift+1: the whole drawing in view, as in Excalidraw.
                    (_, "!" | "1") if shift => {
                        let view = fit(self.tab().doc.layers.bounds(), self.size);
                        self.set_view(view);
                    }
                    (_, "=" | "+") if ctrl => self.set_view(self.tab().view.zoomed(1.25, centre)),
                    (_, "-") if ctrl => self.set_view(self.tab().view.zoomed(0.8, centre)),
                    _ => {
                        if let Some(k) = editor_key(&key) {
                            let m = editor::Modifiers { shift, alt: modifiers.alt(), ctrl };
                            self.tab_mut().doc.editor.key(k, m);
                        }
                    }
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let tab = self.tab();
        let d = &tab.doc;
        let look = self.look();
        let scene = SceneView {
            layers: &d.layers,
            editor: &d.editor,
            built_at: &tab.built_at,
            view: tab.view,
            background: if self.file_background { d.background } else { look.canvas },
            selection: look.accent,
            panning: self.space || d.editor.tool == Tool::Hand,
            timings: self.bench.as_ref().map(|_| &self.timings),
            on_input: Box::new(Message::Input),
        };
        if self.bench.is_some() {
            return scene.into();
        }
        let mut layers = stack![Element::from(scene)];
        if let Some(text_box) = self.text_box(tab) {
            layers = layers.push(text_box);
        }
        if self.screenshot.is_some() && !self.chrome {
            // The drawing, and the text box if typing: no chrome.
            return layers.into();
        }
        let status = text(self.status.clone().unwrap_or_default()).size(12).color(look.muted);
        // Centred in what the theme editor leaves of the drawing.
        let beside = if self.panel.is_some() || self.library.open { theme_editor::WIDTH } else { 0.0 };
        let top = column![toolbar::toolbar(look, &d.editor), container(status).padding(iced::Padding { top: 0.0, right: 8.0, bottom: 8.0, left: 8.0 })].width(Length::Fill);
        layers = layers.push(container(top).padding(iced::Padding { right: beside, ..iced::Padding::ZERO }));
        if let Some(props) = properties::view(self) {
            layers = layers.push(props);
        }
        if let Some(panel) = &self.panel {
            // Takes the pointer over it; the drawing beside it stays live.
            layers = layers.push(theme_editor::view(self, panel));
        } else if self.library.open {
            layers = layers.push(library::view(self));
        }
        let mut window = stack![column![chrome::tab_bar(self), layers].height(Length::Fill)];
        match self.menu {
            Some(Menu::File) => window = window.push(chrome::file_menu(self)),
            Some(Menu::Theme) => window = window.push(chrome::theme_menu(self)),
            Some(Menu::Export) => window = window.push(chrome::export_menu(self)),
            None => {}
        }
        if let Some(prompt) = &self.prompt {
            window = window.push(opaque(chrome::prompt(look, prompt)));
        }
        window.into()
    }

    /// The text box over the text being typed. It takes the typing (input
    /// methods included) and draws the caret and selection; its own text is
    /// transparent, because the element itself stays drawn underneath, laid
    /// out live as it will be (wrapped, centred in its shape).
    ///
    /// Not a box that draws its own text over a hidden element, as
    /// Excalidraw's textarea is: iced's `text_editor` text did not appear in
    /// `window::screenshot` captures (with either renderer, also in a stock
    /// iced app), so it could not be checked, and a left-aligned box over a
    /// centred label would not show what the label becomes anyway.
    fn text_box<'a>(&'a self, tab: &'a Tab) -> Option<Element<'a, Message>> {
        let (id, content) = tab.text_box.as_ref()?;
        let e = tab.doc.editor.scene.by_id(id)?;
        let t = e.text.as_ref()?;
        let zoom = tab.view.zoom;
        let at = tab.view.to_screen([e.x, e.y]);
        let color = render::color::parse(&e.stroke_color);
        let color = Color::from_rgba(color.r, color.g, color.b, color.a);
        let size = t.font_size as f32 * zoom;
        // A label's box is exactly its width, so the caret wraps where it
        // does; free text gets room to grow.
        let width = if t.container_id.is_some() { (e.width as f32 * zoom).max(size) } else { ((e.width as f32 * zoom).max(size) + size).max(40.0) };
        let editor = text_editor(content)
            .id(TEXT_BOX)
            .on_action(Message::TextAction)
            .font(paint::font(t.font_family))
            .size(size)
            .line_height(text::LineHeight::Absolute(iced::Pixels((t.font_size * t.line_height) as f32 * zoom)))
            .padding(0)
            .width(width)
            // A label wraps inside its shape; free text only where typed.
            .wrapping(if t.container_id.is_some() { text::Wrapping::Word } else { text::Wrapping::None })
            .key_binding(|press| {
                let done = matches!(press.key.as_ref(), Key::Named(Named::Escape))
                    || (matches!(press.key.as_ref(), Key::Named(Named::Enter)) && press.modifiers.command());
                if done {
                    Some(text_editor::Binding::Custom(Message::TextDone))
                } else {
                    text_editor::Binding::from_key_press(press)
                }
            })
            .style(move |theme, status| text_editor::Style {
                background: Color::TRANSPARENT.into(),
                border: iced::Border::default(),
                value: Color::TRANSPARENT,
                selection: Color { a: 0.25, ..color },
                ..text_editor::default(theme, status)
            });
        Some(container(editor).padding(iced::Padding { top: at.y, left: at.x, right: 0.0, bottom: 0.0 }).into())
    }

    fn subscription(&self) -> Subscription<Message> {
        let resize = window::resize_events().map(|(_, size)| Message::Resized(size));
        if self.bench.is_some() || self.screenshot.is_some() {
            return Subscription::batch([resize, window::frames().map(Message::Tick)]);
        }
        let mut subs = vec![resize, keyboard::listen().map(Message::Key), window::close_requests().map(|_| Message::Quit)];
        if !self.dev {
            subs.push(iced::time::every(Duration::from_millis(400)).map(Message::Autosave));
            subs.push(Subscription::run(instance::incoming).map(Message::HandedOver));
            subs.push(Subscription::run(system_scheme).map(Message::SystemDark));
        }
        Subscription::batch(subs)
    }

    fn title(&self) -> String {
        let tab = self.tab();
        format!("{}{} — Excalidraw (Iced)", if tab.dirty() { "• " } else { "" }, tab.title())
    }
}

/// The desktop's light or dark preference as it changes, from the portal.
fn system_scheme() -> impl iced::futures::Stream<Item = bool> {
    use iced::futures::StreamExt;
    mundy::Preferences::stream(mundy::Interest::ColorScheme).map(|p| p.color_scheme == mundy::ColorScheme::Dark)
}

/// Puts the bundled fonts straight into iced's font database, borrowed from
/// the binary. `Application::font` copies them onto the heap first, which
/// for Excalifont with its CJK glyphs (22 MB) cost as much memory again;
/// borrowed, only the pages of glyphs actually drawn are ever read in.
fn load_fonts() {
    use iced::advanced::graphics::text::cosmic_text::fontdb;
    let mut system = iced::advanced::graphics::text::font_system().write().expect("the font system lock is not poisoned");
    for (_, bytes) in FONTS {
        let source: std::sync::Arc<dyn AsRef<[u8]> + Send + Sync> = std::sync::Arc::new(*bytes);
        system.raw().db_mut().load_font_source(fontdb::Source::Binary(source));
    }
}

fn main() -> iced::Result {
    let mut args = args();
    if let Some(out) = &args.export {
        if let Err(e) = exporting::export_cli(out, args.paths.first().map(PathBuf::as_path), args.theme.as_deref()) {
            eprintln!("excalidraw-iced: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    // Held until the app exits: the kernel lets go of it if the app dies.
    let mut _lock = None;
    if !args.dev() {
        use excalidraw_desktop_core::{files, store};
        store::set_app_dir(APP_DIR);
        let _ = store::ensure_private_dir(&store::config_dir());
        let drawings: Vec<PathBuf> = args.paths.iter().filter(|p| p.is_file()).map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())).collect();
        match instance::claim(&drawings) {
            Ok(instance::Claim::First(lock)) => _lock = lock,
            Ok(instance::Claim::HandedOver) => return Ok(()),
            Err(why) => {
                eprintln!("excalidraw-iced: {why}");
                std::process::exit(1);
            }
        }
        // Asked before the window opens, so a dark desktop never sees the
        // light theme first. No portal: light, as the Tauri app assumes.
        args.system_dark = mundy::Preferences::once_blocking(mundy::Interest::ColorScheme, Duration::from_millis(300))
            .is_some_and(|p| p.color_scheme == mundy::ColorScheme::Dark);
        // What a run that died mid-write left behind; nothing writes yet.
        for dir in ["", "session", "themes"] {
            files::sweep_temp_files(&store::config_dir().join(dir));
        }
    }
    let app = iced::application(move || App::boot(args.clone()), App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(App::theme)
        .window(window::Settings {
            size: WINDOW,
            // X11 shows this; Wayland takes the icon from the .desktop file.
            icon: window::icon::from_file_data(include_bytes!("../../../../src-tauri/icons/128x128.png"), None).ok(),
            platform_specific: window::settings::PlatformSpecific { application_id: APP_ID.into(), ..Default::default() },
            ..window::Settings::default()
        })
        .exit_on_close_request(false)
        .antialiasing(std::env::var_os("NO_MSAA").is_none());
    load_fonts();
    app.run()
}
