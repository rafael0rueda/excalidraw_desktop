//! Arranging the selection, as Excalidraw 0.18.1's actions do: align
//! (`alignElements`), distribute (`distributeElements`), flip
//! (`flipElements`), and locking (`actionToggleElementLock`,
//! `actionUnlockAllElements`).
//!
//! Groups move as one, labels with their shapes (`getMaximumGroups`).

use crate::geometry::{bounds_of, Bounds};
use crate::Editor;
use scene::edit::{update, update_num, update_points};
use scene::{Element, Kind};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

fn locked(e: &Element) -> bool {
    e.raw().get("locked") == Some(&Value::Bool(true))
}

impl Editor {
    /// Align and distribute need two things, and no frames.
    pub fn can_align(&self) -> bool {
        let s = self.selected();
        s.len() > 1 && !s.iter().any(|e| matches!(e.kind, Kind::Frame | Kind::MagicFrame))
    }

    /// `getMaximumGroups`: the selection as units that move together, each
    /// with its labels.
    fn units(&self) -> Vec<Vec<String>> {
        let mut keys: Vec<String> = Vec::new();
        let mut units: std::collections::HashMap<String, Vec<String>> = Default::default();
        for e in self.selected() {
            let key = e.group_ids.last().cloned().unwrap_or_else(|| e.id.clone());
            let unit = units.entry(key.clone()).or_insert_with(|| {
                keys.push(key);
                Vec::new()
            });
            if let Some(t) = self.bound_text_of(&e.id) {
                unit.push(t);
            }
            unit.push(e.id.clone());
        }
        keys.into_iter().filter_map(|k| units.remove(&k)).collect()
    }

    fn bounds_of_ids(&self, ids: &[String]) -> Option<Bounds> {
        bounds_of(ids.iter().filter_map(|id| self.visible(id)))
    }

    /// Moves each unit by its offset, then the arrows bound to what moved.
    fn shift_units(&mut self, moves: Vec<(Vec<String>, f64, f64)>) {
        let moving: HashSet<String> = moves.iter().flat_map(|(ids, _, _)| ids.iter().cloned()).collect();
        self.begin();
        for (ids, dx, dy) in &moves {
            if *dx == 0.0 && *dy == 0.0 {
                continue;
            }
            for id in ids {
                let Some(e) = self.scene.by_id(id).cloned() else { continue };
                self.change(id, vec![update_num("x", e.x + dx), update_num("y", e.y + dy)]);
            }
        }
        for id in &moving {
            self.update_bound_arrows(id, &moving);
        }
        self.commit();
    }

    /// Ctrl+Shift+arrows, and the panel's align buttons.
    pub fn align(&mut self, to: Align) {
        if !self.can_align() {
            return;
        }
        let Some(all) = bounds_of(self.selected()) else { return };
        let moves = self
            .units()
            .into_iter()
            .filter_map(|unit| {
                let b = self.bounds_of_ids(&unit)?;
                let (dx, dy) = match to {
                    Align::Left => (all[0] - b[0], 0.0),
                    Align::Right => (all[2] - b[2], 0.0),
                    Align::CenterX => ((all[0] + all[2]) / 2.0 - (b[0] + b[2]) / 2.0, 0.0),
                    Align::Top => (0.0, all[1] - b[1]),
                    Align::Bottom => (0.0, all[3] - b[3]),
                    Align::CenterY => (0.0, (all[1] + all[3]) / 2.0 - (b[1] + b[3]) / 2.0),
                };
                Some((unit, dx, dy))
            })
            .collect();
        self.shift_units(moves);
    }

    /// Alt+H and Alt+V: equal gaps between the units, the outer ones kept.
    pub fn distribute(&mut self, axis: Axis) {
        if !self.can_align() {
            return;
        }
        let Some(all) = bounds_of(self.selected()) else { return };
        let (lo, hi) = match axis {
            Axis::X => (0, 2),
            Axis::Y => (1, 3),
        };
        let mut units: Vec<(Vec<String>, Bounds)> = self.units().into_iter().filter_map(|u| Some((u.clone(), self.bounds_of_ids(&u)?))).collect();
        if units.len() < 2 {
            return;
        }
        let mid = |b: &Bounds| (b[lo] + b[hi]) / 2.0;
        units.sort_by(|a, b| mid(&a.1).total_cmp(&mid(&b.1)));
        let span: f64 = units.iter().map(|(_, b)| b[hi] - b[lo]).sum();
        let step = ((all[hi] - all[lo]) - span) / (units.len() - 1) as f64;
        let offset = |d: f64| if axis == Axis::X { (d, 0.0) } else { (0.0, d) };
        let mut moves = Vec::new();
        if step < 0.0 {
            // Overlapping: the middles evenly between the outer two.
            let first = units.iter().position(|(_, b)| b[lo] == all[lo]).unwrap_or(0);
            let last = units.iter().position(|(_, b)| b[hi] == all[hi]).unwrap_or(units.len() - 1);
            let step = (mid(&units[last].1) - mid(&units[first].1)) / (units.len() - 1) as f64;
            let mut pos = mid(&units[first].1);
            for (i, (unit, b)) in units.iter().enumerate() {
                if i != first && i != last {
                    pos += step;
                    let (dx, dy) = offset(pos - mid(b));
                    moves.push((unit.clone(), dx, dy));
                }
            }
        } else {
            let mut pos = all[lo];
            for (unit, b) in &units {
                let (dx, dy) = offset(pos - b[lo]);
                moves.push((unit.clone(), dx, dy));
                pos += step + (b[hi] - b[lo]);
            }
        }
        self.shift_units(moves);
    }

