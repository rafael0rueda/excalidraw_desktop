//! Editing an Excalidraw drawing, with no UI: the app feeds pointer and key
//! events in scene coordinates and draws what this reports.
//!
//! Behaviour follows Excalidraw 0.18.1 where it is visible to the user:
//! the shortcuts, what a click picks (`shouldTestInside`), that a group is
//! selected whole, that tiny new shapes vanish (`isInvisiblySmallElement`),
//! the clipboard format, and the nudge distances.

#![forbid(unsafe_code)]

pub mod geometry;
mod history;
mod transform;

use geometry::{bounds_of, contains, Bounds};
use history::History;
use rough::Point;
use scene::edit::{new_element, random_id, update, update_num, update_points, Style, Updates};
use scene::{Element, Kind, Scene};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
pub use transform::Handle;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Selection,
    Hand,
    Rectangle,
    Diamond,
    Ellipse,
    Arrow,
    Line,
    FreeDraw,
}

impl Tool {
    fn element_type(self) -> Option<&'static str> {
        Some(match self {
            Tool::Rectangle => "rectangle",
            Tool::Diamond => "diamond",
            Tool::Ellipse => "ellipse",
            Tool::Arrow => "arrow",
            Tool::Line => "line",
            Tool::FreeDraw => "freedraw",
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    /// Ctrl, or Cmd on a Mac.
    pub ctrl: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Delete,
    Backspace,
    Escape,
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Default,
    Move,
    Crosshair,
    Grab,
    Resize(Handle),
}

/// What changed since the app last asked: elements to redraw, and whether
/// the stacking order moved.
#[derive(Debug, Default)]
pub struct Changes {
    pub ids: HashSet<String>,
    pub order: bool,
}

enum Gesture {
    /// Pressed on nothing: a rubber band, from `start`, adding to `base`.
    Marquee {
        start: Point,
        base: Vec<String>,
    },
    Drag {
        start: Point,
        origins: Vec<(String, f64, f64)>,
    },
    Resize(transform::Resize),
    Rotate(transform::Rotate),
    Create {
        id: String,
        origin: Point,
    },
}

pub struct Editor {
    pub scene: Scene,
    /// Selected element ids, in no particular order.
    pub selection: Vec<String>,
    pub tool: Tool,
    /// What new elements look like.
    pub style: Style,
    /// Keep the tool after drawing, instead of going back to selection.
    pub tool_locked: bool,
    /// Scene units per screen pixel's worth of zoom: thresholds and handle
    /// sizes are fixed on screen.
    pub zoom: f64,
    history: History,
    gesture: Option<Gesture>,
    changes: Changes,
    dirty: bool,
    /// The rubber band being dragged, for the overlay.
    pub marquee: Option<Bounds>,
}

/// Excalidraw's hit tolerance: 10 screen pixels.
const HIT_PX: f64 = 10.0;
/// `ELEMENT_TRANSLATE_AMOUNT`, `ELEMENT_SHIFT_TRANSLATE_AMOUNT`.
const NUDGE: f64 = 1.0;
const SHIFT_NUDGE: f64 = 5.0;
/// Ctrl+D's offset for the copies.
const DUPLICATE_OFFSET: f64 = 10.0;
pub const CLIPBOARD_TYPE: &str = "excalidraw/clipboard";

impl Editor {
    pub fn new(scene: Scene) -> Self {
        Self {
            scene,
            selection: Vec::new(),
            tool: Tool::Selection,
            style: Style::default(),
            tool_locked: false,
            zoom: 1.0,
            history: History::default(),
            gesture: None,
            changes: Changes::default(),
            dirty: false,
            marquee: None,
        }
    }

    pub fn take_changes(&mut self) -> Changes {
        std::mem::take(&mut self.changes)
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Whether the drawing differs from the last `mark_saved`.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    // --- changing the scene, always through these

    fn begin(&mut self) {
        self.history.begin(&self.scene);
    }

    fn commit(&mut self) {
        self.history.commit(&self.scene);
    }

    fn change(&mut self, id: &str, updates: Updates) -> bool {
        self.history.touch(&self.scene, id);
        let changed = self.scene.mutate(id, updates);
        if changed {
            self.changes.ids.insert(id.to_owned());
            self.dirty = true;
        }
        changed
    }

    fn add(&mut self, at: usize, raw: Map<String, Value>) -> Option<String> {
        let id = raw.get("id")?.as_str()?.to_owned();
        self.history.touch(&self.scene, &id);
        let before = self.scene.order();
        let id = self.scene.insert(at, raw).ok()?;
        // Its neighbours may have been re-keyed only if the scene's indices
        // were out of order; record whoever's key changed.
        for other in before {
            self.changes.ids.insert(other);
        }
        self.changes.ids.insert(id.clone());
        self.changes.order = true;
        self.dirty = true;
        Some(id)
    }

    fn remove_new(&mut self, id: &str) {
        self.scene.set_raw(id, None);
        self.changes.ids.insert(id.to_owned());
        self.changes.order = true;
    }

    fn visible(&self, id: &str) -> Option<&Element> {
        self.scene.by_id(id).filter(|e| !e.is_deleted)
    }

    fn selected(&self) -> Vec<&Element> {
        let set: HashSet<&str> = self.selection.iter().map(String::as_str).collect();
        self.scene
            .visible()
            .filter(|e| set.contains(e.id.as_str()))
            .collect()
    }

    /// Selected ids in stacking order, bottom first.
    fn selected_in_order(&self) -> Vec<String> {
        self.selected().iter().map(|e| e.id.clone()).collect()
    }

    pub fn selection_bounds(&self) -> Option<Bounds> {
        bounds_of(self.selected())
    }

    // --- selection

    /// The ids `id` brings with it: everything in its outermost group.
    fn with_group(&self, id: &str) -> Vec<String> {
        let Some(e) = self.visible(id) else {
            return Vec::new();
        };
        match e.group_ids.last() {
            Some(group) => self
                .scene
                .visible()
                .filter(|o| o.group_ids.contains(group))
                .map(|o| o.id.clone())
                .collect(),
            None => vec![id.to_owned()],
        }
    }

    pub fn select(&mut self, ids: Vec<String>) {
        let mut out: Vec<String> = Vec::new();
        for id in ids {
            for g in self.with_group(&id) {
                if !out.contains(&g) {
                    out.push(g);
                }
            }
        }
        self.selection = out;
    }

    pub fn select_all(&mut self) {
        self.selection = self
            .scene
            .visible()
            .filter(|e| e.kind != Kind::Unknown)
            .map(|e| e.id.clone())
            .collect();
    }

    /// The topmost element under `p`.
    pub fn element_at(&self, p: Point) -> Option<&Element> {
        let threshold = HIT_PX / self.zoom;
        self.scene
            .visible()
            .filter(|e| geometry::hit(e, p, threshold))
            .last()
    }

    // --- pointer

    pub fn cursor_at(&self, p: Point) -> Cursor {
        match self.tool {
            Tool::Hand => return Cursor::Grab,
            Tool::Selection => {}
            _ => return Cursor::Crosshair,
        }
        if let Some(h) = self.handle_at(p) {
            return Cursor::Resize(h);
        }
        if self.element_at(p).is_some()
            || self
                .selection_bounds()
                .is_some_and(|b| self.selection.len() > 1 && contains(b, p))
        {
            Cursor::Move
        } else {
            Cursor::Default
        }
    }

    /// The transform handles of the selection, for drawing them.
    pub fn handles(&self) -> Option<transform::Handles> {
        transform::handles(&self.selected(), self.zoom)
    }

    fn handle_at(&self, p: Point) -> Option<Handle> {
        let handles = self.handles()?;
        handles.at(p, self.zoom)
    }

    pub fn pointer_down(&mut self, p: Point, m: Modifiers) {
        if self.gesture.is_some() {
            return;
        }
        if let Some(kind) = self.tool.element_type() {
            self.begin();
            let mut raw = new_element(kind, p[0], p[1], &self.style);
            if kind == "freedraw" {
                raw.insert("points".into(), json!([[0, 0]]));
            } else if kind == "line" || kind == "arrow" {
                raw.insert("points".into(), json!([[0, 0], [0, 0]]));
            }
            if let Some(id) = self.add(self.scene.elements.len(), raw) {
                self.selection = vec![id.clone()];
                self.gesture = Some(Gesture::Create { id, origin: p });
            }
            return;
        }
        if self.tool != Tool::Selection {
            return;
        }
        if let Some(handle) = self.handle_at(p) {
            let selected: Vec<Element> = self.selected().into_iter().cloned().collect();
            self.begin();
            self.gesture = Some(match handle {
                Handle::Rotation => Gesture::Rotate(transform::Rotate::new(selected, p)),
                h => Gesture::Resize(transform::Resize::new(selected, h, p)),
            });
            return;
        }
        let hit = self.element_at(p).map(|e| e.id.clone());
        let in_selection_box =
            self.selection.len() > 1 && self.selection_bounds().is_some_and(|b| contains(b, p));
        match hit {
            Some(id) if m.shift => {
                // Shift-click toggles, group and all.
                let group = self.with_group(&id);
                if self.selection.contains(&id) {
                    self.selection.retain(|s| !group.contains(s));
                    return;
                }
                for g in group {
                    if !self.selection.contains(&g) {
                        self.selection.push(g);
                    }
                }
            }
            Some(id) if !self.selection.contains(&id) => self.select(vec![id]),
            Some(_) => {}
            None if in_selection_box => {}
            None => {
                let base = if m.shift {
                    self.selection.clone()
                } else {
                    Vec::new()
                };
                self.selection = base.clone();
                self.gesture = Some(Gesture::Marquee { start: p, base });
                return;
            }
        }
        let origins = self
            .selected()
            .iter()
            .map(|e| (e.id.clone(), e.x, e.y))
            .collect();
        self.begin();
        self.gesture = Some(Gesture::Drag { start: p, origins });
    }

    pub fn pointer_move(&mut self, p: Point, m: Modifiers) {
        match self.gesture.take() {
            Some(Gesture::Marquee { start, base }) => {
                let b = [
                    start[0].min(p[0]),
                    start[1].min(p[1]),
                    start[0].max(p[0]),
                    start[1].max(p[1]),
                ];
                self.marquee = Some(b);
                // Whatever lies wholly inside the band.
                let inside: Vec<String> = self
                    .scene
                    .visible()
                    .filter(|e| e.kind != Kind::Unknown)
                    .filter(|e| {
                        let eb = geometry::bounds(e);
                        eb[0] >= b[0] && eb[1] >= b[1] && eb[2] <= b[2] && eb[3] <= b[3]
                    })
                    .map(|e| e.id.clone())
                    .collect();
                let mut ids = base.clone();
                ids.extend(inside);
                self.select(ids);
                self.gesture = Some(Gesture::Marquee { start, base });
            }
            Some(Gesture::Drag { start, origins }) => {
                let (mut dx, mut dy) = (p[0] - start[0], p[1] - start[1]);
                if m.shift {
                    // Shift keeps the drag on one axis.
                    if dx.abs() > dy.abs() {
                        dy = 0.0;
                    } else {
                        dx = 0.0;
                    }
                }
                for (id, x, y) in &origins {
                    self.change(id, vec![update_num("x", x + dx), update_num("y", y + dy)]);
                }
                self.gesture = Some(Gesture::Drag { start, origins });
            }
            Some(Gesture::Resize(r)) => {
                for (id, updates) in r.updates(p, m) {
                    self.change(&id, updates);
                }
                self.gesture = Some(Gesture::Resize(r));
            }
            Some(Gesture::Rotate(r)) => {
                for (id, updates) in r.updates(p, m) {
                    self.change(&id, updates);
                }
                self.gesture = Some(Gesture::Rotate(r));
            }
            Some(Gesture::Create { id, origin }) => {
                self.create_to(&id, origin, p, m);
                self.gesture = Some(Gesture::Create { id, origin });
            }
            None => {}
        }
    }

    fn create_to(&mut self, id: &str, origin: Point, p: Point, m: Modifiers) {
        let Some(kind) = self.scene.by_id(id).map(|e| e.kind) else {
            return;
        };
        match kind {
            Kind::FreeDraw => {
                let Some(fd) = self.scene.by_id(id).and_then(|e| e.free_draw.clone()) else {
                    return;
                };
                let mut points = fd.points;
                let next = [p[0] - origin[0], p[1] - origin[1]];
                if points.last() != Some(&next) {
                    points.push(next);
                    self.change(id, vec![update_points(&points)]);
                }
            }
            Kind::Line | Kind::Arrow => {
                let (mut dx, mut dy) = (p[0] - origin[0], p[1] - origin[1]);
                if m.shift {
                    // `SHIFT_LOCKING_ANGLE`: 15° steps.
                    let step = std::f64::consts::PI / 12.0;
                    let len = dx.hypot(dy);
                    let a = (dy.atan2(dx) / step).round() * step;
                    (dx, dy) = (len * a.cos(), len * a.sin());
                }
                self.change(id, vec![update_points(&[[0.0, 0.0], [dx, dy]])]);
            }
            _ => {
                let (mut w, mut h) = ((p[0] - origin[0]).abs(), (p[1] - origin[1]).abs());
                if m.shift {
                    w = w.max(h);
                    h = w;
                }
                let sx = if p[0] < origin[0] { -1.0 } else { 1.0 };
                let sy = if p[1] < origin[1] { -1.0 } else { 1.0 };
                let (x, y, w, h) = if m.alt {
                    (origin[0] - w, origin[1] - h, w * 2.0, h * 2.0)
                } else {
                    (
                        if sx < 0.0 { origin[0] - w } else { origin[0] },
                        if sy < 0.0 { origin[1] - h } else { origin[1] },
                        w,
                        h,
                    )
                };
                self.change(
                    id,
                    vec![
                        update_num("x", x),
                        update_num("y", y),
                        update_num("width", w),
                        update_num("height", h),
                    ],
                );
            }
        }
    }

    pub fn pointer_up(&mut self, p: Point, m: Modifiers) {
        self.pointer_move(p, m);
        self.marquee = None;
        match self.gesture.take() {
            Some(Gesture::Create { id, .. }) => {
                let e = self.scene.by_id(&id);
                // `isInvisiblySmallElement`, and a zero-length line.
                let tiny = e.is_none_or(|e| match e.kind {
                    Kind::Line | Kind::Arrow => e.width == 0.0 && e.height == 0.0,
                    Kind::FreeDraw => false,
                    _ => e.width == 0.0 && e.height == 0.0,
                });
                if tiny {
                    self.history.cancel(&mut self.scene);
                    self.remove_new(&id);
                    self.selection.clear();
                    return;
                }
                if let Some(fd) = e.and_then(|e| e.free_draw.clone()) {
                    if fd.points.len() == 1 {
                        // A click with the pen: a dot, as Excalidraw makes one.
                        self.change(&id, vec![update_points(&[[0.0, 0.0], [0.0001, 0.0001]])]);
                    }
                }
                self.commit();
                if !self.tool_locked && self.tool != Tool::FreeDraw {
                    self.tool = Tool::Selection;
                }
            }
            Some(Gesture::Marquee { .. }) | None => {}
            Some(_) => self.commit(),
        }
    }

    /// Whether a drag, resize or drawing is under way.
    pub fn is_busy(&self) -> bool {
        self.gesture.is_some()
    }

    /// Escape in the middle of a gesture: everything goes back.
    pub fn cancel_gesture(&mut self) {
        if self.gesture.take().is_some() {
            self.marquee = None;
            let ids: Vec<String> = self.scene.order();
            self.history.cancel(&mut self.scene);
            self.changes.ids.extend(ids);
            self.changes.order = true;
        }
    }

    // --- keyboard

    /// Returns true if the key did something.
    pub fn key(&mut self, key: Key, m: Modifiers) -> bool {
        if self.gesture.is_some() {
            if key == Key::Escape {
                self.cancel_gesture();
                return true;
            }
            return false;
        }
        match (key, m.ctrl, m.shift) {
            (Key::Char('z'), true, false) => self.undo(),
            (Key::Char('z'), true, true) | (Key::Char('y'), true, _) => self.redo(),
            (Key::Char('a'), true, _) => self.select_all(),
            (Key::Char('d'), true, _) => self.duplicate(),
            (Key::Char('g'), true, false) => self.group(),
            (Key::Char('g'), true, true) => self.ungroup(),
            (Key::Char(']'), true, false) => self.bring_forward(),
            (Key::Char('['), true, false) => self.send_backward(),
            (Key::Char(']'), true, true) | (Key::Char('}'), true, _) => self.bring_to_front(),
            (Key::Char('['), true, true) | (Key::Char('{'), true, _) => self.send_to_back(),
            (Key::Delete | Key::Backspace, _, _) => self.delete_selected(),
            (Key::Escape, _, _) => {
                self.selection.clear();
                self.tool = Tool::Selection;
            }
            (Key::Left | Key::Right | Key::Up | Key::Down, _, shift) => {
                let d = if shift { SHIFT_NUDGE } else { NUDGE };
                let (dx, dy) = match key {
                    Key::Left => (-d, 0.0),
                    Key::Right => (d, 0.0),
                    Key::Up => (0.0, -d),
                    _ => (0.0, d),
                };
                self.translate_selected(dx, dy);
            }
            (Key::Char(c), false, _) => {
                // Excalidraw's tool keys: letters, and the number row.
                self.tool = match c.to_ascii_lowercase() {
                    'v' | '1' => Tool::Selection,
                    'r' | '2' => Tool::Rectangle,
                    'd' | '3' => Tool::Diamond,
                    'o' | '4' => Tool::Ellipse,
                    'a' | '5' => Tool::Arrow,
                    'l' | '6' => Tool::Line,
                    'p' | '7' => Tool::FreeDraw,
                    'h' => Tool::Hand,
                    'q' => {
                        self.tool_locked = !self.tool_locked;
                        return true;
                    }
                    _ => return false,
                };
                if self.tool != Tool::Selection {
                    self.selection.clear();
                }
            }
            _ => return false,
        }
        true
    }

    // --- actions

    pub fn undo(&mut self) {
        let ids = self.history.undo(&mut self.scene);
        self.after_history(ids);
    }

    pub fn redo(&mut self) {
        let ids = self.history.redo(&mut self.scene);
        self.after_history(ids);
    }

    fn after_history(&mut self, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        self.dirty = true;
        self.changes.ids.extend(ids);
        self.changes.order = true;
        self.selection
            .retain(|id| self.scene.by_id(id).is_some_and(|e| !e.is_deleted));
    }

    pub fn translate_selected(&mut self, dx: f64, dy: f64) {
        let moves: Vec<(String, f64, f64)> = self
            .selected()
            .iter()
            .map(|e| (e.id.clone(), e.x, e.y))
            .collect();
        if moves.is_empty() {
            return;
        }
        self.begin();
        for (id, x, y) in moves {
            self.change(&id, vec![update_num("x", x + dx), update_num("y", y + dy)]);
        }
        self.commit();
    }

    pub fn delete_selected(&mut self) {
        let ids = self.selected_in_order();
        if ids.is_empty() {
            return;
        }
        self.begin();
        for id in &ids {
            self.change(id, vec![update("isDeleted", true)]);
        }
        self.commit();
        self.selection.clear();
    }

    /// Ctrl+G: one new group around the selection, outermost.
    pub fn group(&mut self) {
        let selected: Vec<(String, Vec<String>)> = self
            .selected()
            .iter()
            .map(|e| (e.id.clone(), e.group_ids.clone()))
            .collect();
        if selected.len() < 2 {
            return;
        }
        let group = random_id();
        self.begin();
        for (id, mut groups) in selected {
            groups.push(group.clone());
            self.change(&id, vec![update("groupIds", json!(groups))]);
        }
        self.commit();
    }

    /// Ctrl+Shift+G: the outermost group of each selected element undone.
    pub fn ungroup(&mut self) {
        let selected: Vec<(String, Vec<String>)> = self
            .selected()
            .iter()
            .filter(|e| !e.group_ids.is_empty())
            .map(|e| (e.id.clone(), e.group_ids.clone()))
            .collect();
        if selected.is_empty() {
            return;
        }
        self.begin();
        for (id, mut groups) in selected {
            groups.pop();
            self.change(&id, vec![update("groupIds", json!(groups))]);
        }
        self.commit();
    }

    fn restack(&mut self, at: usize) {
        let ids = self.selected_in_order();
        if ids.is_empty() {
            return;
        }
        self.begin();
        for id in self.scene.order() {
            self.history.touch(&self.scene, &id);
        }
        let before: Vec<(String, f64)> = self
            .scene
            .elements
            .iter()
            .map(|e| (e.id.clone(), e.version))
            .collect();
        self.scene.move_to(&ids, at);
        for (id, v) in before {
            if self.scene.by_id(&id).is_some_and(|e| e.version != v) {
                self.changes.ids.insert(id);
            }
        }
        self.changes.order = true;
        self.dirty = true;
        self.commit();
    }

    pub fn bring_to_front(&mut self) {
        self.restack(self.scene.elements.len());
    }

    pub fn send_to_back(&mut self) {
        self.restack(0);
    }

    /// One step up: past the first unselected element above the selection.
    pub fn bring_forward(&mut self) {
        let set: HashSet<String> = self.selection.iter().cloned().collect();
        let top = self
            .scene
            .elements
            .iter()
            .rposition(|e| set.contains(&e.id));
        if let Some(top) = top {
            if let Some(next) = self
                .scene
                .elements
                .iter()
                .skip(top + 1)
                .position(|e| !e.is_deleted && !set.contains(&e.id))
            {
                self.restack(top + 1 + next + 1);
            }
        }
    }

    /// One step down: below the first unselected element under the selection.
    pub fn send_backward(&mut self) {
        let set: HashSet<String> = self.selection.iter().cloned().collect();
        let bottom = self.scene.elements.iter().position(|e| set.contains(&e.id));
        if let Some(bottom) = bottom {
            if let Some(prev) = self.scene.elements[..bottom]
                .iter()
                .rposition(|e| !e.is_deleted && !set.contains(&e.id))
            {
                self.restack(prev);
            }
        }
    }

    // --- copies

    /// The selection as copies with new ids and new groups (nested groups
    /// kept apart from the originals'), each `offset` away.
    fn copies(elements: &[Map<String, Value>], offset: Point) -> Vec<Map<String, Value>> {
        let mut ids = std::collections::HashMap::new();
        let mut groups = std::collections::HashMap::new();
        for e in elements {
            if let Some(id) = e.get("id").and_then(Value::as_str) {
                ids.insert(id.to_owned(), random_id());
            }
        }
        elements
            .iter()
            .map(|e| {
                let mut c = e.clone();
                let new_id = c
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| ids.get(id))
                    .cloned()
                    .unwrap_or_else(random_id);
                c.insert("id".into(), json!(new_id));
                if let Some(Value::Array(gs)) = c.get("groupIds") {
                    let mapped: Vec<Value> = gs
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|g| {
                            json!(groups.entry(g.to_owned()).or_insert_with(random_id).clone())
                        })
                        .collect();
                    c.insert("groupIds".into(), Value::Array(mapped));
                }
                // A frame copied with its contents stays theirs; otherwise
                // the copy belongs to no frame.
                let frame = c
                    .get("frameId")
                    .and_then(Value::as_str)
                    .and_then(|f| ids.get(f))
                    .cloned();
                c.insert("frameId".into(), frame.map_or(Value::Null, Value::from));
                for key in ["x", "y"] {
                    let d = if key == "x" { offset[0] } else { offset[1] };
                    let v = c.get(key).and_then(Value::as_f64).unwrap_or(0.0) + d;
                    c.insert(key.into(), scene::edit::num(v));
                }
                c.insert("seed".into(), json!(scene::edit::random_integer()));
                c.insert("version".into(), json!(1));
                c.insert("versionNonce".into(), json!(0));
                c.insert("updated".into(), json!(scene::edit::now_ms()));
                c.insert("index".into(), Value::Null);
                // Bindings point at elements that were not copied with them.
                for key in ["boundElements", "startBinding", "endBinding", "containerId"] {
                    if c.contains_key(key) {
                        c.insert(key.into(), Value::Null);
                    }
                }
                c
            })
            .collect()
    }

    /// Ctrl+D: copies just above the originals, offset, and selected.
    pub fn duplicate(&mut self) {
        let ids = self.selected_in_order();
        if ids.is_empty() {
            return;
        }
        let raws: Vec<_> = ids.iter().filter_map(|id| self.scene.raw_of(id)).collect();
        let top = ids
            .iter()
            .filter_map(|id| self.scene.position(id))
            .max()
            .unwrap_or(0);
        self.begin();
        let mut new = Vec::new();
        for (i, raw) in Self::copies(&raws, [DUPLICATE_OFFSET, DUPLICATE_OFFSET])
            .into_iter()
            .enumerate()
        {
            if let Some(id) = self.add(top + 1 + i, raw) {
                new.push(id);
            }
        }
        self.commit();
        self.selection = new;
    }

    /// The selection in Excalidraw's clipboard format, files included.
    pub fn copy(&self) -> Option<String> {
        let ids = self.selected_in_order();
        if ids.is_empty() {
            return None;
        }
        let elements: Vec<Value> = ids
            .iter()
            .filter_map(|id| self.scene.raw_of(id))
            .map(Value::Object)
            .collect();
        let mut files = Map::new();
        for e in self.selected() {
            if let Some(file_id) = e.image.as_ref().and_then(|i| i.file_id.as_deref()) {
                if let Some(f) = self.scene.file_value(file_id) {
                    files.insert(file_id.to_owned(), f.clone());
                }
            }
        }
        Some(json!({ "type": CLIPBOARD_TYPE, "elements": elements, "files": files }).to_string())
    }

    pub fn cut(&mut self) -> Option<String> {
        let text = self.copy()?;
        self.delete_selected();
        Some(text)
    }

    /// Pastes Excalidraw clipboard JSON, centred on `at`. False if `text` is
    /// not that (plain text becomes a text element in phase 3).
    pub fn paste(&mut self, text: &str, at: Point) -> bool {
        let Ok(Value::Object(data)) = serde_json::from_str::<Value>(text) else {
            return false;
        };
        if data.get("type").and_then(Value::as_str) != Some(CLIPBOARD_TYPE) {
            return false;
        }
        let Some(Value::Array(items)) = data.get("elements") else {
            return false;
        };
        let raws: Vec<Map<String, Value>> = items
            .iter()
            .filter_map(|v| v.as_object().cloned())
            .filter(|e| !e.get("isDeleted").and_then(Value::as_bool).unwrap_or(false))
            .collect();
        if raws.is_empty() {
            return false;
        }
        // Where they were, to centre them on the pointer.
        let probe = Scene::parse(&json!({ "type": "excalidraw", "elements": raws }).to_string());
        let Some(b) = probe
            .ok()
            .and_then(|s| bounds_of(s.visible()))
        else {
            return false;
        };
        let c = geometry::center(b);
        let offset = [at[0] - c[0], at[1] - c[1]];
        self.begin();
        if let Some(Value::Object(files)) = data.get("files") {
            for (id, f) in files {
                self.scene.add_file(id, f.clone());
            }
        }
        let mut new = Vec::new();
        for raw in Self::copies(&raws, offset) {
            if let Some(id) = self.add(self.scene.elements.len(), raw) {
                new.push(id);
            }
        }
        self.commit();
        self.selection = new;
        true
    }
}

impl Editor {
    #[cfg(test)]
    fn ids(&self) -> Vec<String> {
        self.scene.visible().map(|e| e.id.clone()).collect()
    }
}

#[cfg(test)]
mod tests;
