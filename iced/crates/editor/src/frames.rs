//! Frames, as Excalidraw 0.18.1 keeps them: an element of type `frame`
//! whose children carry its id in `frameId`.
//!
//! - Drawn with the frame tool (F) in `FRAME_STYLE`; what lies wholly
//!   inside it then becomes its (`getElementsInNewFrame`: whole groups
//!   only, nothing already in another frame).
//! - Moved, deleted, copied, duplicated and erased with what it holds.
//! - Elements dragged onto a frame join it; dragged out of their frame,
//!   they leave it (`updateFrameMembershipOfSelectedElements`).
//! - Picked by its border or its name; renamed by double-clicking the name.

use crate::geometry::{bounds, bounds_of, Bounds};
use crate::Editor;
use rough::Point;
use scene::edit::update;
use scene::{Element, Kind};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

/// `FRAME_STYLE.nameFontSize`, `nameLineHeight`, `nameOffsetY`: the name
/// sits above the frame, a fixed size on screen.
const NAME_HEIGHT_PX: f64 = 14.0 * 1.25 + 3.0;

fn is_frame(e: &Element) -> bool {
    matches!(e.kind, Kind::Frame | Kind::MagicFrame)
}

fn within(inner: Bounds, outer: Bounds) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

fn overlaps(a: Bounds, b: Bounds) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// `FRAME_STYLE` over a new element's fields, and `name` after them.
pub(crate) fn frame_style(raw: &mut Map<String, Value>) {
    raw.insert("strokeColor".into(), json!("#bbb"));
    raw.insert("backgroundColor".into(), json!("transparent"));
    raw.insert("fillStyle".into(), json!("solid"));
    raw.insert("strokeWidth".into(), json!(2));
    raw.insert("strokeStyle".into(), json!("solid"));
    raw.insert("roughness".into(), json!(0));
    raw.insert("roundness".into(), Value::Null);
    raw.insert("name".into(), Value::Null);
}

impl Editor {
    /// What is in frame `id`.
    pub fn frame_children(&self, id: &str) -> Vec<String> {
        self.scene.visible().filter(|e| e.frame_id.as_deref() == Some(id)).map(|e| e.id.clone()).collect()
    }

    /// `ids` with the contents of the frames among them.
    pub(crate) fn with_frames(&self, ids: Vec<String>) -> Vec<String> {
        let mut out = ids.clone();
        for id in &ids {
            if self.visible(id).is_some_and(is_frame) {
                for c in self.frame_children(id) {
                    if !out.contains(&c) {
                        out.push(c);
                    }
                }
            }
        }
        out
    }

    /// What an action on the selection takes along: frame contents, labels.
    pub(crate) fn with_contents(&self, ids: Vec<String>) -> Vec<String> {
        self.with_labels(self.with_frames(ids))
    }

    /// A frame just drawn takes in what lies wholly inside it.
    pub(crate) fn adopt_into_new_frame(&mut self, frame: &str) {
        let Some(f) = self.visible(frame).cloned() else { return };
        let fb = bounds(&f);
        let mut checked: std::collections::HashMap<String, bool> = Default::default();
        let candidates: Vec<Element> = self.scene.visible().filter(|e| e.id != frame && !is_frame(e) && e.frame_id.is_none()).cloned().collect();
        let mut taken = Vec::new();
        for e in &candidates {
            if !within(bounds(e), fb) {
                continue;
            }
            // A group comes in whole or not at all.
            if let Some(g) = e.group_ids.last() {
                let whole = *checked.entry(g.clone()).or_insert_with(|| {
                    let members: Vec<&Element> = self.scene.visible().filter(|o| o.group_ids.contains(g)).collect();
                    !members.iter().any(|m| is_frame(m)) && bounds_of(members.iter().copied()).is_some_and(|b| within(b, fb))
                });
                if !whole {
                    continue;
                }
            }
            taken.push(e.id.clone());
        }
        for id in self.with_labels(taken) {
            self.change(&id, vec![update("frameId", frame)]);
        }
    }

    /// The topmost frame `p` is inside, other than those in `except`.
    pub fn frame_at(&self, p: Point, except: &HashSet<String>) -> Option<String> {
        self.scene
            .visible()
            .filter(|e| is_frame(e) && !except.contains(&e.id))
            .filter(|e| {
                let b = bounds(e);
                p[0] >= b[0] && p[0] <= b[2] && p[1] >= b[1] && p[1] <= b[3]
            })
            .last()
            .map(|e| e.id.clone())
    }

    /// After a drag ending at `p`: dragged onto a frame, the moved elements
    /// (whole groups, labels with their shapes) join it; otherwise those
    /// no longer touching their frame leave it. Elements moved together
    /// with their own frame keep it.
    pub(crate) fn update_frames_after_drag(&mut self, moved: &HashSet<String>, p: Point) {
        let target = self.frame_at(p, moved);
        let elements: Vec<Element> = moved.iter().filter_map(|id| self.visible(id).cloned()).filter(|e| !is_frame(e)).collect();
        for e in &elements {
            if e.text.as_ref().is_some_and(|t| t.container_id.is_some()) {
                continue; // follows its shape, below
            }
            if e.frame_id.as_ref().is_some_and(|f| moved.contains(f)) {
                continue;
            }
            let next = match &target {
                Some(f) => {
                    let fb = self.visible(f).map(bounds);
                    let unit: Vec<&Element> = match e.group_ids.last() {
                        Some(g) => self.scene.visible().filter(|o| o.group_ids.contains(g)).collect(),
                        None => vec![e],
                    };
                    match fb {
                        Some(fb) if unit.iter().any(|u| overlaps(bounds(u), fb)) => Some(f.clone()),
                        _ => e.frame_id.clone(),
                    }
                }
                None => e.frame_id.clone().filter(|f| self.visible(f).is_some_and(|fe| overlaps(bounds(e), bounds(fe)))),
            };
            if next != e.frame_id {
                let mut ids = vec![e.id.clone()];
                ids.extend(self.bound_text_of(&e.id));
                for id in ids {
                    self.change(&id, vec![update("frameId", next.clone().map_or(Value::Null, Value::from))]);
                }
            }
        }
    }

    /// The frame whose name is under `p`: the strip above its top edge.
    pub fn frame_name_at(&self, p: Point) -> Option<String> {
        let h = NAME_HEIGHT_PX / self.zoom;
        self.scene
            .visible()
            .filter(|e| is_frame(e) && e.angle == 0.0)
            .filter(|e| p[0] >= e.x && p[0] <= e.x + e.width.max(40.0 / self.zoom) && p[1] >= e.y - h && p[1] < e.y)
            .last()
            .map(|e| e.id.clone())
    }

    /// A frame's name as shown (`getFrameLikeTitle`).
    pub fn frame_title(&self, id: &str) -> String {
        match self.visible(id) {
            Some(e) => e.frame_name.clone().unwrap_or_else(|| if e.kind == Kind::MagicFrame { "AI Frame".into() } else { "Frame".into() }),
            None => String::new(),
        }
    }

    /// Renames a frame; an empty name goes back to the default.
    pub fn rename_frame(&mut self, id: &str, name: &str) {
        let name = name.trim();
        self.begin();
        self.change(id, vec![update("name", if name.is_empty() { Value::Null } else { json!(name) })]);
        self.commit();
    }
}
