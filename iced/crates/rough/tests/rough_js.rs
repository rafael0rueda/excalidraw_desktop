//! Holds the port to rough.js's own output.
//!
//! `gen-rough-js.mjs` runs the rough.js in `node_modules` over 343 cases
//! shaped like Excalidraw's calls. Every number must match exactly, not
//! nearly: a port that is merely close drifts apart as soon as one comparison
//! or `Math.round` lands the other way, and the strokes stop being the same
//! strokes.
//!
//! `rough-js.digest.json` (committed) holds a hash of every op set's numbers;
//! `rough-js.json` (written beside it, not committed) holds the numbers, so
//! that when it is present a failure names the one that differs.

use rough::{Drawable, FillStyle, Op, OpSetType, Options};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Fixture<S> {
    version: String,
    cases: Vec<Case<S>>,
}

#[derive(Deserialize)]
struct Case<S> {
    method: String,
    args: Vec<Value>,
    options: serde_json::Map<String, Value>,
    sets: Vec<S>,
}

#[derive(Deserialize)]
struct DigestSet {
    #[serde(rename = "type")]
    kind: String,
    n: usize,
    fnv: String,
}

#[derive(Deserialize)]
struct FullSet {
    #[serde(rename = "type")]
    kind: String,
    ops: Vec<Vec<Value>>,
}

fn options(js: &serde_json::Map<String, Value>) -> Options {
    let mut o = Options::default();
    for (key, value) in js {
        let n = || value.as_f64().unwrap();
        let b = || value.as_bool().unwrap();
        match key.as_str() {
            "seed" => o.seed = n(),
            "roughness" => o.roughness = n(),
            "strokeWidth" => o.stroke_width = n(),
            "fillWeight" => o.fill_weight = n(),
            "hachureGap" => o.hachure_gap = n(),
            "curveFitting" => o.curve_fitting = n(),
            "disableMultiStroke" => o.disable_multi_stroke = b(),
            "preserveVertices" => o.preserve_vertices = b(),
            "stroke" => o.stroke = value.as_str().unwrap().into(),
            "fill" => o.fill = Some(value.as_str().unwrap().into()),
            "fillStyle" => o.fill_style = FillStyle::from_name(value.as_str().unwrap()),
            other => panic!("option {other} not mapped"),
        }
    }
    o
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn points(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| [f(&p[0]), f(&p[1])])
        .collect()
}

fn run<S>(case: &Case<S>) -> Drawable {
    let o = options(&case.options);
    let a = &case.args;
    match case.method.as_str() {
        "rectangle" => Drawable::rectangle(f(&a[0]), f(&a[1]), f(&a[2]), f(&a[3]), &o),
        "ellipse" => Drawable::ellipse(f(&a[0]), f(&a[1]), f(&a[2]), f(&a[3]), &o),
        "circle" => Drawable::circle(f(&a[0]), f(&a[1]), f(&a[2]), &o),
        "line" => Drawable::line(f(&a[0]), f(&a[1]), f(&a[2]), f(&a[3]), &o),
        "polygon" => Drawable::polygon(&points(&a[0]), &o),
        "linearPath" => Drawable::linear_path(&points(&a[0]), &o),
        "curve" => Drawable::curve(&points(&a[0]), &o),
        "path" => Drawable::path(a[0].as_str().unwrap(), &o),
        other => panic!("method {other} not mapped"),
    }
}

fn kind_name(kind: OpSetType) -> &'static str {
    match kind {
        OpSetType::Path => "path",
        OpSetType::FillPath => "fillPath",
        OpSetType::FillSketch => "fillSketch",
    }
}

fn op_values(op: &Op) -> (&'static str, u8, Vec<f64>) {
    match op {
        Op::Move(p) => ("move", 0, p.to_vec()),
        Op::LineTo(p) => ("lineTo", 1, p.to_vec()),
        Op::BCurveTo(d) => ("bcurveTo", 2, d.to_vec()),
    }
}

/// The generator's FNV-1a: each op's kind byte, then its numbers' bits.
fn fnv(ops: &[Op]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut step = |byte: u8| {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for op in ops {
        let (_, kind, values) = op_values(op);
        step(kind);
        for v in values {
            v.to_le_bytes().into_iter().for_each(&mut step);
        }
    }
    format!("{h:016x}")
}

fn label<S>(n: usize, case: &Case<S>) -> String {
    format!(
        "case {n} ({} {:?})",
        case.method,
        case.options.get("fillStyle")
    )
}

fn report(failures: Vec<String>, total: usize) {
    assert!(
        failures.is_empty(),
        "{} of {total} cases differ from rough.js:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_case_matches_rough_js_bit_for_bit() {
    let fixture: Fixture<DigestSet> =
        serde_json::from_str(include_str!("rough-js.digest.json")).unwrap();
    assert_eq!(
        fixture.version, "4.6.4",
        "regenerate against the rough.js Excalidraw 0.18.1 pins"
    );
    let mut failures = Vec::new();
    for (n, case) in fixture.cases.iter().enumerate() {
        let got = run(case);
        let got: Vec<_> = got
            .sets
            .iter()
            .map(|s| (kind_name(s.kind), s.ops.len(), fnv(&s.ops)))
            .collect();
        let want: Vec<_> = case
            .sets
            .iter()
            .map(|s| (s.kind.as_str(), s.n, s.fnv.clone()))
            .collect();
        if got != want {
            failures.push(format!(
                "{}:\n    port:     {got:?}\n    rough.js: {want:?}",
                label(n, case)
            ));
        }
    }
    report(failures, fixture.cases.len());
}

/// The same check with every number in hand, when the generator has been run
/// here: it names the first number of each op set that differs.
#[test]
fn every_number_matches_rough_js_when_the_full_fixture_is_present() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/rough-js.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("{path} not present; run tests/gen-rough-js.mjs for number-by-number diffs");
        return;
    };
    let fixture: Fixture<FullSet> = serde_json::from_str(&text).unwrap();
    let mut failures = Vec::new();
    for (n, case) in fixture.cases.iter().enumerate() {
        let got = run(case);
        let label = label(n, case);
        if got.sets.len() != case.sets.len() {
            failures.push(format!(
                "{label}: {} sets, rough.js has {}",
                got.sets.len(),
                case.sets.len()
            ));
            continue;
        }
        'sets: for (s, (got_set, want_set)) in got.sets.iter().zip(&case.sets).enumerate() {
            let kind = kind_name(got_set.kind);
            if kind != want_set.kind || got_set.ops.len() != want_set.ops.len() {
                failures.push(format!(
                    "{label} set {s}: {kind} × {}, rough.js has {} × {}",
                    got_set.ops.len(),
                    want_set.kind,
                    want_set.ops.len()
                ));
                continue;
            }
            for (i, (got_op, want_op)) in got_set.ops.iter().zip(&want_set.ops).enumerate() {
                let (name, _, values) = op_values(got_op);
                let want_values: Vec<f64> = want_op[1..].iter().map(f).collect();
                if name != want_op[0].as_str().unwrap() || values != want_values {
                    failures.push(format!(
                        "{label} set {s} op {i}: {name} {values:?}\n    rough.js: {} {want_values:?}",
                        want_op[0]
                    ));
                    continue 'sets;
                }
            }
        }
    }
    report(failures, fixture.cases.len());
}
