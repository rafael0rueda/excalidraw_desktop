//! A drawing read and written back unchanged is the same file, byte for byte.

use scene::{Kind, Scene};

const EVERY_ELEMENT: &str = include_str!("fixtures/every-element.excalidraw");

#[test]
fn a_drawing_saved_unchanged_is_byte_for_byte_what_was_read() {
    let scene = Scene::parse(EVERY_ELEMENT).unwrap();
    let written = scene.to_json();
    if written != EVERY_ELEMENT {
        let line = written
            .lines()
            .zip(EVERY_ELEMENT.lines())
            .position(|(a, b)| a != b);
        panic!(
            "differs at line {line:?}:\n  wrote: {:?}\n  read:  {:?}",
            line.and_then(|l| written.lines().nth(l)),
            line.and_then(|l| EVERY_ELEMENT.lines().nth(l))
        );
    }
}

#[test]
fn unknown_types_and_fields_survive() {
    let scene = Scene::parse(EVERY_ELEMENT).unwrap();
    let future = scene.by_id("future").unwrap();
    assert_eq!(future.kind, Kind::Unknown);
    assert_eq!(future.raw()["nested"]["keep"][1], 2.5);
    assert!(scene.to_json().contains("\"extraTopLevel\""));
}

#[test]
fn the_view_applies_excalidraws_defaults_and_migrations() {
    let scene = Scene::parse(EVERY_ELEMENT).unwrap();
    let get = |id| scene.by_id(id).unwrap();

    // Negative sizes are flipped, moving the origin.
    let neg = get("neg");
    assert_eq!(
        (neg.x, neg.y, neg.width, neg.height),
        (10.5 - 40.0, -20.25 - 30.0, 40.0, 30.0)
    );

    // `strokeSharpness: "round"` on a rectangle is the legacy radius.
    assert_eq!(get("legacy-round").roundness.unwrap().kind, 1.0);

    // A line's first point is moved to the origin, and its size follows its points.
    let line = get("line");
    assert_eq!((line.x, line.y), (5.0, 5.0));
    assert_eq!(
        line.linear.as_ref().unwrap().points,
        vec![[0.0, 0.0], [100.0, 40.0], [50.0, 90.0]]
    );
    assert_eq!((line.width, line.height), (100.0, 90.0));

    // Arrowheads: absent end is an arrow, explicit null is none.
    let arrow = get("arrow").linear.clone().unwrap();
    assert_eq!(arrow.start_arrowhead.as_deref(), Some("dot"));
    assert_eq!(arrow.end_arrowhead.as_deref(), Some("arrow"));
    assert_eq!(
        get("arrow-null").linear.clone().unwrap().end_arrowhead,
        None
    );
    assert_eq!(get("line").linear.clone().unwrap().end_arrowhead, None);

    // An arrow with fewer than two points is rebuilt from its size.
    assert_eq!(
        get("arrow-old").linear.clone().unwrap().points,
        vec![[0.0, 0.0], [50.0, 25.0]]
    );

    // `font: "28px Virgil"` and no line height: detected from the height.
    let legacy = get("txt-legacy").text.clone().unwrap();
    assert_eq!((legacy.font_size, legacy.font_family), (28.0, 1.0));
    assert_eq!(legacy.line_height, 70.0 / 2.0 / 28.0);

    assert_eq!(get("img").image.clone().unwrap().scale, [-1.0, 1.0]);
    assert_eq!(scene.file("f1").unwrap().0, "image/png");
    assert_eq!(scene.view_background_color(), Some("#ffffff"));

    let visible: Vec<_> = scene.visible().map(|e| e.id.as_str()).collect();
    assert!(!visible.contains(&"gone"));
}

#[test]
fn what_is_not_a_drawing_is_refused() {
    assert!(Scene::parse("[]").is_err());
    assert!(Scene::parse(r#"{"type":"excalidrawlib"}"#).is_err());
    assert!(Scene::parse("{ not json").is_err());
    assert!(
        Scene::parse(r#"{"elements":[]}"#).is_ok(),
        "type may be absent"
    );
}
