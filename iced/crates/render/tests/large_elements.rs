//! Large but real elements are drawn (review 2026-11-15, IR-9): a long
//! timeline, a map at 1:1. rough.js's work is capped where it grows with
//! size, so they draw in reasonable time rather than being skipped.

use scene::Scene;
use std::time::{Duration, Instant};

fn element(kind: &str, size: f64, fill: &str, stroke: &str, roughness: f64) -> serde_json::Value {
    let mut e = serde_json::json!({
        "id": "e", "type": kind, "x": 0, "y": 0, "width": size, "height": size,
        "strokeColor": "#1e1e1e", "backgroundColor": "#a5d8ff", "fillStyle": fill,
        "strokeWidth": 1, "strokeStyle": stroke, "roughness": roughness, "opacity": 100,
        "seed": 7, "version": 1, "versionNonce": 1, "isDeleted": false,
    });
    if matches!(kind, "line" | "arrow" | "freedraw") {
        e["points"] = serde_json::json!([[0, 0], [size / 2.0, size / 3.0], [size, size]]);
    }
    e
}

#[test]
fn large_elements_are_drawn_in_reasonable_time() {
    let mut slowest = (Duration::ZERO, String::new());
    for size in [1.2e6, 5e7, 9e8] {
        for kind in ["rectangle", "diamond", "ellipse", "line", "arrow", "freedraw"] {
            for fill in ["hachure", "cross-hatch", "zigzag", "solid"] {
                for stroke in ["solid", "dashed"] {
                    for roughness in [0.0, 1.0, 2.0] {
                        let doc = serde_json::json!({ "type": "excalidraw", "version": 2, "elements": [element(kind, size, fill, stroke, roughness)] });
                        let scene = Scene::parse(&doc.to_string()).unwrap();
                        let e = &scene.elements[0];
                        let started = Instant::now();
                        let shapes = render::shape::element_shapes(e, "#ffffff");
                        let took = started.elapsed();
                        let what = format!("{kind} {size:e} {fill} {stroke} roughness {roughness}");
                        if kind != "freedraw" {
                            assert!(!shapes.is_empty(), "{what}: not drawn");
                        }
                        if took > slowest.0 {
                            slowest = (took, what);
                        }
                    }
                }
            }
        }
    }
    eprintln!("slowest: {:?} for {}", slowest.0, slowest.1);
    assert!(slowest.0 < Duration::from_secs(2), "{} took {:?}", slowest.1, slowest.0);
}

#[test]
fn a_long_dashed_line_is_cut_into_a_bounded_number_of_dashes() {
    let dash = |size: f64| {
        let doc = serde_json::json!({ "type": "excalidraw", "version": 2, "elements": [element("line", size, "solid", "dashed", 0.0)] });
        let scene = Scene::parse(&doc.to_string()).unwrap();
        let prepared = render::prepare(&scene.elements[0], &scene, "#ffffff");
        prepared.items.iter().find_map(|i| match i {
            render::Item::Stroke { dash: Some(d), .. } => Some(d.clone()),
            _ => None,
        }).expect("a dashed stroke")
    };
    // An ordinary line keeps Excalidraw's dash exactly.
    assert_eq!(dash(500.0), vec![8.0, 9.0]);
    // A 9e8-unit one: the pattern is stretched to about 20,000 dashes.
    let long = dash(9e8);
    let period: f64 = long.iter().sum();
    let length = 9e8 * std::f64::consts::SQRT_2;
    assert!(length / period <= 20_001.0 && length / period > 5_000.0, "{long:?}");
    assert!((long[0] / long[1] - 8.0 / 9.0).abs() < 1e-9, "same proportions: {long:?}");
}

/// A closed line zigzagging 2,000 times, filled: every hachure row crosses
/// it 2,000 times, so rough.js makes a stroke per pair of crossings (about
/// 28 million operations at a million units), and its outline, flattened,
/// runs to tens of millions of points. Both are bounded, at any size.
#[test]
fn a_filled_outline_crossing_every_row_thousands_of_times_is_bounded() {
    for size in [1e4, 1e6, 9e8] {
        let mut points: Vec<[f64; 2]> = (0..2000).map(|i| [(i % 2) as f64 * size, (i / 2 % 2) as f64 * size / 2.0 + i as f64]).collect();
        points.push([0.0, 0.0]);
        for fill in ["hachure", "cross-hatch", "zigzag", "solid"] {
            let mut e = element("line", size, fill, "solid", 1.0);
            e["points"] = serde_json::json!(points);
            e["roundness"] = serde_json::json!({ "type": 2 });
            let doc = serde_json::json!({ "type": "excalidraw", "version": 2, "elements": [e] });
            let scene = Scene::parse(&doc.to_string()).unwrap();
            let started = Instant::now();
            let shapes = render::shape::element_shapes(&scene.elements[0], "#ffffff");
            let took = started.elapsed();
            let ops: usize = shapes.iter().flat_map(|d| &d.sets).map(|s| s.ops.len()).sum();
            let what = format!("{size:e} {fill}: {ops} operations in {took:?}");
            assert!(ops > 0 && ops < 1_000_000, "{what}");
            assert!(took < Duration::from_secs(2), "{what}");
        }
    }
}
