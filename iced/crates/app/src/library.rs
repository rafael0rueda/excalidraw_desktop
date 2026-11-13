//! The Library: shapes kept to put on any drawing, as the Tauri app keeps
//! them (src/lib/library.ts): in `library.excalidrawlib` in the config
//! directory, in Excalidraw's own format, so either app reads the other's
//! file. A first launch starts with the ER diagram shapes; the Kanagawa ERD
//! sets are added on demand. `.excalidrawlib` files can be imported and the
//! library exported as one.
//!
//! Items are kept as the JSON they were read as, so nothing this app does
//! not know about is lost on the way back to disk.

use crate::{App, Message, Prompt};
use excalidraw_desktop_core::files;
use iced::widget::image::Handle;
use iced::Task;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;

const ER: &str = include_str!("../../../../resources/libraries/entity-relationship.excalidrawlib");
const WAVE: &str = include_str!("../../../../resources/libraries/kanagawa-erd-wave.excalidrawlib");
const DRAGON: &str = include_str!("../../../../resources/libraries/kanagawa-erd-dragon.excalidrawlib");
const LOTUS: &str = include_str!("../../../../resources/libraries/kanagawa-erd-lotus.excalidrawlib");

/// The sets that come with the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Set {
    Er,
    Wave,
    Dragon,
    Lotus,
}

impl Set {
    fn text(self) -> &'static str {
        match self {
            Set::Er => ER,
            Set::Wave => WAVE,
            Set::Dragon => DRAGON,
            Set::Lotus => LOTUS,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Set::Er => "ER diagram",
            Set::Wave => "Wave",
            Set::Dragon => "Dragon",
            Set::Lotus => "Lotus",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Toggle,
    /// Put an item on the drawing, in the middle of the view.
    Insert(String),
    /// Remove an item (asks first).
    Remove(String),
    AddSelection,
    AddSet(Set),
    Import,
    Imported(Vec<PathBuf>),
    Export,
    ExportTo(Option<PathBuf>),
    /// Pictures of items, drawn on another thread: (id, width, height, RGBA).
    Pictures(Vec<(String, u32, u32, Vec<u8>)>),
    /// An item pressed: dragged onto the drawing, or a click.
    Grab(String),
    /// The pointer, anywhere in the window, while an item is held.
    DragMove(iced::Point),
    /// Let go: on the drawing it lands there; a click without moving puts it
    /// in the middle of the view.
    Drop,
}

/// An item being dragged out of the panel.
#[derive(Debug, Clone)]
pub struct Drag {
    pub id: String,
    /// Where the pointer is in the window, once it has moved.
    pub at: Option<iced::Point>,
    start: Option<iced::Point>,
}

pub struct Library {
    /// `libraryItems`, newest first, as read.
    pub items: Vec<Map<String, Value>>,
    /// False when the file was there but could not be read: then nothing is
    /// written over it (the Tauri app's rule).
    writable: bool,
    persist: bool,
    pub pictures: HashMap<String, Handle>,
    /// Items whose picture is being drawn, or could not be.
    pending: std::collections::HashSet<String>,
    pub open: bool,
    pub drag: Option<Drag>,
}

/// `restoreLibraryItems`: version 2's `libraryItems`, or version 1's
/// `library` (arrays of elements), as items.
pub fn parse(text: &str) -> Result<Vec<Map<String, Value>>, String> {
    let data: Value = serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
    let list = data
        .get("libraryItems")
        .or_else(|| data.get("library"))
        .and_then(Value::as_array)
        .ok_or("not an Excalidraw library")?;
    let mut items = Vec::new();
    for item in list {
        let mut map = match item {
            Value::Array(elements) => {
                let mut m = Map::new();
                m.insert("status".into(), json!("unpublished"));
                m.insert("elements".into(), Value::Array(elements.clone()));
                m
            }
            Value::Object(m) => m.clone(),
            _ => continue,
        };
        if !map.get("elements").is_some_and(|e| e.as_array().is_some_and(|a| !a.is_empty())) {
            continue;
        }
        if !map.get("id").is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty())) {
            map.insert("id".into(), json!(scene::edit::random_id()));
        }
        if !map.contains_key("status") {
            map.insert("status".into(), json!("unpublished"));
        }
        if !map.contains_key("created") {
            map.insert("created".into(), json!(scene::edit::now_ms()));
        }
        items.push(map);
    }
    Ok(items)
}

