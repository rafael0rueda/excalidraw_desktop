//! Holds the freedraw outline to perfect-freehand and Excalidraw's own path
//! code, string for string (`gen-freehand.mjs`).

use scene::FreeDraw;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    version: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    points: Vec<[f64; 2]>,
    pressures: Vec<f64>,
    simulate_pressure: bool,
    stroke_width: f64,
    path: String,
}

#[test]
fn every_outline_matches_excalidraw_exactly() {
    let fixture: Fixture = serde_json::from_str(include_str!("freehand.json")).unwrap();
    assert_eq!(fixture.version, "1.2.0");
    let mut failures = Vec::new();
    for (n, case) in fixture.cases.iter().enumerate() {
        let fd = FreeDraw {
            points: case.points.clone(),
            pressures: case.pressures.clone(),
            simulate_pressure: case.simulate_pressure,
        };
        let got = render::freehand::svg_path(&fd, case.stroke_width);
        if got != case.path {
            let at = got
                .chars()
                .zip(case.path.chars())
                .position(|(a, b)| a != b)
                .unwrap_or(got.len().min(case.path.len()));
            let lo = at.saturating_sub(40);
            failures.push(format!(
                "case {n} ({} points, simulate {}): differs at {at}\n    port:      …{}\n    excalidraw: …{}",
                case.points.len(),
                case.simulate_pressure,
                got.get(lo..(at + 40).min(got.len())).unwrap_or(""),
                case.path.get(lo..(at + 40).min(case.path.len())).unwrap_or("")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        fixture.cases.len(),
        failures.join("\n")
    );
}
