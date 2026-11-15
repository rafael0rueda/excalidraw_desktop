//! A drawing is untrusted input (plan, "Security"). Damaged and hostile
//! variants of real drawings are pushed through everything that reads one:
//! parsing, drawing, the editor's actions, SVG and PNG export. None may
//! panic; refusing the file is fine.
//!
//! Stable Rust and deterministic, so it runs in CI on every change (300
//! rounds). Longer runs: `FUZZ_ROUNDS=20000 FUZZ_SEED=7 cargo test --release
//! -p export --test fuzz_smoke`; `FUZZ_TRACE=1` keeps each round's drawing
//! in the temp directory, for runs that hang or are killed, and
//! `FUZZ_FILE=drawing … replay` runs one drawing again.

#[path = "common/exercise.rs"]
mod exercise;

use exercise::exercise;
use serde_json::{json, Value};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Values that have broken parsers and geometry code before.
fn nasty(rng: &mut Rng) -> Value {
    match rng.below(18) {
        0 => Value::Null,
        1 => json!(true),
        2 => json!(-1),
        3 => json!(0),
        4 => json!(1e308),
        5 => json!(-1e308),
        6 => json!(1e-308),
        7 => json!(5e15),
        8 => json!(""),
        9 => json!("x".repeat(5000)),
        10 => json!([]),
        11 => json!({}),
        12 => json!([[1e308, -1e308], [0, 0]]),
        13 => json!([[0, 0]]),
        14 => Value::Array((0..2000).map(|i| json!([i, -i])).collect()),
        15 => json!(["a", "b"]),
        16 => json!({ "elementId": "nope", "focus": 1e9, "gap": -5, "fixedPoint": [1e308, "x"] }),
        _ => json!(-0.0),
    }
}

const TYPES: [&str; 14] = ["rectangle", "diamond", "ellipse", "line", "arrow", "freedraw", "text", "image", "frame", "magicframe", "iframe", "embeddable", "selection", "nonsense"];

/// One damage done somewhere in the drawing.
fn mutate(doc: &mut Value, rng: &mut Rng) {
    let Some(elements) = doc.get_mut("elements").and_then(Value::as_array_mut) else { return };
    if elements.is_empty() {
        elements.push(json!({ "id": "x", "type": "rectangle" }));
    }
    let i = rng.below(elements.len());
    match rng.below(6) {
        // A field set to something nasty.
        0 | 1 => {
            if let Some(obj) = elements[i].as_object_mut() {
                let keys: Vec<String> = obj.keys().cloned().collect();
                let key = keys.get(rng.below(keys.len())).cloned().unwrap_or_else(|| "x".into());
                let v = nasty(rng);
                obj.insert(key, v);
            }
        }
        // A field gone.
        2 => {
            if let Some(obj) = elements[i].as_object_mut() {
                let keys: Vec<String> = obj.keys().cloned().collect();
                if let Some(k) = keys.get(rng.below(keys.len())) {
                    obj.remove(k);
                }
            }
        }
        // Another type, with the old one's fields.
        3 => {
            if let Some(obj) = elements[i].as_object_mut() {
                obj.insert("type".into(), json!(TYPES[rng.below(TYPES.len())]));
            }
        }
        // A copy with the same id, or one pointing at itself.
        4 => {
            let mut copy = elements[i].clone();
            if copy.is_object() && rng.below(2) == 0 {
                let id = copy["id"].clone();
                copy["containerId"] = id.clone();
                copy["frameId"] = id.clone();
                copy["groupIds"] = json!([id]);
            }
            elements.push(copy);
        }
        // Not an element at all.
        _ => elements[i] = nasty(rng),
    }
}

/// `FUZZ_FILE=drawing cargo test -p export --test fuzz_smoke replay`:
/// one drawing, PNG included, outside `catch_unwind`, for a debugger.
#[test]
fn replay() {
    if let Some(path) = std::env::var_os("FUZZ_FILE") {
        exercise(&std::fs::read_to_string(path).unwrap(), true);
    }
}

#[test]
fn damaged_drawings_never_panic() {
    let seeds = [
        include_str!("../../scene/tests/fixtures/every-element.excalidraw").to_owned(),
        json!({ "type": "excalidraw", "elements": [] }).to_string(),
    ];
    let var = |name: &str| std::env::var(name).ok().and_then(|s| s.parse().ok());
    let mut rng = Rng(var("FUZZ_SEED").unwrap_or(0x5eed_1234_abcd_ef01u64) | 1);
    let rounds = var("FUZZ_ROUNDS").map_or(300, |n: u64| n as usize);
    let trace = std::env::var_os("FUZZ_TRACE").is_some();
    for round in 0..rounds {
        let mut doc: Value = serde_json::from_str(&seeds[round % seeds.len()]).unwrap();
        for _ in 0..1 + rng.below(4) {
            mutate(&mut doc, &mut rng);
        }
        let text = doc.to_string();
        let png = round % 25 == 0;
        if trace {
            // A run that hangs or is killed leaves its drawing behind.
            std::fs::write(std::env::temp_dir().join("excalidraw-fuzz-current.excalidraw"), &text).unwrap();
            eprintln!("round {round}");
        }
        if std::panic::catch_unwind(|| exercise(&text, png)).is_err() {
            let path = std::env::temp_dir().join("excalidraw-fuzz-failure.excalidraw");
            std::fs::write(&path, &text).unwrap();
            panic!("round {round} panicked; the drawing is in {}", path.display());
        }
    }
}