pub fn id(item: &Map<String, Value>) -> &str {
    item.get("id").and_then(Value::as_str).unwrap_or("")
}

pub fn name(item: &Map<String, Value>) -> Option<&str> {
    item.get("name").and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn elements(item: &Map<String, Value>) -> &[Value] {
    item.get("elements").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// `isUniqueItem`: no item already has the same elements, by id and
/// `versionNonce`, in the same order.
fn is_new(existing: &[Map<String, Value>], item: &Map<String, Value>) -> bool {
    let key = |e: &Value| (e.get("id").cloned(), e.get("versionNonce").cloned());
    let theirs: Vec<_> = elements(item).iter().map(key).collect();
    !existing.iter().any(|i| elements(i).iter().map(key).collect::<Vec<_>>() == theirs)
}

impl Library {
    /// The saved library, or the ER shapes if none was ever saved.
    pub fn load(persist: bool) -> (Self, Option<String>) {
        let seed = || parse(ER).unwrap_or_default();
        let (items, writable, problem) = if !persist {
            (seed(), true, None)
        } else {
            match excalidraw_desktop_core::library::load_library() {
                Ok(None) => (seed(), true, None),
                Ok(Some(text)) => match parse(&text) {
                    Ok(items) => (items, true, None),
                    Err(e) => (Vec::new(), false, Some(format!("The library could not be read ({e}), so it is left as it is on disk and changes to it are not saved."))),
                },
                Err(e) => (Vec::new(), false, Some(format!("The library could not be read: {e}"))),
            }
        };
        let library = Self { items, writable, persist, pictures: HashMap::new(), pending: Default::default(), open: false, drag: None };
        (library, problem)
    }

    /// The file's text, as the Tauri app writes it.
    pub fn to_json(&self) -> String {
        let items: Vec<Value> = self.items.iter().cloned().map(Value::Object).collect();
        scene::to_js_json(&json!({ "type": "excalidrawlib", "version": 2, "source": "excalidraw-desktop", "libraryItems": items }))
    }

    fn save(&self) -> Result<(), String> {
        if !self.persist {
            return Ok(());
        }
        if !self.writable {
            return Err("the library file could not be read when the app started, so it is not written over".into());
        }
        excalidraw_desktop_core::library::save_library(self.to_json())
    }

    /// `mergeLibraryItems`: the items not already here, put first. How many.
    pub fn merge(&mut self, items: Vec<Map<String, Value>>) -> usize {
        let new: Vec<_> = items.into_iter().filter(|i| is_new(&self.items, i)).collect();
        let n = new.len();
        self.items.splice(0..0, new);
        n
    }

    /// Items with no picture yet, as owned scenes to draw elsewhere.
    fn unpictured(&mut self) -> Vec<(String, scene::Scene)> {
        let mut out = Vec::new();
        for item in &self.items {
            let id = id(item).to_owned();
            if self.pictures.contains_key(&id) || self.pending.contains(&id) {
                continue;
            }
            self.pending.insert(id.clone());
            if let Ok(s) = scene::Scene::parse(&json!({ "type": "excalidraw", "elements": elements(item) }).to_string()) {
                out.push((id, s));
            }
        }
        out
    }
}

/// The size a picture is drawn at: its cell, at twice the pixels for
/// sharp edges on a high-density screen.
pub const CELL: f32 = 74.0;

impl App {
    pub fn library(&mut self, msg: Msg) -> Task<Message> {
        match msg {
            Msg::Toggle => {
                self.library.open = !self.library.open;
                if self.library.open {
                    // One panel down the right at a time.
                    if self.panel.is_some() {
                        self.theme_edit(crate::theme_editor::Edit::Close, false);
                        if self.panel.is_some() {
                            self.library.open = false;
                            return Task::none();
                        }
                    }
                    return self.draw_pictures();
                }
            }
            Msg::Grab(id) => self.library.drag = Some(Drag { id, at: None, start: None }),
            Msg::DragMove(p) => {
                if let Some(d) = &mut self.library.drag {
                    match d.start {
                        None => d.start = Some(p),
                        // A few pixels of wobble is still a click.
                        Some(s) if d.at.is_some() || (p.x - s.x).abs() + (p.y - s.y).abs() > 4.0 => d.at = Some(p),
                        Some(_) => {}
                    }
                }
            }
            Msg::Drop => {
                let Some(d) = self.library.drag.take() else { return Task::none() };
                match d.at {
                    None => return self.library(Msg::Insert(d.id)),
                    Some(p) => {
                        // Over the drawing, not the panel or the tab bar.
                        let panel = crate::theme_editor::WIDTH;
                        let y = p.y - self.bar;
                        if y >= 0.0 && p.x < self.size.width - panel {
                            let at = self.tab().view.to_scene(iced::Point::new(p.x, y));
                            self.insert_library_item(&d.id, at);
                        }
                    }
                }
            }
            Msg::Insert(id) => {
                let size = self.size;
                let at = self.tab().view.to_scene(iced::Point::new(size.width / 2.0, size.height / 2.0));
                self.insert_library_item(&id, at);
            }
            Msg::Remove(id) => {
                if let Some(item) = self.library.items.iter().find(|i| self::id(i) == id) {
                    let name = name(item).unwrap_or("this item").to_owned();
                    self.prompt = Some(Prompt::RemoveLibraryItem { id, name });
                }
            }
            Msg::AddSelection => {
                let elements = self.tab().doc.editor.selected_elements();
                if elements.is_empty() {
                    self.status = Some("Select what to add to the library first".into());
                    return Task::none();
                }
                let mut item = Map::new();
                item.insert("id".into(), json!(scene::edit::random_id()));
                item.insert("status".into(), json!("unpublished"));
                item.insert("elements".into(), Value::Array(elements));
                item.insert("created".into(), json!(scene::edit::now_ms()));
                self.library.items.insert(0, item);
                self.library_saved("Added to the library");
                return self.draw_pictures();
            }
            Msg::AddSet(set) => {
                let items = parse(set.text()).unwrap_or_default();
                let total = items.len();
                let n = self.library.merge(items);
                let what = format!("{} shapes", set.label());
                let done = match (n, total - n) {
                    (0, _) => format!("The {what} are all in the library already"),
                    (n, 0) => format!("Added {n} {what}"),
                    (n, had) => format!("Added {n} {what} ({had} were there already)"),
                };
                self.library.open = true;
                self.library_saved(&done);
                return self.draw_pictures();
            }
            Msg::Import => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Excalidraw library", &["excalidrawlib", "json"])
                            .pick_files()
                            .await
                            .map(|f| f.into_iter().map(|f| f.path().to_path_buf()).collect())
                            .unwrap_or_default()
                    },
                    |paths| Message::Library(Msg::Imported(paths)),
                );
            }
            Msg::Imported(paths) => {
                let mut added = 0;
                let mut problems = Vec::new();
                for p in paths {
                    let read = files::readable(&p, files::MAX_TEXT_BYTES)
                        .and_then(|()| std::fs::read_to_string(&p).map_err(|e| e.to_string()))
                        .and_then(|t| parse(&t));
                    match read {
                        Ok(items) => added += self.library.merge(items),
                        Err(e) => problems.push(format!("{}: {e}", p.display())),
                    }
                }
                if !problems.is_empty() {
                    self.prompt = Some(Prompt::Error(format!("Could not import:\n{}", problems.join("\n"))));
                }
                if added > 0 {
                    self.library_saved(&format!("Imported {added} shape{}", if added == 1 { "" } else { "s" }));
                } else if self.prompt.is_none() {
                    self.status = Some("Nothing new to import".into());
                }
                return self.draw_pictures();
            }
            Msg::Export => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Excalidraw library", &["excalidrawlib"])
                            .set_file_name("library.excalidrawlib")
                            .save_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    |path| Message::Library(Msg::ExportTo(path)),
                );
            }
            Msg::ExportTo(path) => {
                let Some(mut path) = path else { return Task::none() };
                if !path.extension().is_some_and(|e| e == "excalidrawlib") {
                    path.as_mut_os_string().push(".excalidrawlib");
                }
                match files::write_atomic(&path, self.library.to_json().as_bytes()) {
                    Ok(()) => self.status = Some(format!("Exported the library to {}", path.display())),
                    Err(e) => self.prompt = Some(Prompt::Error(format!("Could not export the library: {e}"))),
                }
            }
            Msg::Pictures(pictures) => {
                for (id, w, h, rgba) in pictures {
                    self.library.pending.remove(&id);
                    self.library.pictures.insert(id, Handle::from_rgba(w, h, rgba));
                }
            }
        }
        Task::none()
    }

    /// Removing an item, once confirmed.
    pub fn remove_library_item(&mut self, id: &str) {
        self.library.items.retain(|i| self::id(i) != id);
        self.library.pictures.remove(id);
        self.library_saved("Removed from the library");
    }

    fn library_saved(&mut self, done: &str) {
        match self.library.save() {
            Ok(()) => self.status = Some(done.to_owned()),
            Err(e) => self.status = Some(format!("{done}, but the library was not saved: {e}")),
        }
    }

    /// Draws the pictures still missing, on another thread, while the
    /// library is open.
    fn draw_pictures(&mut self) -> Task<Message> {
        if !self.library.open {
            return Task::none();
        }
        let todo = self.library.unpictured();
        if todo.is_empty() {
            return Task::none();
        }
        let canvas = self.look_canvas_css();
        let side = f64::from(CELL) * 2.0;
        Task::perform(
            crate::exporting::off_thread(move || {
                todo.into_iter()
                    .filter_map(|(id, scene)| {
                        let x = export::Export { scene: &scene, only: None, canvas: &canvas, transparent: true, font: &crate::paint::export_font };
                        let (w, h, rgba) = x.thumbnail(side, 2.0).ok()?;
                        Some((id, w, h, rgba))
                    })
                    .collect()
            }),
            |pictures| Message::Library(Msg::Pictures(pictures.unwrap_or_default())),
        )
    }
}

