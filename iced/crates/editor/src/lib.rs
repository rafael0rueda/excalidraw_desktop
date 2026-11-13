//! Editing an Excalidraw drawing, with no UI: the app feeds pointer and key
//! events in scene coordinates and draws what this reports.
//!
//! Behaviour follows Excalidraw 0.18.1 where it is visible to the user:
//! the shortcuts, what a click picks (`shouldTestInside`), that a group is
//! selected whole, that tiny new shapes vanish (`isInvisiblySmallElement`),
//! the clipboard format, and the nudge distances.

#![forbid(unsafe_code)]

mod binding;
pub mod geometry;
mod history;
pub mod text;
mod transform;

use geometry::{bounds_of, contains, Bounds};
use history::History;
use rough::Point;
use scene::edit::{
    new_element, new_text_element, random_id, update, update_num, update_points, Style, Updates,
};
use scene::{Element, Kind, Scene};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
pub use text::Measure;
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
    Text,
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
    Enter,
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
    /// Dragging one point of a line or arrow.
    Point {
        id: String,
        index: usize,
    },
}

/// A handle of the line editor: a point, or the middle of a segment
/// (dragging it adds a point there).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointHandle {
    Point(usize, Point),
    Mid(usize, Point),
}

/// Point handles' radius on screen.
const POINT_HANDLE_PX: f64 = 5.0;

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
    /// The shape an arrow being drawn would bind to, for the overlay.
    pub binding_hint: Option<String>,
    /// The line point last picked, which Delete removes.
    pub selected_point: Option<usize>,
    /// The text being typed, if any: the app shows its own text box over it.
    pub editing: Option<TextEdit>,
    /// `currentItemFontFamily`, `currentItemFontSize`, `currentItemTextAlign`.
    pub font_family: f64,
    pub font_size: f64,
    pub text_align: String,
    measure: Box<dyn Measure>,
}

/// A text element being typed into.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEdit {
    pub id: String,
    /// The shape it is the label of.
    pub container: Option<String>,
    /// Made for this edit: if it ends empty, it was never there.
    pub is_new: bool,
}

