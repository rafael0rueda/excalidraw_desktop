//! Holds the scene view and the shape code to Excalidraw 0.18.1 itself:
//! `restoreElements`, `ShapeCache.generateElementShape` and
//! `getFreeDrawSvgPath`, run on the same elements by
//! `gen-excalidraw-shapes.mjs`. Every number in every op set must match.

use rough::{Drawable, Op, OpSetType};
use scene::Scene;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Fixture {
    version: String,
    canvas: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    input: Value,
    view: View,
    drawables: Vec<Want>,
    freedraw_path: Option<String>,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct View {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    roughness: f64,
    stroke_width: f64,
    is_deleted: bool,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Want {
    shape: String,
    stroke: String,
    stroke_width: f64,
    fill: Option<String>,
    fill_style: String,
    stroke_line_dash: Option<Vec<f64>>,
    roughness: f64,
    sets: Vec<Set>,
}

#[derive(Deserialize, Debug, PartialEq)]
struct Set {
    #[serde(rename = "type")]
    kind: String,
    n: usize,
    fnv: String,
}

fn fnv(ops: &[Op]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut step = |byte: u8| {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for op in ops {
        let (kind, values): (u8, Vec<f64>) = match op {
            Op::Move(p) => (0, p.to_vec()),
            Op::LineTo(p) => (1, p.to_vec()),
            Op::BCurveTo(d) => (2, d.to_vec()),
        };
        step(kind);
        for v in values {
            v.to_le_bytes().into_iter().for_each(&mut step);
        }
    }
    format!("{h:016x}")
}

fn describe(d: &Drawable) -> Want {
    let o = &d.options;
    Want {
        shape: format!("{:?}", d.shape)
            .to_lowercase()
            .replace("linearpath", "linearPath"),
        stroke: o.stroke.clone(),
        stroke_width: o.stroke_width,
        fill: o.fill.clone(),
        fill_style: match o.fill_style {
            rough::FillStyle::Hachure => "hachure",
            rough::FillStyle::CrossHatch => "cross-hatch",
            rough::FillStyle::Zigzag => "zigzag",
            rough::FillStyle::Solid => "solid",
        }
        .into(),
        stroke_line_dash: o.stroke_line_dash.clone(),
        roughness: o.roughness,
        sets: d
            .sets
            .iter()
            .map(|s| Set {
                kind: match s.kind {
                    OpSetType::Path => "path",
                    OpSetType::FillPath => "fillPath",
                    OpSetType::FillSketch => "fillSketch",
                }
                .into(),
                n: s.ops.len(),
                fnv: fnv(&s.ops),
            })
            .collect(),
    }
}

#[test]
fn every_element_matches_excalidraw() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("excalidraw-shapes.digest.json")).unwrap();
    assert_eq!(
        fixture.version, "0.18.1",
        "regenerate against the Excalidraw the app pins"
    );
    let elements: Vec<Value> = fixture.cases.iter().map(|c| c.input.clone()).collect();
    let file = serde_json::json!({ "type": "excalidraw", "elements": elements });
    let scene = Scene::parse(&file.to_string()).unwrap();
    let mut failures = Vec::new();
    for (case, e) in fixture.cases.iter().zip(&scene.elements) {
        let label = format!("{} ({})", e.id, case.input["type"]);
        let view = View {
            x: e.x,
            y: e.y,
            width: e.width,
            height: e.height,
            roughness: e.roughness,
            stroke_width: e.stroke_width,
            is_deleted: e.is_deleted,
        };
        if view != case.view {
            failures.push(format!(
                "{label}: view\n    scene:      {view:?}\n    excalidraw: {:?}",
                case.view
            ));
            continue;
        }
        let got: Vec<Want> = render::shape::element_shapes(e, &fixture.canvas)
            .iter()
            .map(describe)
            .collect();
        if got.len() != case.drawables.len() {
            failures.push(format!(
                "{label}: {} drawables, Excalidraw has {}",
                got.len(),
                case.drawables.len()
            ));
            continue;
        }
        for (i, (g, w)) in got.iter().zip(&case.drawables).enumerate() {
            if g != w {
                failures.push(format!(
                    "{label} drawable {i}:\n    port:       {g:?}\n    excalidraw: {w:?}"
                ));
            }
        }
        if let (Some(want), Some(fd)) = (&case.freedraw_path, &e.free_draw) {
            if &render::freehand::svg_path(fd, e.stroke_width) != want {
                failures.push(format!("{label}: freedraw outline differs"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} elements differ:\n{}",
        failures.len(),
        fixture.cases.len(),
        failures.join("\n")
    );
}