    /// Shift+H and Shift+V: the selection mirrored about its middle. Arrows
    /// bound at either end, flipped alone, swap their arrowheads instead.
    pub fn flip(&mut self, axis: Axis) {
        let ids = self.with_labels(self.selected_in_order());
        let elements: Vec<Element> = ids.iter().filter_map(|id| self.visible(id).cloned()).collect();
        if elements.is_empty() {
            return;
        }
        self.begin();
        let bound_arrows_only = elements.iter().all(|e| {
            e.kind == Kind::Arrow && (e.raw().get("startBinding").is_some_and(|b| !b.is_null()) || e.raw().get("endBinding").is_some_and(|b| !b.is_null()))
        });
        if bound_arrows_only {
            for e in &elements {
                let raw = e.raw();
                let (s, end) = (raw.get("startArrowhead").cloned().unwrap_or(Value::Null), raw.get("endArrowhead").cloned().unwrap_or(Value::Null));
                self.change(&e.id, vec![update("startArrowhead", end), update("endArrowhead", s)]);
            }
            self.commit();
            return;
        }
        let Some(b) = bounds_of(elements.iter()) else { return };
        let mid = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        let x = axis == Axis::X;
        let tau = std::f64::consts::TAU;
        let mut containers = Vec::new();
        for e in &elements {
            if e.linear.as_ref().is_some_and(|l| l.elbowed) {
                continue;
            }
            if let Some(container) = e.text.as_ref().and_then(|t| t.container_id.clone()) {
                // Labels are placed by their shapes, after.
                containers.push(container);
                continue;
            }
            // A mirror reverses the turn.
            let angle = (tau - e.angle).rem_euclid(tau);
            let mut updates = vec![update_num("angle", angle)];
            let points = e.linear.as_ref().map(|l| l.points.clone()).or_else(|| e.free_draw.as_ref().map(|f| f.points.clone()));
            match points {
                // Points mirror about the origin; the origin about the middle.
                Some(points) => {
                    let mirrored: Vec<[f64; 2]> = points.iter().map(|p| if x { [-p[0], p[1]] } else { [p[0], -p[1]] }).collect();
                    updates.push(update_points(&mirrored));
                    updates.push(if x { update_num("x", 2.0 * mid[0] - e.x) } else { update_num("y", 2.0 * mid[1] - e.y) });
                }
                None => {
                    updates.push(if x { update_num("x", 2.0 * mid[0] - e.x - e.width) } else { update_num("y", 2.0 * mid[1] - e.y - e.height) });
                    if let Some(img) = &e.image {
                        let s = if x { [-img.scale[0], img.scale[1]] } else { [img.scale[0], -img.scale[1]] };
                        updates.push(update("scale", json!([scene::edit::num(s[0]), scene::edit::num(s[1])])));
                    }
                }
            }
            self.change(&e.id, updates);
        }
        for c in containers {
            self.sync_label(&c, false);
        }
        let moving: HashSet<String> = ids.iter().cloned().collect();
        for e in &elements {
            if e.kind == Kind::Arrow {
                self.rebind(&e.id);
            } else {
                self.update_bound_arrows(&e.id, &moving);
            }
        }
        self.commit();
    }

    /// Ctrl+Shift+L: locks the selection, or unlocks it if all of it is
    /// locked. Locked elements cannot be picked, moved or erased.
    pub fn toggle_lock(&mut self) {
        let ids = self.with_labels(self.selected_in_order());
        if ids.is_empty() {
            return;
        }
        let lock = ids.iter().filter_map(|id| self.visible(id)).any(|e| !locked(e));
        self.begin();
        for id in &ids {
            self.change(id, vec![update("locked", lock)]);
        }
        self.commit();
    }

    /// Unlocks everything, and selects what was locked.
    pub fn unlock_all(&mut self) {
        let ids: Vec<String> = self.scene.visible().filter(|e| locked(e)).map(|e| e.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        self.begin();
        for id in &ids {
            self.change(id, vec![update("locked", false)]);
        }
        self.commit();
        self.selection = ids.into_iter().filter(|id| self.visible(id).is_some_and(|e| e.text.as_ref().is_none_or(|t| t.container_id.is_none()))).collect();
    }

    pub fn any_locked(&self) -> bool {
        self.scene.visible().any(locked)
    }

    /// Whether everything selected is locked.
    pub fn selection_locked(&self) -> bool {
        let s = self.selected();
        !s.is_empty() && s.iter().all(|e| locked(e))
    }
}

pub(crate) fn is_locked(e: &Element) -> bool {
    locked(e)
}
