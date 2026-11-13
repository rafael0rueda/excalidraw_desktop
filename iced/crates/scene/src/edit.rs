//! Changing a drawing the way Excalidraw changes one.
//!
//! - `mutate` is `mutateElement`: an update equal to what is there is no
//!   change; a real change bumps `version`, draws a new random
//!   `versionNonce` and stamps `updated`, which is how Excalidraw (and its
//!   collaboration and library merging) tells the newer copy of an element.
//! - New elements carry every field `_newElementBase` writes, in its order.
//! - Z-order lives in each element's `index` (fractional-indexing keys):
//!   moved and inserted elements get keys between their new neighbours, as
//!   `syncMovedIndices` gives them.
//!
//! Every change goes through the raw JSON and rebuilds the typed view from
//! it, so the two never disagree.

use crate::fractional::{is_valid_key, n_keys_between};
use crate::{Element, Error, Scene};
use serde_json::{json, Map, Value};
use std::cell::Cell;

thread_local! {
    static RNG: Cell<u64> = Cell::new(seed());
}

fn seed() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    (t ^ (u64::from(std::process::id()) << 32)) | 1
}

/// xorshift64*: identifiers and seeds need to differ, not to be secret.
fn next_u64() -> u64 {
    RNG.with(|r| {
        let mut x = r.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        r.set(x);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    })
}

/// `randomInteger`: 0 to 2³¹, what seeds and version nonces are.
pub fn random_integer() -> i64 {
    (next_u64() >> 33) as i64
}

/// `nanoid()`: 21 characters of nanoid's URL-safe alphabet.
pub fn random_id() -> String {
    const ALPHABET: &[u8] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";
    (0..21)
        .map(|_| ALPHABET[(next_u64() >> 58) as usize] as char)
        .collect()
}

/// `Date.now()`.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// What a new element looks like: Excalidraw's `currentItem*` app state.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub stroke_color: String,
    pub background_color: String,
    pub fill_style: String,
    pub stroke_width: f64,
    pub stroke_style: String,
    pub roughness: f64,
    pub opacity: f64,
    /// `currentItemRoundness`: "round" or "sharp".
    pub round: bool,
    /// `currentItemArrowType`: "round" or "sharp" (elbow arrows: phase 10).
    pub arrow_round: bool,
    pub start_arrowhead: Option<String>,
    pub end_arrowhead: Option<String>,
}

impl Default for Style {
    /// Excalidraw's defaults for a fresh canvas.
    fn default() -> Self {
        Self {
            stroke_color: "#1e1e1e".into(),
            background_color: "transparent".into(),
            fill_style: "solid".into(),
            stroke_width: 2.0,
            stroke_style: "solid".into(),
            roughness: 1.0,
            opacity: 100.0,
            round: true,
            arrow_round: true,
            start_arrowhead: None,
            end_arrowhead: Some("arrow".into()),
        }
    }
}

/// A JSON number as `JSON.stringify` would keep it: integers stay integers.
pub fn num(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 {
        json!(n as i64)
    } else {
        json!(n)
    }
}

fn points_value(points: &[[f64; 2]]) -> Value {
    Value::Array(
        points
            .iter()
            .map(|p| json!([num(p[0]), num(p[1])]))
            .collect(),
    )
}

/// A new element of `kind` at `x, y`, every field `_newElementBase` writes
/// and in its order, plus what `newLinearElement` and `newFreeDrawElement`
/// add. Its `index` is set when it is added to a scene.
pub fn new_element(kind: &str, x: f64, y: f64, style: &Style) -> Map<String, Value> {
    // `getDefaultRoundnessTypeForElement`.
    let roundness = match kind {
        // New arrows take theirs from the arrow type.
        "arrow" if style.arrow_round => json!({ "type": 2 }),
        "arrow" => Value::Null,
        _ if !style.round => Value::Null,
        "rectangle" | "embeddable" | "iframe" | "image" => json!({ "type": 3 }),
        "diamond" | "line" => json!({ "type": 2 }),
        _ => Value::Null,
    };
    let mut m = Map::new();
    let mut put = |k: &str, v: Value| {
        m.insert(k.into(), v);
    };
    put("id", json!(random_id()));
    put("type", json!(kind));
    put("x", num(x));
    put("y", num(y));
    put("width", json!(0));
    put("height", json!(0));
    put("angle", json!(0));
    put("strokeColor", json!(style.stroke_color));
    put("backgroundColor", json!(style.background_color));
    put("fillStyle", json!(style.fill_style));
    put("strokeWidth", num(style.stroke_width));
    put("strokeStyle", json!(style.stroke_style));
    put("roughness", num(style.roughness));
    put("opacity", num(style.opacity));
    put("groupIds", json!([]));
    put("frameId", Value::Null);
    put("index", Value::Null);
    put("roundness", roundness);
    put("seed", json!(random_integer()));
    put("version", json!(1));
    put("versionNonce", json!(0));
    put("isDeleted", json!(false));
    put("boundElements", Value::Null);
    put("updated", json!(now_ms()));
    put("link", Value::Null);
    put("locked", json!(false));
    match kind {
        "line" | "arrow" => {
            put("points", json!([[0, 0]]));
            put("lastCommittedPoint", Value::Null);
            put("startBinding", Value::Null);
            put("endBinding", Value::Null);
            let head = |h: &Option<String>| h.as_ref().map_or(Value::Null, |h| json!(h));
            if kind == "arrow" {
                put("startArrowhead", head(&style.start_arrowhead));
                put("endArrowhead", head(&style.end_arrowhead));
                put("elbowed", json!(false));
            } else {
                put("startArrowhead", Value::Null);
                put("endArrowhead", Value::Null);
            }
        }
        "freedraw" => {
            put("points", json!([]));
            put("pressures", json!([]));
            put("simulatePressure", json!(true));
            put("lastCommittedPoint", Value::Null);
        }
        _ => {}
    }
    m
}

