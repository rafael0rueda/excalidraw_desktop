//! The elbow arrow router against Excalidraw 0.18.1's own routes
//! (`elbow.json`, from `gen-elbow.mjs`).
//!
//! Unbound arrows and arrows bound to rectangles at the binding distance
//! must match exactly: they exercise the grid, the A* search and its
//! tie-breaking, the boxes and the dongles. Ellipses, diamonds, rotated
//! shapes and arrows being drawn go through outline geometry this crate
//! approximates (square corners, 64-sided ellipses), so they must take the
//! same path, within a unit.

use editor::elbow::{route, End};
use scene::{Element, Scene};
use serde_json::{json, Value};

fn elements(v: &Value) -> Vec<Element> {
    let scene = Scene::parse(&json!({ "type": "excalidraw", "elements": v }).to_string()).unwrap();
    scene.elements
}

fn fixed(b: &Value) -> [f64; 2] {
    [b["fixedPoint"][0].as_f64().unwrap(), b["fixedPoint"][1].as_f64().unwrap()]
}

#[test]
fn routes_match_excalidraw() {
    let data: Value = serde_json::from_str(include_str!("elbow.json")).unwrap();
    let mut worst: Vec<(String, usize, f64)> = Vec::new();
    for (i, c) in data["cases"].as_array().unwrap().iter().enumerate() {
        let kind = c["kind"].as_str().unwrap();
        let els = elements(&c["elements"]);
        let arrow = &elements(&json!([c["arrow"].clone()]))[0];
        let pts = &arrow.linear.as_ref().unwrap().points;
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        let by_id = |id: &Value| id.as_str().and_then(|id| els.iter().find(|e| e.id == id));
        let start = End {
            point: [arrow.x + first[0], arrow.y + first[1]],
            bound: by_id(&c["start"]["elementId"]).map(|e| (e, fixed(&c["start"]))),
            hovered: by_id(&c["hoveredStart"]),
            arrowhead: !c["arrow"]["startArrowhead"].is_null(),
        };
        let end = End {
            point: [arrow.x + last[0], arrow.y + last[1]],
            bound: by_id(&c["end"]["elementId"]).map(|e| (e, fixed(&c["end"]))),
            hovered: by_id(&c["hoveredEnd"]),
            arrowhead: !c["arrow"]["endArrowhead"].is_null(),
        };
        let (x, y, got) = route(&start, &end, c["dragging"].as_bool().unwrap()).expect("a route");
        let want: Vec<[f64; 2]> = c["result"]["points"].as_array().unwrap().iter().map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]).collect();
        let (wx, wy) = (c["result"]["x"].as_f64().unwrap(), c["result"]["y"].as_f64().unwrap());
        assert_eq!(got.len(), want.len(), "case {i} ({kind}): {} points, Excalidraw {}: {got:?} vs {want:?}", got.len(), want.len());
        let err = got.iter().zip(&want).map(|(g, w)| (g[0] + x - w[0] - wx).abs().max((g[1] + y - w[1] - wy).abs())).fold(0.0, f64::max);
        match kind {
            "free" | "bound" => assert!(err < 1e-9, "case {i} ({kind}): off by {err}: {:?} vs {:?}", (x, y, &got), (wx, wy, &want)),
            _ => assert!(err < 1.0, "case {i} ({kind}): off by {err}: {:?} vs {:?}", (x, y, &got), (wx, wy, &want)),
        }
        worst.push((kind.to_owned(), i, err));
    }
    worst.sort_by(|a, b| b.2.total_cmp(&a.2));
    eprintln!("largest differences: {:?}", &worst[..5]);
}