// ------------------------------------------------------------ the panel

use crate::chrome::{self, small};
use iced::widget::{button, column, container, image, mouse_area, pin, row, scrollable, stack, text, tooltip, Space};
use iced::{Alignment, Background, Border, Element, Length};

fn lib(m: Msg) -> Message {
    Message::Library(m)
}

/// One item: its picture (its name until drawn), a click to put it on the
/// drawing, and a × to remove it.
fn cell<'a>(app: &'a App, item: &'a Map<String, Value>) -> Element<'a, Message> {
    let look = app.look();
    let id = id(item).to_owned();
    let face: Element<'a, Message> = match app.library.pictures.get(&id) {
        Some(handle) => image(handle.clone()).width(CELL - 10.0).height(CELL - 10.0).content_fit(iced::ContentFit::Contain).into(),
        None => text(name(item).unwrap_or("…").to_owned()).size(10).color(look.muted).into(),
    };
    // Pressed, then dragged onto the drawing or let go as a click.
    let held = app.library.drag.as_ref().is_some_and(|d| d.id == id);
    let body = button(container(face).center(CELL - 2.0)).padding(0).width(CELL).height(CELL).style(move |_, status| {
        let hover = held || matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(look.canvas)),
            border: Border { color: if hover { look.accent } else { look.surface_alt }, width: 1.0, radius: 6.0.into() },
            text_color: look.text,
            ..button::Style::default()
        }
    });
    let body = mouse_area(body).on_press(lib(Msg::Grab(id.clone()))).interaction(iced::mouse::Interaction::Grab);
    let remove = button(text("×").size(11)).on_press(lib(Msg::Remove(id))).padding([0, 4]).style(chrome::chip(look, false));
    let both = stack![body, container(remove).align_right(CELL)];
    match name(item) {
        Some(n) => tooltip(both, container(text(n).size(13)).padding([4, 8]).style(move |t| chrome::island(look)(t)), tooltip::Position::Bottom).into(),
        None => both.into(),
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let look = app.look();
    let action = |label: &str, m: Option<Message>| button(small(label.to_owned())).on_press_maybe(m).style(chrome::secondary(look)).padding([5, 10]);
    let selected = !app.tab().doc.editor.selection.is_empty();
    let actions = row![
        action("Add selection", selected.then_some(lib(Msg::AddSelection))),
        action("Import…", Some(lib(Msg::Import))),
        action("Export…", (!app.library.items.is_empty()).then_some(lib(Msg::Export))),
    ]
    .spacing(6);
    let sets = column![
        small("Add the shapes that come with the app:").color(look.muted),
        row![
            action(Set::Er.label(), Some(lib(Msg::AddSet(Set::Er)))),
            small("Kanagawa ERD").color(look.muted),
            action(Set::Wave.label(), Some(lib(Msg::AddSet(Set::Wave)))),
            action(Set::Dragon.label(), Some(lib(Msg::AddSet(Set::Dragon)))),
            action(Set::Lotus.label(), Some(lib(Msg::AddSet(Set::Lotus)))),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .wrap(),
    ]
    .spacing(6);

    let per_row = 4;
    let mut grid = column![].spacing(8);
    for chunk in app.library.items.chunks(per_row) {
        let mut r = row![].spacing(8);
        for item in chunk {
            r = r.push(cell(app, item));
        }
        grid = grid.push(r);
    }
    let body: Element<'_, Message> = if app.library.items.is_empty() {
        text("Nothing here yet. Select shapes and choose Add selection, or add a set above.").size(13).color(look.muted).into()
    } else {
        grid.into()
    };
    let hint = text("Drag a shape onto the drawing, or click it to put it in the middle of the view.").size(12).color(look.muted);

    let header = row![
        text("Library").size(16).color(look.text),
        Space::new().width(Length::Fill),
        button(small("×")).on_press(lib(Msg::Toggle)).style(chrome::chip(look, false))
    ]
    .align_y(Alignment::Center)
    .padding([10, 16]);
    let content = column![actions, sets, hint, body].spacing(14).padding(iced::Padding { top: 4.0, right: 16.0, bottom: 16.0, left: 16.0 });
    let card = column![header, scrollable(content).spacing(2).height(Length::Fill)];
    container(container(card).width(crate::theme_editor::WIDTH).height(Length::Fill).style(move |t| container::Style { border: Border { radius: 0.0.into(), ..chrome::island(look)(t).border }, ..chrome::island(look)(t) }))
        .align_right(Length::Fill)
        .into()
}

impl App {
    /// A copy of library item `id` on the drawing, centred on `at`.
    fn insert_library_item(&mut self, id: &str, at: [f64; 2]) {
        let Some(item) = self.library.items.iter().find(|i| self::id(i) == id) else { return };
        let items = elements(item).to_vec();
        let editor = &mut self.tab_mut().doc.editor;
        editor.finish_text();
        if editor.insert(&items, None, at) {
            editor.tool = editor::Tool::Selection;
        }
    }
}

/// The item being dragged, under the pointer.
pub fn ghost(app: &App) -> Option<Element<'_, Message>> {
    let d = app.library.drag.as_ref()?;
    let at = d.at?;
    let picture = app.library.pictures.get(&d.id)?;
    let size = CELL - 10.0;
    let img = image(picture.clone()).width(size).height(size).content_fit(iced::ContentFit::Contain).opacity(0.7_f32);
    Some(pin(img).x(at.x - size / 2.0).y(at.y - size / 2.0).into())
}