/// A new text element, as `newTextElement` makes one: the base fields, then
/// the text's own, in its order. Its size is set once the text is measured.
#[allow(clippy::too_many_arguments)] // newTextElement's own options, one by one
pub fn new_text_element(
    x: f64,
    y: f64,
    style: &Style,
    font_family: f64,
    font_size: f64,
    text_align: &str,
    vertical_align: &str,
    container_id: Option<&str>,
) -> Map<String, Value> {
    let mut m = new_element("text", x, y, style);
    // `getDefaultRoundnessTypeForElement` gives text none.
    m.insert("roundness".into(), Value::Null);
    m.insert("text".into(), json!(""));
    m.insert("fontSize".into(), num(font_size));
    m.insert("fontFamily".into(), num(font_family));
    m.insert("textAlign".into(), json!(text_align));
    m.insert("verticalAlign".into(), json!(vertical_align));
    m.insert(
        "containerId".into(),
        container_id.map_or(Value::Null, |c| json!(c)),
    );
    m.insert("originalText".into(), json!(""));
    m.insert("autoResize".into(), json!(true));
    m.insert(
        "lineHeight".into(),
        num(crate::default_line_height(font_family)),
    );
    m
}

/// `getSizeFromPoints`, for a `points` update.
fn size_of(points: &Value) -> Option<(f64, f64)> {
    let pts = points.as_array()?;
    if pts.is_empty() {
        return None;
    }
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in pts {
        let (x, y) = (p.get(0)?.as_f64()?, p.get(1)?.as_f64()?);
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    Some((b[2] - b[0], b[3] - b[1]))
}

/// Builds an update list: `updates![("x", 5.0), ("strokeColor", "#fff")]`.
pub type Updates = Vec<(String, Value)>;

pub fn update(key: &str, value: impl Into<Value>) -> (String, Value) {
    (key.to_owned(), value.into())
}

pub fn update_num(key: &str, n: f64) -> (String, Value) {
    (key.to_owned(), num(n))
}

pub fn update_points(points: &[[f64; 2]]) -> (String, Value) {
    ("points".to_owned(), points_value(points))
}

impl Scene {
    pub fn position(&self, id: &str) -> Option<usize> {
        self.elements.iter().position(|e| e.id == id)
    }

    /// `mutateElement`. Returns whether anything changed.
    pub fn mutate(&mut self, id: &str, mut updates: Updates) -> bool {
        let Some(i) = self.position(id) else {
            return false;
        };
        if let Some((_, points)) = updates.iter().find(|(k, _)| k == "points") {
            if let Some((w, h)) = size_of(points) {
                // `{ ...getSizeFromPoints(points), ...updates }`: given sizes win.
                if !updates.iter().any(|(k, _)| k == "width") {
                    updates.push(update_num("width", w));
                }
                if !updates.iter().any(|(k, _)| k == "height") {
                    updates.push(update_num("height", h));
                }
            }
        }
        let mut raw = self.elements[i].raw().clone();
        let mut changed = false;
        for (key, value) in updates {
            if raw.get(&key) == Some(&value) {
                continue;
            }
            raw.insert(key, value);
            changed = true;
        }
        if !changed {
            return false;
        }
        bump(&mut raw);
        self.replace(i, raw);
        true
    }

    /// Puts `raw` in place of element `i`, rebuilding its view.
    fn replace(&mut self, i: usize, raw: Map<String, Value>) {
        // Only a points array over the limit fails, and an edit that makes
        // one is refused by leaving the element as it was.
        if let Ok(e) = Element::restore(raw) {
            self.elements[i] = e;
        }
    }

    /// Adds a new element at stacking position `at` (the end is the top),
    /// giving it an index between its neighbours. Returns its id.
    pub fn insert(&mut self, at: usize, raw: Map<String, Value>) -> Result<String, Error> {
        let element = Element::restore(raw)?;
        let id = element.id.clone();
        let at = at.min(self.elements.len());
        self.elements.insert(at, element);
        self.sync_moved(std::slice::from_ref(&id));
        Ok(id)
    }

    pub fn push(&mut self, raw: Map<String, Value>) -> Result<String, Error> {
        self.insert(self.elements.len(), raw)
    }

    /// Deleting is marking `isDeleted`, as Excalidraw does: the element stays
    /// until saving, so undo and collaboration can still see it.
    pub fn delete(&mut self, ids: &[String]) {
        for id in ids {
            self.mutate(id, vec![update("isDeleted", true)]);
        }
    }

    /// The raw JSON of every element, for history snapshots.
    pub fn raw_of(&self, id: &str) -> Option<Map<String, Value>> {
        self.elements
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.raw().clone())
    }

    /// Sets an element's JSON exactly, without bumping anything: for undo
    /// and redo, which put back what was there. `None` removes it. A new
    /// element goes on top; `reorder` puts it where it belongs.
    pub fn set_raw(&mut self, id: &str, raw: Option<Map<String, Value>>) {
        match (self.position(id), raw) {
            (Some(i), Some(raw)) => self.replace(i, raw),
            (Some(i), None) => {
                self.elements.remove(i);
            }
            (None, Some(raw)) => {
                if let Ok(e) = Element::restore(raw) {
                    self.elements.push(e);
                }
            }
            (None, None) => {}
        }
    }

    /// The stacking order, bottom first.
    pub fn order(&self) -> Vec<String> {
        self.elements.iter().map(|e| e.id.clone()).collect()
    }

    /// Restacks to `order` (ids not in it keep their place after those that
    /// are), without touching indices: for undo, whose snapshots carry them.
    pub fn reorder(&mut self, order: &[String]) {
        let rank: std::collections::HashMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        self.elements
            .sort_by_key(|e| rank.get(e.id.as_str()).copied().unwrap_or(usize::MAX));
    }

    /// Moves `ids` (in their current relative order) to sit just above the
    /// element at stacking position `before` would be, re-keying them.
    pub fn move_to(&mut self, ids: &[String], at: usize) {
        let set: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        let below = self.elements[..at.min(self.elements.len())]
            .iter()
            .filter(|e| !set.contains(e.id.as_str()))
            .count();
        let (moved, rest): (Vec<Element>, Vec<Element>) = std::mem::take(&mut self.elements)
            .into_iter()
            .partition(|e| set.contains(e.id.as_str()));
        self.elements = rest;
        let tail = self.elements.split_off(below);
        self.elements.extend(moved);
        self.elements.extend(tail);
        self.sync_moved(ids);
    }

    /// `syncMovedIndices`: each run of `moved` elements gets keys between its
    /// nearest unmoved neighbours. Falls back to `sync_invalid` where those
    /// neighbours are themselves out of order.
    pub fn sync_moved(&mut self, moved: &[String]) {
        let set: std::collections::HashSet<&str> = moved.iter().map(String::as_str).collect();
        let mut i = 0;
        let mut ok = true;
        while i < self.elements.len() {
            if !set.contains(self.elements[i].id.as_str()) {
                i += 1;
                continue;
            }
            let start = i;
            while i < self.elements.len() && set.contains(self.elements[i].id.as_str()) {
                i += 1;
            }
            let lower = start.checked_sub(1).and_then(|j| self.index_key(j));
            let upper = (i < self.elements.len())
                .then(|| self.index_key(i))
                .flatten();
            match n_keys_between(lower.as_deref(), upper.as_deref(), i - start) {
                Some(keys) => {
                    for (j, key) in (start..i).zip(keys) {
                        self.set_index(j, key);
                    }
                }
                None => ok = false,
            }
        }
        if !ok {
            self.sync_invalid();
        }
    }

    /// Repairs indices that are missing or out of order: elements keep their
    /// key while it is valid and above the last one kept, the rest are
    /// re-keyed between the kept keys around them.
    pub fn sync_invalid(&mut self) {
        let n = self.elements.len();
        let mut keep = vec![false; n];
        let mut last: Option<String> = None;
        for (i, k) in keep.iter_mut().enumerate() {
            if let Some(key) = self.index_key(i) {
                if last.as_ref().is_none_or(|l| *l < key) {
                    *k = true;
                    last = Some(key);
                }
            }
        }
        let mut i = 0;
        while i < n {
            if keep[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < n && !keep[i] {
                i += 1;
            }
            let lower = start.checked_sub(1).and_then(|j| self.index_key(j));
            let upper = (i < n).then(|| self.index_key(i)).flatten();
            if let Some(keys) = n_keys_between(lower.as_deref(), upper.as_deref(), i - start) {
                for (j, key) in (start..i).zip(keys) {
                    self.set_index(j, key);
                }
            }
        }
    }

    fn index_key(&self, i: usize) -> Option<String> {
        let key = self.elements.get(i)?.raw().get("index")?.as_str()?;
        is_valid_key(key).then(|| key.to_owned())
    }

    /// `mutateElement(element, { index })`.
    fn set_index(&mut self, i: usize, key: String) {
        let id = self.elements[i].id.clone();
        self.mutate(&id, vec![update("index", key)]);
    }
}

/// `version++`, a new `versionNonce`, `updated` now.
fn bump(raw: &mut Map<String, Value>) {
    let version = raw.get("version").and_then(Value::as_f64).unwrap_or(0.0);
    raw.insert("version".into(), num(version + 1.0));
    raw.insert("versionNonce".into(), json!(random_integer()));
    raw.insert("updated".into(), json!(now_ms()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> Scene {
        Scene::parse(r#"{"type":"excalidraw","elements":[]}"#).unwrap()
    }

    #[test]
    fn new_elements_have_excalidraws_fields_in_its_order() {
        let e = new_element("arrow", 10.0, 20.5, &Style::default());
        let keys: Vec<&str> = e.keys().map(String::as_str).collect();
        assert_eq!(&keys[..6], ["id", "type", "x", "y", "width", "height"]);
        assert_eq!(keys.last(), Some(&"elbowed"));
        assert_eq!(e["roundness"], json!({ "type": 2 }));
        assert_eq!(e["endArrowhead"], "arrow");
        assert_eq!(e["id"].as_str().unwrap().len(), 21);
        assert_eq!(
            e["x"],
            json!(10),
            "whole numbers stay integers, as JSON.stringify writes them"
        );
    }

    #[test]
    fn a_change_bumps_the_version_and_an_unchanged_update_does_not() {
        let mut s = scene();
        let id = s
            .push(new_element("rectangle", 0.0, 0.0, &Style::default()))
            .unwrap();
        let v = s.by_id(&id).unwrap().version;
        assert!(
            !s.mutate(&id, vec![update_num("x", 0.0)]),
            "the same value is no change"
        );
        assert!(s.mutate(&id, vec![update_num("x", 5.0)]));
        let e = s.by_id(&id).unwrap();
        assert_eq!((e.x, e.version), (5.0, v + 1.0));
    }

    #[test]
    fn points_set_the_size() {
        let mut s = scene();
        let id = s
            .push(new_element("line", 0.0, 0.0, &Style::default()))
            .unwrap();
        s.mutate(
            &id,
            vec![update_points(&[[0.0, 0.0], [30.0, -10.0], [10.0, 20.0]])],
        );
        let e = s.by_id(&id).unwrap();
        assert_eq!((e.width, e.height), (30.0, 30.0));
    }

    #[test]
    fn indices_follow_the_stacking_order() {
        let mut s = scene();
        let ids: Vec<String> = (0..5)
            .map(|i| {
                s.push(new_element(
                    "rectangle",
                    f64::from(i),
                    0.0,
                    &Style::default(),
                ))
                .unwrap()
            })
            .collect();
        let keys = |s: &Scene| {
            s.elements
                .iter()
                .map(|e| e.raw()["index"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&s), ["a0", "a1", "a2", "a3", "a4"]);
        // To the back: one new key, below the rest; nothing else changes.
        s.move_to(&[ids[3].clone()], 0);
        assert_eq!(s.order()[0], ids[3]);
        let k = keys(&s);
        assert_eq!(&k[1..], ["a0", "a1", "a2", "a4"]);
        assert!(k.windows(2).all(|w| w[0] < w[1]), "{k:?}");
        // Inserting between two keys.
        let new = s
            .insert(2, new_element("ellipse", 0.0, 0.0, &Style::default()))
            .unwrap();
        let k = keys(&s);
        assert_eq!(s.order()[2], new);
        assert!(k.windows(2).all(|w| w[0] < w[1]), "{k:?}");
    }

    #[test]
    fn invalid_indices_are_repaired_keeping_the_valid_ones() {
        let mut s = Scene::parse(
            r#"{"type":"excalidraw","elements":[
                {"id":"a","type":"rectangle","index":"a0"},
                {"id":"b","type":"rectangle","index":"a90"},
                {"id":"c","type":"rectangle"},
                {"id":"d","type":"rectangle","index":"a5"}]}"#,
        )
        .unwrap();
        s.sync_invalid();
        let k: Vec<_> = s
            .elements
            .iter()
            .map(|e| e.raw()["index"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!((k[0].as_str(), k[3].as_str()), ("a0", "a5"));
        assert!(k.windows(2).all(|w| w[0] < w[1]), "{k:?}");
    }
}