/// A stand-in for real text measurement: every character half as wide as
/// the font is tall. The app replaces it with shaping (`set_measure`).
struct Approx;
impl Measure for Approx {
    fn line_width(&self, line: &str, _family: f64, size: f64) -> f64 {
        line.chars().count() as f64 * size * 0.5
    }
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
            binding_hint: None,
            selected_point: None,
            editing: None,
            font_family: 5.0,
            font_size: 20.0,
            text_align: "left".into(),
            measure: Box::new(Approx),
        }
    }

    /// How text is measured: the app shapes it with the real fonts.
    pub fn set_measure(&mut self, measure: Box<dyn Measure>) {
        self.measure = measure;
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

    /// Marks the drawing as differing from its file: a recovered snapshot.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
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
    /// A label picks the shape it belongs to, as in Excalidraw.
    pub fn element_at(&self, p: Point) -> Option<&Element> {
        let threshold = HIT_PX / self.zoom;
        let hit = self
            .scene
            .visible()
            .filter(|e| geometry::hit(e, p, threshold))
            .last()?;
        match hit.text.as_ref().and_then(|t| t.container_id.as_deref()) {
            Some(container) => self.visible(container).or(Some(hit)),
            None => Some(hit),
        }
    }

    /// The label inside `container`, if it has a live one.
    pub fn bound_text_of(&self, container: &str) -> Option<String> {
        let c = self.scene.by_id(container)?;
        let Some(Value::Array(bound)) = c.raw().get("boundElements") else {
            return None;
        };
        bound
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("id").and_then(Value::as_str))
            .find(|id| self.scene.by_id(id).is_some_and(|t| t.text.is_some()))
            .map(str::to_owned)
    }

    /// What "export the selection" covers (the Tauri app's `expandSelection`):
    /// the selection, what is inside a selected frame, and the labels of
    /// both.
    pub fn export_selection(&self) -> HashSet<String> {
        let mut ids: HashSet<String> = self.selected().iter().map(|e| e.id.clone()).collect();
        let framed: Vec<String> = self
            .scene
            .visible()
            .filter(|e| e.frame_id.as_ref().is_some_and(|f| ids.contains(f)))
            .map(|e| e.id.clone())
            .collect();
        ids.extend(framed);
        let labels: Vec<String> = self
            .scene
            .visible()
            .filter(|e| e.text.as_ref().and_then(|t| t.container_id.as_ref()).is_some_and(|c| ids.contains(c)))
            .map(|e| e.id.clone())
            .collect();
        ids.extend(labels);
        ids
    }

    /// `ids` and the labels of any of them that have one.
    fn with_labels(&self, ids: Vec<String>) -> Vec<String> {
        let mut out = ids.clone();
        for id in &ids {
            if let Some(t) = self.bound_text_of(id) {
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
        out
    }

    // --- pointer

    pub fn cursor_at(&self, p: Point) -> Cursor {
        match self.tool {
            Tool::Hand => return Cursor::Grab,
            Tool::Selection => {}
            _ => return Cursor::Crosshair,
        }
        if self.point_handle_at(p).is_some() {
            return Cursor::Move;
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
        // A picked line point stays picked only by being pressed again.
        self.selected_point = None;
        if self.editing.is_some() {
            // A click elsewhere ends the typing; it does nothing else.
            self.finish_text();
            return;
        }
        if self.tool == Tool::Text {
            self.start_text(p);
            if !self.tool_locked {
                self.tool = Tool::Selection;
            }
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
        if let Some(handle) = self.point_handle_at(p) {
            let Some(id) = self.selection.first().cloned() else {
                return;
            };
            self.begin();
            let index = match handle {
                PointHandle::Point(i, _) => i,
                PointHandle::Mid(i, at) => {
                    // A new point where the middle handle was.
                    let Some(e) = self.scene.by_id(&id).cloned() else {
                        return;
                    };
                    let Some(mut points) = e.linear.as_ref().map(|l| l.points.clone()) else {
                        return;
                    };
                    points.insert(i + 1, geometry::to_local(&e, at));
                    self.set_points(&id, points);
                    i + 1
                }
            };
            self.selected_point = Some(index);
            self.gesture = Some(Gesture::Point { id, index });
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
        let ids = self.with_labels(self.selected_in_order());
        let origins = ids
            .iter()
            .filter_map(|id| self.scene.by_id(id))
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
                let moving: HashSet<String> = origins.iter().map(|o| o.0.clone()).collect();
                for id in &moving {
                    self.update_bound_arrows(id, &moving);
                }
                self.gesture = Some(Gesture::Drag { start, origins });
            }
            Some(Gesture::Resize(r)) => {
                let updates = r.updates(p, m);
                let moving: HashSet<String> = updates.iter().map(|u| u.0.clone()).collect();
                for (id, updates) in updates {
                    self.change(&id, updates);
                    // `handleBindTextResize`: the label is wrapped again.
                    self.sync_label(&id, true);
                    self.update_bound_arrows(&id, &moving);
                }
                self.gesture = Some(Gesture::Resize(r));
            }
            Some(Gesture::Rotate(r)) => {
                let updates = r.updates(p, m);
                let moving: HashSet<String> = updates.iter().map(|u| u.0.clone()).collect();
                for (id, updates) in updates {
                    self.change(&id, updates);
                    self.sync_label(&id, false);
                    self.update_bound_arrows(&id, &moving);
                }
                self.gesture = Some(Gesture::Rotate(r));
            }
            Some(Gesture::Point { id, index }) => {
                self.move_point(&id, index, p, m);
                self.gesture = Some(Gesture::Point { id, index });
            }
            Some(Gesture::Create { id, origin }) => {
                self.create_to(&id, origin, p, m);
                // What the arrow's end would bind to, shown while drawing.
                self.binding_hint = self
                    .scene
                    .by_id(&id)
                    .filter(|e| e.kind == Kind::Arrow)
                    .and_then(|e| {
                        e.linear
                            .as_ref()
                            .and_then(|l| l.points.last())
                            .map(|q| geometry::to_scene(e, *q))
                    })
                    .and_then(|q| self.bindable_at(q, &id));
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
        self.binding_hint = None;
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
                // `bindOrUnbindLinearElement`: both ends, to what they are on.
                if self.scene.by_id(&id).is_some_and(|e| e.kind == Kind::Arrow) {
                    self.rebind(&id);
                }
                self.commit();
                if !self.tool_locked && self.tool != Tool::FreeDraw {
                    self.tool = Tool::Selection;
                }
            }
            Some(Gesture::Marquee { .. }) | None => {}
            Some(Gesture::Point { id, index }) => {
                // An arrow's end let go onto a shape binds to it.
                let is_end = self
                    .scene
                    .by_id(&id)
                    .and_then(|e| {
                        e.linear
                            .as_ref()
                            .map(|l| index == 0 || index + 1 == l.points.len())
                    })
                    .unwrap_or(false);
                if is_end {
                    self.rebind(&id);
                }
                self.commit();
            }
            Some(Gesture::Drag { origins, .. }) => {
                // An arrow moved without the shapes it is bound to binds to
                // whatever its ends are on now, or to nothing.
                let moving: HashSet<String> = origins.iter().map(|o| o.0.clone()).collect();
                for id in &moving {
                    let bound_elsewhere = self.scene.by_id(id).is_some_and(|e| {
                        e.kind == Kind::Arrow
                            && self
                                .bindings_of(e)
                                .iter()
                                .any(|(_, target)| !moving.contains(target))
                    });
                    let unbound_arrow = self
                        .scene
                        .by_id(id)
                        .is_some_and(|e| e.kind == Kind::Arrow && self.bindings_of(e).is_empty());
                    if bound_elsewhere || unbound_arrow {
                        self.rebind(id);
                    }
                }
                self.commit();
            }
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
        if self.editing.is_some() {
            // Typing goes to the app's text box; only these reach here.
            if key == Key::Escape || (key == Key::Enter && m.ctrl) {
                self.finish_text();
                return true;
            }
            return false;
        }
        match (key, m.ctrl, m.shift) {
            (Key::Enter, false, _) => {
                // Enter on one selected text or shape: type into it.
                let [id] = self.selection.as_slice() else {
                    return false;
                };
                let id = id.clone();
                return self.edit_at_element(&id);
            }
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
            (Key::Delete | Key::Backspace, _, _) => {
                if !self.delete_selected_point() {
                    self.delete_selected();
                }
            }
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
                    't' | '8' => Tool::Text,
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
        let ids = self.with_labels(self.selected_in_order());
        let moves: Vec<(String, f64, f64)> = ids
            .iter()
            .filter_map(|id| self.scene.by_id(id))
            .map(|e| (e.id.clone(), e.x, e.y))
            .collect();
        if moves.is_empty() {
            return;
        }
        self.begin();
        let moving: HashSet<String> = moves.iter().map(|m| m.0.clone()).collect();
        for (id, x, y) in moves {
            self.change(&id, vec![update_num("x", x + dx), update_num("y", y + dy)]);
        }
        for id in &moving {
            self.update_bound_arrows(id, &moving);
        }
        self.commit();
    }

    pub fn delete_selected(&mut self) {
        let ids = self.with_labels(self.selected_in_order());
        if ids.is_empty() {
            return;
        }
        self.begin();
        for id in &ids {
            self.change(id, vec![update("isDeleted", true)]);
        }
        // `fixBindingsAfterDeletion`: arrows let go of deleted shapes, and
        // shapes forget deleted arrows.
        for id in &ids {
            let Some(e) = self.scene.by_id(id).cloned() else {
                continue;
            };
            if e.kind == Kind::Arrow {
                for (_, target) in self.bindings_of(&e) {
                    self.remove_bound(&target, id);
                }
            }
            for arrow in self.bound_arrows_of(id) {
                if ids.contains(&arrow) {
                    continue;
                }
                if let Some(a) = self.scene.by_id(&arrow).cloned() {
                    for (start, target) in self.bindings_of(&a) {
                        if target == *id {
                            let key = if start { "startBinding" } else { "endBinding" };
                            self.change(&arrow, vec![update(key, Value::Null)]);
                        }
                    }
                }
            }
        }
        self.commit();
        self.selection.clear();
    }

    // --- the line editor

    /// The line or arrow whose points can be edited: the only one selected.
    fn point_edited(&self) -> Option<&Element> {
        let [id] = self.selection.as_slice() else {
            return None;
        };
        self.visible(id)
            .filter(|e| e.linear.as_ref().is_some_and(|l| !l.elbowed))
    }

    /// The handles of the line editor, for drawing them.
    pub fn point_handles(&self) -> Vec<PointHandle> {
        let Some(e) = self.point_edited() else {
            return Vec::new();
        };
        let Some(l) = e.linear.as_ref() else {
            return Vec::new();
        };
        let mut out: Vec<PointHandle> = l
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| PointHandle::Point(i, geometry::to_scene(e, *p)))
            .collect();
        for i in 0..l.points.len().saturating_sub(1) {
            let mid = geometry::segment_mid(e, i);
            // Only where the segment is long enough to tell it from its ends.
            let long = geometry::distance(
                geometry::to_scene(e, l.points[i]),
                geometry::to_scene(e, l.points[i + 1]),
            ) * self.zoom
                > POINT_HANDLE_PX * 6.0;
            if let (Some(m), true) = (mid, long) {
                out.push(PointHandle::Mid(i, m));
            }
        }
        out
    }

    fn point_handle_at(&self, p: Point) -> Option<PointHandle> {
        let reach = (POINT_HANDLE_PX + 3.0) / self.zoom;
        self.point_handles().into_iter().find(|h| {
            let (PointHandle::Point(_, at) | PointHandle::Mid(_, at)) = *h;
            geometry::distance(at, p) <= reach
        })
    }

    /// Sets a line's points, keeping its first point at its origin by moving
    /// the element instead, as `LinearElementEditor` does.
    fn set_points(&mut self, id: &str, points: Vec<Point>) {
        let Some(e) = self.scene.by_id(id).cloned() else {
            return;
        };
        let o = points[0];
        let points: Vec<Point> = points.iter().map(|p| [p[0] - o[0], p[1] - o[1]]).collect();
        self.change(
            id,
            vec![
                update_points(&points),
                update_num("x", e.x + o[0]),
                update_num("y", e.y + o[1]),
            ],
        );
        self.sync_label(id, false);
    }

    fn move_point(&mut self, id: &str, index: usize, p: Point, m: Modifiers) {
        let Some(e) = self.scene.by_id(id).cloned() else {
            return;
        };
        let Some(mut points) = e.linear.as_ref().map(|l| l.points.clone()) else {
            return;
        };
        if index >= points.len() {
            return;
        }
        let mut q = geometry::to_local(&e, p);
        if m.shift && points.len() > 1 {
            // 15° steps from the neighbouring point.
            let n = points[if index == 0 { 1 } else { index - 1 }];
            let (dx, dy) = (q[0] - n[0], q[1] - n[1]);
            let step = std::f64::consts::PI / 12.0;
            let a = (dy.atan2(dx) / step).round() * step;
            let len = dx.hypot(dy);
            q = [n[0] + len * a.cos(), n[1] + len * a.sin()];
        }
        points[index] = q;
        self.set_points(id, points);
        let is_end = index == 0 || index + 1 == e.linear.as_ref().map_or(0, |l| l.points.len());
        self.binding_hint = (e.kind == Kind::Arrow && is_end)
            .then(|| self.bindable_at(p, id))
            .flatten();
    }

    /// Delete with a line point picked: that point goes, if two are left.
    fn delete_selected_point(&mut self) -> bool {
        let (Some(i), Some(e)) = (self.selected_point, self.point_edited().cloned()) else {
            return false;
        };
        let Some(mut points) = e.linear.as_ref().map(|l| l.points.clone()) else {
            return false;
        };
        if points.len() <= 2 || i >= points.len() {
            return false;
        }
        points.remove(i);
        self.begin();
        self.set_points(&e.id, points);
        if e.kind == Kind::Arrow
            && (i == 0 || i == e.linear.as_ref().map_or(0, |l| l.points.len()) - 1)
        {
            self.rebind(&e.id);
        }
        self.commit();
        self.selected_point = None;
        true
    }

    // --- bindings

    /// The topmost bindable element an arrow end at `p` would bind to.
    fn bindable_at(&self, p: Point, exclude: &str) -> Option<String> {
        self.scene
            .visible()
            .filter(|e| e.id != exclude && binding::catches(e, p, self.zoom))
            .last()
            .map(|e| e.id.clone())
    }

    /// An arrow's bindings: (is it the start, the element it is bound to).
    fn bindings_of(&self, arrow: &Element) -> Vec<(bool, String)> {
        [(true, "startBinding"), (false, "endBinding")]
            .into_iter()
            .filter_map(|(start, key)| {
                Some((
                    start,
                    arrow.raw().get(key)?.get("elementId")?.as_str()?.to_owned(),
                ))
            })
            .collect()
    }

    fn bound_arrows_of(&self, id: &str) -> Vec<String> {
        let Some(Value::Array(bound)) = self
            .scene
            .by_id(id)
            .and_then(|e| e.raw().get("boundElements"))
        else {
            return Vec::new();
        };
        bound
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("arrow"))
            .filter_map(|b| b.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect()
    }

    fn remove_bound(&mut self, target: &str, arrow: &str) {
        let Some(Value::Array(bound)) = self
            .scene
            .by_id(target)
            .and_then(|e| e.raw().get("boundElements"))
            .cloned()
        else {
            return;
        };
        let kept: Vec<Value> = bound
            .into_iter()
            .filter(|b| b.get("id").and_then(Value::as_str) != Some(arrow))
            .collect();
        self.change(
            target,
            vec![update(
                "boundElements",
                if kept.is_empty() {
                    Value::Null
                } else {
                    Value::Array(kept)
                },
            )],
        );
    }

    /// `bindOrUnbindLinearElement`: binds each end of an arrow to the
    /// element it is on, or unbinds it.
    fn rebind(&mut self, id: &str) {
        let Some(a) = self.scene.by_id(id).cloned() else {
            return;
        };
        let Some(l) = a.linear.clone() else { return };
        if a.kind != Kind::Arrow || l.elbowed || l.points.len() < 2 {
            return;
        }
        let start_target = self.bindable_at(geometry::to_scene(&a, l.points[0]), id);
        let mut end_target =
            self.bindable_at(geometry::to_scene(&a, l.points[l.points.len() - 1]), id);
        // `isLinearElementSimpleAndAlreadyBound`: a straight arrow does not
        // bind both ends to the same element.
        if l.points.len() == 2 && start_target.is_some() && start_target == end_target {
            end_target = None;
        }
        let old = self.bindings_of(&a);
        for (start, target) in [(true, start_target), (false, end_target)] {
            let key = if start { "startBinding" } else { "endBinding" };
            let previous = old
                .iter()
                .find(|(s, _)| *s == start)
                .map(|(_, t)| t.clone());
            if let Some(p) = &previous {
                // Unless the other end stays bound to it.
                let other_keeps = old.iter().any(|(s, t)| *s != start && t == p);
                if previous != target && !other_keeps {
                    self.remove_bound(p, id);
                }
            }
            match target {
                Some(t) => {
                    let (Some(arrow), Some(el)) =
                        (self.scene.by_id(id).cloned(), self.scene.by_id(&t).cloned())
                    else {
                        continue;
                    };
                    let Some((focus, gap)) = binding::focus_and_gap(&arrow, start, &el) else {
                        continue;
                    };
                    self.change(id, vec![update(key, json!({ "elementId": t, "focus": scene::edit::num(focus), "gap": scene::edit::num(gap) }))]);
                    if !self.bound_arrows_of(&t).contains(&id.to_owned()) {
                        let mut bound = match el.raw().get("boundElements") {
                            Some(Value::Array(b)) => b.clone(),
                            _ => Vec::new(),
                        };
                        bound.push(json!({ "id": id, "type": "arrow" }));
                        self.change(&t, vec![update("boundElements", Value::Array(bound))]);
                    }
                }
                None => {
                    self.change(id, vec![update(key, Value::Null)]);
                }
            }
        }
    }

    /// `updateBoundElements`: the arrows bound to `id` follow it, except
    /// those moving with it (`moving`).
    fn update_bound_arrows(&mut self, id: &str, moving: &HashSet<String>) {
        let Some(el) = self.scene.by_id(id).cloned() else {
            return;
        };
        if !binding::is_bindable(&el) {
            return;
        }
        for arrow_id in self.bound_arrows_of(id) {
            if moving.contains(&arrow_id) {
                continue;
            }
            let Some(a) = self
                .scene
                .by_id(&arrow_id)
                .cloned()
                .filter(|a| !a.is_deleted)
            else {
                continue;
            };
            let Some(mut points) = a.linear.as_ref().map(|l| l.points.clone()) else {
                continue;
            };
            for key in ["startBinding", "endBinding"] {
                let start = key == "startBinding";
                let Some(b) = a
                    .raw()
                    .get(key)
                    .filter(|b| b.get("elementId").and_then(Value::as_str) == Some(id))
                else {
                    continue;
                };
                let focus = b.get("focus").and_then(Value::as_f64).unwrap_or(0.0);
                let gap = b.get("gap").and_then(Value::as_f64).unwrap_or(0.0);
                if let Some(q) = binding::bound_point(&a, start, focus, gap, &el) {
                    let i = if start { 0 } else { points.len() - 1 };
                    points[i] = geometry::to_local(&a, q);
                }
            }
            // The first point stays at the origin: the arrow moves instead.
            let o = points[0];
            let points: Vec<Point> = points.iter().map(|p| [p[0] - o[0], p[1] - o[1]]).collect();
            self.change(
                &arrow_id,
                vec![
                    update_points(&points),
                    update_num("x", a.x + o[0]),
                    update_num("y", a.y + o[1]),
                ],
            );
            self.sync_label(&arrow_id, false);
        }
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

    /// `ids` in stacking order.
    fn in_order(&self, ids: Vec<String>) -> Vec<String> {
        let set: HashSet<String> = ids.into_iter().collect();
        self.scene
            .order()
            .into_iter()
            .filter(|id| set.contains(id))
            .collect()
    }

    // --- text

    fn font_of(e: &Element) -> text::Font {
        let t = e.text.as_ref();
        text::Font {
            family: t.map_or(5.0, |t| t.font_family),
            size: t.map_or(20.0, |t| t.font_size),
            line_height: t.map_or(1.25, |t| t.line_height),
        }
    }

    /// Lays a shape's label out again after the shape changed: re-wrapped to
    /// its width when `rewrap`, the shape grown to fit it if need be, and the
    /// label placed and turned as the shape is (`handleBindTextResize`,
    /// `computeBoundTextPosition`).
    fn sync_label(&mut self, container: &str, rewrap: bool) {
        let Some(tid) = self.bound_text_of(container) else {
            return;
        };
        let (Some(c), Some(t)) = (
            self.scene.by_id(container).cloned(),
            self.scene.by_id(&tid).cloned(),
        ) else {
            return;
        };
        let Some(tv) = t.text.clone() else { return };
        let font = Self::font_of(&t);
        let original = t
            .raw()
            .get("originalText")
            .and_then(Value::as_str)
            .unwrap_or(&tv.text)
            .to_owned();
        let wrapped = if rewrap {
            text::wrap(
                &original,
                font,
                text::max_width(&c, font.size),
                self.measure.as_ref(),
            )
        } else {
            tv.text.clone()
        };
        let (w, h) = text::measure(&wrapped, font, self.measure.as_ref());
        if c.kind != Kind::Arrow && h > text::max_height(&c, h) {
            let height = text::container_dimension(h, c.kind);
            self.change(container, vec![update_num("height", height)]);
        }
        let Some(c) = self.scene.by_id(container).cloned() else {
            return;
        };
        // An arrow's label sits on its middle, upright; a shape's is placed
        // inside it and turns with it.
        let ((x, y), angle) = if c.kind == Kind::Arrow {
            let Some(m) = geometry::arrow_label_center(&c) else {
                return;
            };
            ((m[0] - w / 2.0, m[1] - h / 2.0), 0.0)
        } else {
            (
                text::bound_position(&c, &tv.text_align, &tv.vertical_align, w, h),
                c.angle,
            )
        };
        self.change(
            &tid,
            vec![
                update("text", wrapped),
                update_num("width", w),
                update_num("height", h),
                update_num("x", x),
                update_num("y", y),
                update_num("angle", angle),
            ],
        );
    }

    /// Starts typing a new text at `p`, its first line's top left.
    pub fn start_text(&mut self, p: Point) {
        self.finish_text();
        self.begin();
        let raw = new_text_element(
            p[0],
            p[1],
            &self.style,
            self.font_family,
            self.font_size,
            &self.text_align,
            "top",
            None,
        );
        if let Some(id) = self.add(self.scene.elements.len(), raw) {
            self.selection.clear();
            self.editing = Some(TextEdit {
                id,
                container: None,
                is_new: true,
            });
        }
    }

    /// Starts typing into a text element, or into the label of a shape,
    /// making the label if it has none. False for anything else.
    pub fn edit_at_element(&mut self, id: &str) -> bool {
        let Some(e) = self.visible(id).cloned() else {
            return false;
        };
        self.finish_text();
        if e.text.is_some() {
            let container = e.text.as_ref().and_then(|t| t.container_id.clone());
            self.begin();
            self.history.touch(&self.scene, id);
            self.changes.ids.insert(id.to_owned());
            self.editing = Some(TextEdit {
                id: id.to_owned(),
                container,
                is_new: false,
            });
            return true;
        }
        // `isTextBindableContainer`: shapes, and arrows (not lines).
        if !matches!(
            e.kind,
            Kind::Rectangle | Kind::Diamond | Kind::Ellipse | Kind::Arrow
        ) {
            return false;
        }
        if let Some(tid) = self.bound_text_of(id) {
            return self.edit_at_element(&tid);
        }
        self.begin();
        let c = geometry::center(geometry::bounds(&e));
        let raw = new_text_element(
            c[0],
            c[1],
            &self.style,
            self.font_family,
            self.font_size,
            "center",
            "middle",
            Some(id),
        );
        let at = self
            .scene
            .position(id)
            .map_or(self.scene.elements.len(), |i| i + 1);
        let Some(tid) = self.add(at, raw) else {
            return false;
        };
        let mut bound = match e.raw().get("boundElements") {
            Some(Value::Array(b)) => b.clone(),
            _ => Vec::new(),
        };
        bound.push(json!({ "type": "text", "id": tid }));
        self.change(id, vec![update("boundElements", Value::Array(bound))]);
        self.change(
            &tid,
            vec![update_num(
                "angle",
                if e.kind == Kind::Arrow { 0.0 } else { e.angle },
            )],
        );
        self.editing = Some(TextEdit {
            id: tid,
            container: Some(id.to_owned()),
            is_new: true,
        });
        true
    }

    /// A double click: type into what is there, or start new text.
    pub fn double_click(&mut self, p: Point) {
        if self.tool != Tool::Selection {
            return;
        }
        let hit = self.element_at(p).map(|e| e.id.clone());
        match hit {
            Some(id) => {
                self.edit_at_element(&id);
            }
            None => self.start_text(p),
        }
    }

    /// The text element being typed into, and the text typed so far.
    pub fn editing_text(&self) -> Option<(&Element, String)> {
        let edit = self.editing.as_ref()?;
        let e = self.scene.by_id(&edit.id)?;
        let original = e
            .raw()
            .get("originalText")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Some((e, original))
    }

    /// What has been typed so far: the element follows it (measured, and
    /// wrapped inside a shape) as Excalidraw's text box does.
    pub fn set_text(&mut self, typed: &str) {
        let Some(edit) = self.editing.clone() else {
            return;
        };
        let Some(e) = self.scene.by_id(&edit.id).cloned() else {
            return;
        };
        let typed = text::normalize(typed);
        if let Some(container) = &edit.container {
            self.change(
                &edit.id,
                vec![update("originalText", typed.clone()), update("text", typed)],
            );
            self.sync_label(container, true);
            return;
        }
        let font = Self::font_of(&e);
        let (w, h) = text::measure(&typed, font, self.measure.as_ref());
        // Left-aligned text grows to the right; centred grows both ways.
        let dx = match e.text.as_ref().map_or("left", |t| t.text_align.as_str()) {
            "center" => (w - e.width) / 2.0,
            "right" => w - e.width,
            _ => 0.0,
        };
        self.change(
            &edit.id,
            vec![
                update("text", typed.clone()),
                update("originalText", typed),
                update_num("width", w),
                update_num("height", h),
                update_num("x", e.x - dx),
            ],
        );
    }

    /// Ends the typing. Text left empty is no text: a new one leaves no
    /// trace, an old one is deleted (with its place in its shape).
    pub fn finish_text(&mut self) {
        let Some(edit) = self.editing.take() else {
            return;
        };
        self.changes.ids.insert(edit.id.clone());
        let empty = self
            .scene
            .by_id(&edit.id)
            .and_then(|e| {
                e.raw()
                    .get("originalText")
                    .and_then(Value::as_str)
                    .map(|t| t.trim().is_empty())
            })
            .unwrap_or(true);
        if empty && edit.is_new {
            let ids = self.scene.order();
            self.history.cancel(&mut self.scene);
            self.remove_new(&edit.id);
            self.changes.ids.extend(ids);
            return;
        }
        if empty {
            self.change(&edit.id, vec![update("isDeleted", true)]);
            if let Some(container) = &edit.container {
                if let Some(Value::Array(bound)) = self
                    .scene
                    .by_id(container)
                    .and_then(|c| c.raw().get("boundElements"))
                    .cloned()
                {
                    let kept: Vec<Value> = bound
                        .into_iter()
                        .filter(|b| b.get("id").and_then(Value::as_str) != Some(&edit.id))
                        .collect();
                    self.change(
                        container,
                        vec![update(
                            "boundElements",
                            if kept.is_empty() {
                                Value::Null
                            } else {
                                Value::Array(kept)
                            },
                        )],
                    );
                }
            }
        }
        self.commit();
        self.selection = match (&edit.container, empty) {
            (Some(c), _) => vec![c.clone()],
            (None, false) => vec![edit.id.clone()],
            (None, true) => Vec::new(),
        };
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
                // Bindings follow the copies when both ends were copied, and
                // are dropped when not.
                if let Some(container) = c.get("containerId").and_then(Value::as_str) {
                    let mapped = ids.get(container).cloned();
                    c.insert(
                        "containerId".into(),
                        mapped.map_or(Value::Null, Value::from),
                    );
                }
                if let Some(Value::Array(bound)) = c.get("boundElements").cloned() {
                    let kept: Vec<Value> = bound
                        .iter()
                        .filter_map(|b| {
                            let new = ids.get(b.get("id")?.as_str()?)?;
                            let mut b = b.clone();
                            b["id"] = json!(new);
                            Some(b)
                        })
                        .collect();
                    c.insert(
                        "boundElements".into(),
                        if kept.is_empty() {
                            Value::Null
                        } else {
                            Value::Array(kept)
                        },
                    );
                }
                for key in ["startBinding", "endBinding"] {
                    let mapped = c.get(key).and_then(|b| {
                        let new = ids.get(b.get("elementId")?.as_str()?)?;
                        let mut b = b.clone();
                        b["elementId"] = json!(new);
                        Some(b)
                    });
                    if c.contains_key(key) {
                        c.insert(key.into(), mapped.unwrap_or(Value::Null));
                    }
                }
                c
            })
            .collect()
    }

    /// Ctrl+D: copies just above the originals, offset, and selected.
    pub fn duplicate(&mut self) {
        let ids = self.in_order(self.with_labels(self.selected_in_order()));
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
        let ids = self.in_order(self.with_labels(self.selected_in_order()));
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
        let files = match data.get("files") {
            Some(Value::Object(files)) => Some(files.clone()),
            _ => None,
        };
        self.insert(items, files.as_ref(), at)
    }

    /// Copies of `items` (Excalidraw elements) added on top, centred on
    /// `at` and selected: a paste, or a library item put on the canvas.
    /// New ids, groups and seeds; bindings kept between the copies.
    pub fn insert(&mut self, items: &[Value], files: Option<&Map<String, Value>>, at: Point) -> bool {
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
        let Some(b) = probe.ok().and_then(|s| bounds_of(s.visible())) else {
            return false;
        };
        let c = geometry::center(b);
        let offset = [at[0] - c[0], at[1] - c[1]];
        self.begin();
        for (id, f) in files.into_iter().flatten() {
            self.scene.add_file(id, f.clone());
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

    /// The selection's elements as they are, labels and framed elements
    /// included, bottom first: what "Add to library" keeps.
    pub fn selected_elements(&self) -> Vec<Value> {
        let ids = self.export_selection();
        self.scene
            .visible()
            .filter(|e| ids.contains(&e.id))
            .map(|e| Value::Object(e.raw().clone()))
            .collect()
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
