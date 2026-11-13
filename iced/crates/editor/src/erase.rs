//! The eraser (E or 0), as Excalidraw's `handleEraser`: what the pointer
//! passes over is marked, drawn faded, and deleted when it is released.
//! Alt un-marks. A group goes whole, a shape with its label.

use crate::geometry;
use crate::{Editor, Gesture, Modifiers, HIT_PX};
use rough::Point;
use serde_json::Value;
use std::collections::HashSet;

impl Editor {
    pub(crate) fn erase_start(&mut self, p: Point, m: Modifiers) {
        self.erasing.clear();
        self.gesture = Some(Gesture::Erase { last: p });
        self.erase_over(p, p, m);
    }

    /// Marks (or with Alt, un-marks) everything along `from` → `to`,
    /// sampled every hit distance as Excalidraw does.
    pub(crate) fn erase_over(&mut self, from: Point, to: Point, m: Modifiers) {
        let threshold = HIT_PX / self.zoom;
        let length = ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt();
        let before = self.erasing.clone();
        let mut t = 0.0;
        loop {
            let r = if length > 0.0 { t / length } else { 0.0 };
            let p = [from[0] + (to[0] - from[0]) * r, from[1] + (to[1] - from[1]) * r];
            // Everything under the point, not only the top one.
            let hits: Vec<(String, Option<String>, bool)> = self
                .scene
                .visible()
                .filter(|e| geometry::hit(e, p, threshold))
                .map(|e| (e.id.clone(), e.group_ids.last().cloned(), e.raw().get("locked") == Some(&Value::Bool(true))))
                .collect();
            for (id, group, locked) in hits {
                if locked {
                    break;
                }
                let mut ids = vec![id];
                if let Some(g) = group {
                    ids.extend(self.scene.visible().filter(|o| o.group_ids.contains(&g)).map(|o| o.id.clone()));
                }
                for id in ids {
                    if m.alt {
                        self.erasing.remove(&id);
                    } else {
                        self.erasing.insert(id);
                    }
                }
            }
            if t >= length {
                break;
            }
            t = (t + threshold).min(length);
        }
        // Labels and their shapes go together.
        let pairs: Vec<(String, String)> = self
            .scene
            .visible()
            .filter_map(|e| Some((e.id.clone(), e.text.as_ref()?.container_id.clone()?)))
            .collect();
        for (label, container) in pairs {
            if self.erasing.contains(&label) || self.erasing.contains(&container) {
                if m.alt {
                    self.erasing.remove(&label);
                    self.erasing.remove(&container);
                } else {
                    self.erasing.insert(label);
                    self.erasing.insert(container);
                }
            }
        }
        // Redraw what changed, faded or not.
        let changed: HashSet<String> = before.symmetric_difference(&self.erasing).cloned().collect();
        self.changes.ids.extend(changed);
    }

    /// Released: everything marked is deleted, as one undo step.
    pub(crate) fn erase_end(&mut self) {
        let ids: Vec<String> = std::mem::take(&mut self.erasing).into_iter().collect();
        self.changes.ids.extend(ids.iter().cloned());
        if ids.is_empty() {
            return;
        }
        // A frame goes with what it holds.
        let ids = self.with_frames(ids);
        self.selection = self.in_order(ids);
        self.delete_selected();
    }
}
