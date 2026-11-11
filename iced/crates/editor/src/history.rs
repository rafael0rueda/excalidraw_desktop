//! Undo and redo.
//!
//! One entry per user action: the JSON of every element the action touched,
//! before and after, and the stacking order around it. Undo puts the
//! "before" JSON back but bumps its version past the current one, as
//! Excalidraw's history does (it applies changes with `newElementWith`), so
//! anything comparing versions sees the undo as the newest change.

use scene::edit::{now_ms, num, random_integer};
use scene::Scene;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

type Raw = Option<Map<String, Value>>;

struct Entry {
    before: Vec<(String, Raw)>,
    after: Vec<(String, Raw)>,
    order_before: Vec<String>,
    order_after: Vec<String>,
}

#[derive(Default)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    /// The action in progress: what each touched element was before it.
    pending: Option<(HashMap<String, Raw>, Vec<String>)>,
}

/// More than any session needs; old entries go first.
const LIMIT: usize = 500;

impl History {
    /// Starts recording an action. Calls nest: only the outermost counts.
    pub fn begin(&mut self, scene: &Scene) {
        if self.pending.is_none() {
            self.pending = Some((HashMap::new(), scene.order()));
        }
    }

    /// Records `id` as it is now, before the action changes it. Only the
    /// first call for an id in an action counts.
    pub fn touch(&mut self, scene: &Scene, id: &str) {
        if let Some((before, _)) = &mut self.pending {
            before
                .entry(id.to_owned())
                .or_insert_with(|| scene.raw_of(id));
        }
    }

    /// Ends the action. Nothing is recorded if nothing changed.
    pub fn commit(&mut self, scene: &Scene) {
        let Some((before, order_before)) = self.pending.take() else {
            return;
        };
        let mut entry = Entry {
            before: Vec::new(),
            after: Vec::new(),
            order_before,
            order_after: scene.order(),
        };
        for (id, was) in before {
            let now = scene.raw_of(&id);
            if now != was {
                entry.before.push((id.clone(), was));
                entry.after.push((id, now));
            }
        }
        if entry.before.is_empty() && entry.order_before == entry.order_after {
            return;
        }
        self.undo.push(entry);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Abandons the action in progress, putting everything back.
    pub fn cancel(&mut self, scene: &mut Scene) {
        let Some((before, order)) = self.pending.take() else {
            return;
        };
        for (id, raw) in before {
            scene.set_raw(&id, raw);
        }
        scene.reorder(&order);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Returns the ids it changed.
    pub fn undo(&mut self, scene: &mut Scene) -> Vec<String> {
        let Some(entry) = self.undo.pop() else {
            return Vec::new();
        };
        let ids = apply(scene, &entry.before, &entry.order_before);
        self.redo.push(entry);
        ids
    }

    pub fn redo(&mut self, scene: &mut Scene) -> Vec<String> {
        let Some(entry) = self.redo.pop() else {
            return Vec::new();
        };
        let ids = apply(scene, &entry.after, &entry.order_after);
        self.undo.push(entry);
        ids
    }
}

fn apply(scene: &mut Scene, state: &[(String, Raw)], order: &[String]) -> Vec<String> {
    for (id, raw) in state {
        let current = scene.by_id(id).map(|e| e.version);
        let raw = raw.clone().map(|mut raw| {
            if let Some(v) = current {
                let was = raw.get("version").and_then(Value::as_f64).unwrap_or(0.0);
                raw.insert("version".into(), num(v.max(was) + 1.0));
                raw.insert("versionNonce".into(), json!(random_integer()));
                raw.insert("updated".into(), json!(now_ms()));
            }
            raw
        });
        scene.set_raw(id, raw);
    }
    scene.reorder(order);
    state.iter().map(|(id, _)| id.clone()).collect()
}
