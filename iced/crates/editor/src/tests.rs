//! The editor driven as a user would drive it.

use super::*;

const NONE: Modifiers = Modifiers {
    shift: false,
    alt: false,
    ctrl: false,
};
const SHIFT: Modifiers = Modifiers {
    shift: true,
    alt: false,
    ctrl: false,
};
const CTRL: Modifiers = Modifiers {
    shift: false,
    alt: false,
    ctrl: true,
};
const CTRL_SHIFT: Modifiers = Modifiers {
    shift: true,
    alt: false,
    ctrl: true,
};

fn empty() -> Editor {
    Editor::new(Scene::parse(r#"{"type":"excalidraw","elements":[]}"#).unwrap())
}

fn drag(ed: &mut Editor, from: Point, to: Point, m: Modifiers) {
    ed.pointer_down(from, m);
    ed.pointer_move([(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0], m);
    ed.pointer_up(to, m);
}

fn rect(ed: &mut Editor, x: f64, y: f64, w: f64, h: f64) -> String {
    ed.tool = Tool::Rectangle;
    drag(ed, [x, y], [x + w, y + h], NONE);
    ed.selection[0].clone()
}

fn get<'a>(ed: &'a Editor, id: &str) -> &'a Element {
    ed.scene.by_id(id).unwrap()
}

#[test]
fn drawing_a_rectangle_then_undo_and_redo() {
    let mut ed = empty();
    let id = rect(&mut ed, 10.0, 20.0, 100.0, 50.0);
    let e = get(&ed, &id);
    assert_eq!((e.x, e.y, e.width, e.height), (10.0, 20.0, 100.0, 50.0));
    assert_eq!(
        e.roundness.map(|r| r.kind),
        Some(3.0),
        "rectangles are drawn round by default"
    );
    assert_eq!(ed.tool, Tool::Selection, "the tool goes back to selection");
    assert_eq!(ed.selection, vec![id.clone()]);
    assert!(ed.is_dirty());

    ed.key(Key::Char('z'), CTRL);
    assert!(ed.scene.by_id(&id).is_none(), "undo removes what was drawn");
    ed.key(Key::Char('z'), CTRL_SHIFT);
    assert_eq!(get(&ed, &id).width, 100.0, "redo brings it back");
}

#[test]
fn drawing_backwards_and_with_shift() {
    let mut ed = empty();
    ed.tool = Tool::Ellipse;
    drag(&mut ed, [100.0, 100.0], [40.0, 70.0], SHIFT);
    let e = get(&ed, &ed.selection[0].clone()).clone();
    assert_eq!(
        (e.x, e.y, e.width, e.height),
        (40.0, 40.0, 60.0, 60.0),
        "shift draws a circle, toward the pointer"
    );
}

#[test]
fn a_click_with_a_shape_tool_draws_nothing_and_records_nothing() {
    let mut ed = empty();
    ed.tool = Tool::Rectangle;
    ed.pointer_down([5.0, 5.0], NONE);
    ed.pointer_up([5.0, 5.0], NONE);
    assert!(ed.ids().is_empty());
    assert!(!ed.can_undo());
}

#[test]
fn lines_arrows_and_freehand() {
    let mut ed = empty();
    ed.tool = Tool::Arrow;
    drag(&mut ed, [0.0, 0.0], [100.0, 8.0], SHIFT);
    let a = get(&ed, &ed.selection[0].clone()).clone();
    let l = a.linear.unwrap();
    assert_eq!(l.end_arrowhead.as_deref(), Some("arrow"));
    assert!(
        l.points[1][1].abs() < 1e-9,
        "shift snaps to 15° steps: flat"
    );

    ed.tool = Tool::FreeDraw;
    ed.pointer_down([0.0, 0.0], NONE);
    for i in 1..10 {
        ed.pointer_move([f64::from(i), f64::from(i * i)], NONE);
    }
    ed.pointer_up([10.0, 100.0], NONE);
    let f = get(&ed, &ed.selection[0].clone()).clone();
    assert_eq!(f.free_draw.unwrap().points.len(), 11);
    assert_eq!(ed.tool, Tool::FreeDraw, "the pen stays the pen");
}

#[test]
fn selecting_moving_and_undoing_a_move() {
    let mut ed = empty();
    let id = rect(&mut ed, 0.0, 0.0, 100.0, 50.0);
    ed.selection.clear();
    // On the outline: picked, then dragged.
    drag(&mut ed, [0.0, 25.0], [30.0, 45.0], NONE);
    assert_eq!(ed.selection, vec![id.clone()]);
    let e = get(&ed, &id);
    assert_eq!((e.x, e.y), (30.0, 20.0));
    ed.undo();
    let e = get(&ed, &id);
    assert_eq!((e.x, e.y), (0.0, 0.0));
    // A click in the empty middle of a transparent box picks nothing.
    ed.selection.clear();
    ed.pointer_down([50.0, 25.0], NONE);
    ed.pointer_up([50.0, 25.0], NONE);
    assert!(ed.selection.is_empty());
}

#[test]
fn the_rubber_band_selects_what_it_encloses() {
    let mut ed = empty();
    let a = rect(&mut ed, 0.0, 0.0, 50.0, 50.0);
    let b = rect(&mut ed, 100.0, 0.0, 50.0, 50.0);
    let _c = rect(&mut ed, 300.0, 0.0, 50.0, 50.0);
    ed.selection.clear();
    drag(&mut ed, [-20.0, -20.0], [200.0, 80.0], NONE);
    let mut sel = ed.selection.clone();
    sel.sort();
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(sel, want);
}

#[test]
fn resizing_from_a_corner_keeps_the_opposite_one() {
    let mut ed = empty();
    let id = rect(&mut ed, 0.0, 0.0, 100.0, 50.0);
    let handles = ed.handles().unwrap();
    let se = handles
        .handles
        .iter()
        .find(|(h, _)| *h == Handle::SE)
        .unwrap()
        .1;
    drag(&mut ed, se, [se[0] + 100.0, se[1] + 50.0], NONE);
    let e = get(&ed, &id);
    assert!(
        (e.width - 200.0).abs() < 1e-9 && (e.height - 100.0).abs() < 1e-9,
        "{} × {}",
        e.width,
        e.height
    );
    assert_eq!((e.x, e.y), (0.0, 0.0));
    // Shift keeps the ratio.
    let se = ed
        .handles()
        .unwrap()
        .handles
        .iter()
        .find(|(h, _)| *h == Handle::SE)
        .unwrap()
        .1;
    drag(&mut ed, se, [se[0] + 200.0, se[1]], SHIFT);
    let e = get(&ed, &id);
    assert!((e.width / e.height - 2.0).abs() < 1e-9);
}

#[test]
fn resizing_a_rotated_box_keeps_its_fixed_corner_in_place() {
    let mut ed = empty();
    let id = rect(&mut ed, 0.0, 0.0, 100.0, 50.0);
    ed.begin();
    ed.change(&id, vec![update_num("angle", 0.5)]);
    ed.commit();
    let nw_before = geometry::corners(get(&ed, &id))[0];
    let se = ed
        .handles()
        .unwrap()
        .handles
        .iter()
        .find(|(h, _)| *h == Handle::SE)
        .unwrap()
        .1;
    drag(&mut ed, se, [se[0] + 40.0, se[1] + 70.0], NONE);
    let nw_after = geometry::corners(get(&ed, &id))[0];
    assert!(
        (nw_after[0] - nw_before[0]).abs() < 1e-6 && (nw_after[1] - nw_before[1]).abs() < 1e-6,
        "{nw_before:?} → {nw_after:?}"
    );
    assert!(get(&ed, &id).width > 100.0);
}

#[test]
fn rotating_with_the_handle() {
    let mut ed = empty();
    let id = rect(&mut ed, 0.0, 0.0, 100.0, 100.0);
    let r = ed
        .handles()
        .unwrap()
        .handles
        .iter()
        .find(|(h, _)| *h == Handle::Rotation)
        .unwrap()
        .1;
    // From above the centre (50, 50) to its right: a quarter turn.
    drag(&mut ed, r, [200.0, 50.0], SHIFT);
    let e = get(&ed, &id);
    assert!(
        (e.angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
        "{}",
        e.angle
    );
    assert_eq!(
        (e.x, e.y),
        (0.0, 0.0),
        "a square turned about its centre stays where it was"
    );
}

#[test]
fn groups_are_selected_whole_and_can_be_undone() {
    let mut ed = empty();
    let a = rect(&mut ed, 0.0, 0.0, 50.0, 50.0);
    let b = rect(&mut ed, 100.0, 0.0, 50.0, 50.0);
    ed.selection = vec![a.clone(), b.clone()];
    ed.key(Key::Char('g'), CTRL);
    ed.selection.clear();
    ed.pointer_down([0.0, 25.0], NONE);
    ed.pointer_up([0.0, 25.0], NONE);
    assert_eq!(ed.selection.len(), 2, "clicking one selects the group");
    ed.key(Key::Char('g'), CTRL_SHIFT);
    assert!(get(&ed, &a).group_ids.is_empty());
}

#[test]
fn stacking_order() {
    let mut ed = empty();
    let a = rect(&mut ed, 0.0, 0.0, 50.0, 50.0);
    let b = rect(&mut ed, 10.0, 0.0, 50.0, 50.0);
    let c = rect(&mut ed, 20.0, 0.0, 50.0, 50.0);
    ed.selection = vec![a.clone()];
    ed.key(Key::Char(']'), CTRL);
    assert_eq!(ed.ids(), [b.clone(), a.clone(), c.clone()]);
    ed.key(Key::Char(']'), CTRL_SHIFT);
    assert_eq!(ed.ids(), [b.clone(), c.clone(), a.clone()]);
    ed.key(Key::Char('['), CTRL_SHIFT);
    assert_eq!(ed.ids(), [a.clone(), b.clone(), c.clone()]);
    let keys: Vec<String> = ed
        .scene
        .elements
        .iter()
        .map(|e| e.raw()["index"].as_str().unwrap().to_owned())
        .collect();
    assert!(keys.windows(2).all(|w| w[0] < w[1]), "{keys:?}");
    ed.undo();
    assert_eq!(ed.ids(), [b, c, a], "undo restacks");
}

#[test]
fn delete_duplicate_copy_and_paste() {
    let mut ed = empty();
    let a = rect(&mut ed, 0.0, 0.0, 50.0, 50.0);
    ed.key(Key::Char('d'), CTRL);
    assert_eq!(ed.ids().len(), 2);
    let copy = get(&ed, &ed.selection[0].clone()).clone();
    assert_ne!(copy.id, a);
    assert_eq!((copy.x, copy.y), (10.0, 10.0));

    ed.selection = vec![a.clone()];
    let text = ed.copy().unwrap();
    assert!(text.contains("\"excalidraw/clipboard\""));
    assert!(ed.paste(&text, [500.0, 500.0]));
    let pasted = get(&ed, &ed.selection[0].clone()).clone();
    assert_eq!(
        (pasted.x, pasted.y),
        (475.0, 475.0),
        "centred on the pointer"
    );
    assert!(!ed.paste("just words", [0.0, 0.0]));

    ed.selection = vec![a.clone()];
    ed.key(Key::Delete, NONE);
    assert!(
        get(&ed, &a).is_deleted,
        "deleted is marked, as Excalidraw does"
    );
    ed.undo();
    assert!(!get(&ed, &a).is_deleted);
}

#[test]
fn nudging_and_escape() {
    let mut ed = empty();
    let a = rect(&mut ed, 0.0, 0.0, 50.0, 50.0);
    ed.key(Key::Right, NONE);
    ed.key(Key::Down, SHIFT);
    assert_eq!((get(&ed, &a).x, get(&ed, &a).y), (1.0, 5.0));
    // Escape in the middle of a drag puts it back.
    ed.pointer_down([1.0, 30.0], NONE);
    ed.pointer_move([100.0, 100.0], NONE);
    ed.key(Key::Escape, NONE);
    assert_eq!((get(&ed, &a).x, get(&ed, &a).y), (1.0, 5.0));
}

#[test]
fn edits_survive_a_save_and_reload_as_excalidraw_would_read_them() {
    let mut ed = empty();
    let a = rect(&mut ed, 0.0, 0.0, 50.0, 50.0);
    let b = rect(&mut ed, 80.0, 0.0, 50.0, 50.0);
    ed.selection = vec![b.clone()];
    ed.key(Key::Delete, NONE);
    let saved = ed.scene.to_file_json();
    let back = Scene::parse(&saved).unwrap();
    assert_eq!(back.elements.len(), 1, "deleted elements are not saved");
    assert_eq!(back.elements[0].id, a);
    assert_eq!(back.elements[0].raw()["index"], "a0");
}

// --- text (the default measure: every character half the font size wide)

fn label_of(ed: &Editor, container: &str) -> Element {
    get(ed, &ed.bound_text_of(container).expect("a label")).clone()
}

#[test]
fn typing_new_text_and_an_empty_one_leaves_nothing() {
    let mut ed = empty();
    ed.key(Key::Char('t'), NONE);
    ed.pointer_down([10.0, 20.0], NONE);
    let id = ed.editing.clone().unwrap().id;
    ed.set_text("Hello\nworld!");
    let e = get(&ed, &id).clone();
    let t = e.text.clone().unwrap();
    assert_eq!(t.text, "Hello\nworld!");
    assert_eq!(e.raw()["originalText"], "Hello\nworld!");
    // 6 characters × 10 wide; two lines of 20 × 1.25.
    assert_eq!((e.x, e.y, e.width, e.height), (10.0, 20.0, 60.0, 50.0));
    assert_eq!(t.font_family, 5.0, "Excalifont by default");
    ed.key(Key::Escape, NONE);
    assert!(ed.editing.is_none());
    assert_eq!(ed.selection, vec![id.clone()]);
    ed.undo();
    assert!(
        ed.scene.by_id(&id).is_none(),
        "one undo takes the whole text away"
    );

    // Started and left empty: no element, nothing to undo.
    let mut ed = empty();
    ed.double_click([5.0, 5.0]);
    assert!(ed.editing.is_some());
    ed.key(Key::Escape, NONE);
    assert!(ed.scene.elements.is_empty());
    assert!(!ed.can_undo());
}

#[test]
fn a_label_wraps_inside_its_shape_and_grows_it() {
    let mut ed = empty();
    let r = rect(&mut ed, 0.0, 0.0, 120.0, 60.0);
    ed.selection.clear();
    ed.double_click([0.0, 30.0]);
    let edit = ed.editing.clone().unwrap();
    assert_eq!(edit.container.as_deref(), Some(r.as_str()));
    ed.set_text("one two three four");
    let label = label_of(&ed, &r);
    let t = label.text.clone().unwrap();
    // 110 wide inside the padding: 11 characters a line.
    assert_eq!(t.text, "one two\nthree four");
    assert_eq!(label.raw()["originalText"], "one two three four");
    assert_eq!(label.raw()["containerId"], json!(r.clone()));
    assert_eq!(
        (t.text_align.as_str(), t.vertical_align.as_str()),
        ("center", "middle")
    );
    // Two lines need 50 + 10 of padding: the box was 60, so it fits.
    assert_eq!(get(&ed, &r).height, 60.0);
    // Centred in the box.
    assert_eq!(
        (label.x, label.y),
        (5.0 + (110.0 - 100.0) / 2.0, 5.0 + (50.0 / 2.0 - 25.0))
    );
    ed.set_text("one two three four five six");
    assert!(get(&ed, &r).height > 60.0, "a third line grows the box");
    ed.key(Key::Enter, CTRL);
    assert_eq!(ed.selection, vec![r.clone()], "the shape is selected after");

    // The label goes where the shape goes, and is picked through it.
    let before = label_of(&ed, &r);
    drag(&mut ed, [0.0, 10.0], [100.0, 110.0], NONE);
    let after = label_of(&ed, &r);
    assert_eq!((after.x - before.x, after.y - before.y), (100.0, 100.0));
    let inside = [after.x + 5.0, after.y + 5.0];
    assert_eq!(ed.element_at(inside).map(|e| e.id.clone()), Some(r.clone()));

    // Narrowing the shape wraps the label again.
    let e = ed
        .handles()
        .unwrap()
        .handles
        .iter()
        .find(|(h, _)| *h == Handle::E)
        .unwrap()
        .1;
    drag(&mut ed, e, [e[0] - 40.0, e[1]], NONE);
    assert!(label_of(&ed, &r).text.unwrap().text.lines().count() > 3);

    // Duplicating copies the label, bound to the copy.
    ed.selection = vec![r.clone()];
    ed.key(Key::Char('d'), CTRL);
    let copy = ed.selection[0].clone();
    let copied_label = label_of(&ed, &copy);
    assert_eq!(copied_label.raw()["containerId"], json!(copy.clone()));
    assert_ne!(copied_label.id, label_of(&ed, &r).id);

    // Deleting the shape deletes its label.
    ed.selection = vec![r.clone()];
    ed.key(Key::Delete, NONE);
    let label_id = ed.bound_text_of(&r);
    assert!(label_id.is_none() || get(&ed, &label_id.unwrap()).is_deleted);
}

#[test]
fn emptying_an_existing_label_removes_it_from_its_shape() {
    let mut ed = empty();
    let r = rect(&mut ed, 0.0, 0.0, 120.0, 60.0);
    ed.double_click([0.0, 30.0]);
    ed.set_text("hi");
    ed.finish_text();
    ed.selection = vec![r.clone()];
    ed.key(Key::Enter, NONE);
    ed.set_text("   ");
    ed.finish_text();
    assert!(ed.bound_text_of(&r).is_none());
    assert_eq!(get(&ed, &r).raw()["boundElements"], Value::Null);
}

#[test]
fn excalidraw_files_with_labels_round_trip_through_the_editor() {
    let mut ed = empty();
    let r = rect(&mut ed, 0.0, 0.0, 120.0, 60.0);
    ed.double_click([0.0, 30.0]);
    ed.set_text("label");
    ed.finish_text();
    let back = Scene::parse(&ed.scene.to_file_json()).unwrap();
    let label = back.elements.iter().find(|e| e.text.is_some()).unwrap();
    assert_eq!(
        label.text.as_ref().unwrap().container_id.as_deref(),
        Some(r.as_str())
    );
    let keys: Vec<&str> = label.raw().keys().map(String::as_str).collect();
    let tail = &keys[keys.len() - 9..];
    assert_eq!(
        tail,
        [
            "text",
            "fontSize",
            "fontFamily",
            "textAlign",
            "verticalAlign",
            "containerId",
            "originalText",
            "autoResize",
            "lineHeight"
        ]
    );
}

#[test]
fn arrow_labels_sit_on_the_middle_and_follow() {
    let mut ed = empty();
    ed.tool = Tool::Arrow;
    drag(&mut ed, [0.0, 0.0], [200.0, 0.0], NONE);
    let a = ed.selection[0].clone();
    ed.selection.clear();
    ed.double_click([100.0, 0.0]);
    assert_eq!(
        ed.editing.as_ref().and_then(|e| e.container.clone()),
        Some(a.clone())
    );
    ed.set_text("mid");
    ed.finish_text();
    let label = label_of(&ed, &a);
    let centre = [label.x + label.width / 2.0, label.y + label.height / 2.0];
    assert!(
        (centre[0] - 100.0).abs() < 1e-9 && centre[1].abs() < 1e-9,
        "{centre:?}"
    );
    assert_eq!(label.angle, 0.0);
    ed.selection = vec![a.clone()];
    ed.key(Key::Down, SHIFT);
    let moved = label_of(&ed, &a);
    assert_eq!(moved.y - label.y, 5.0);
    // Lines take no label.
    ed.tool = Tool::Line;
    drag(&mut ed, [0.0, 100.0], [200.0, 100.0], NONE);
    let l = ed.selection[0].clone();
    assert!(!ed.edit_at_element(&l));
}

// --- arrows bound to shapes

fn binding(ed: &Editor, arrow: &str, key: &str) -> Option<String> {
    get(ed, arrow)
        .raw()
        .get(key)?
        .get("elementId")?
        .as_str()
        .map(str::to_owned)
}

fn end_of(ed: &Editor, arrow: &str) -> Point {
    let a = get(ed, arrow);
    let pts = &a.linear.as_ref().unwrap().points;
    geometry::to_scene(a, pts[pts.len() - 1])
}

#[test]
fn an_arrow_drawn_onto_a_shape_binds_and_follows_it() {
    let mut ed = empty();
    let r = rect(&mut ed, 200.0, 0.0, 100.0, 100.0);
    ed.tool = Tool::Arrow;
    drag(&mut ed, [0.0, 50.0], [195.0, 50.0], NONE);
    let a = ed.selection[0].clone();
    assert_eq!(binding(&ed, &a, "endBinding").as_deref(), Some(r.as_str()));
    assert_eq!(
        binding(&ed, &a, "startBinding"),
        None,
        "the start is on nothing"
    );
    let bound = get(&ed, &r).raw()["boundElements"].clone();
    assert_eq!(bound, json!([{ "id": a, "type": "arrow" }]));
    let gap = get(&ed, &a).raw()["endBinding"]["gap"].as_f64().unwrap();
    assert!((gap - 5.0).abs() < 1e-9, "{gap}");

    // The shape moves down: the end follows, onto its outline plus the gap.
    ed.selection = vec![r.clone()];
    // By the bottom edge, clear of the handles and of the arrow's end.
    drag(&mut ed, [225.0, 100.0], [225.0, 200.0], NONE);
    let end = end_of(&ed, &a);
    assert!(
        (end[0] - 195.0).abs() < 1e-6 && end[1] > 60.0 && end[1] < 200.0,
        "{end:?}"
    );
    assert_eq!(
        get(&ed, &a).linear.as_ref().unwrap().points[0],
        [0.0, 0.0],
        "the start stays put"
    );

    // One undo puts both back.
    ed.undo();
    assert!((end_of(&ed, &a)[1] - 50.0).abs() < 1e-6);

    // The arrow dragged away on its own lets go.
    ed.selection = vec![a.clone()];
    drag(&mut ed, [100.0, 50.0], [100.0, 400.0], NONE);
    assert_eq!(binding(&ed, &a, "endBinding"), None);
    assert_eq!(get(&ed, &r).raw()["boundElements"], Value::Null);
}

#[test]
fn deleting_either_end_of_a_binding_removes_it() {
    let mut ed = empty();
    let r = rect(&mut ed, 200.0, 0.0, 100.0, 100.0);
    ed.tool = Tool::Arrow;
    drag(&mut ed, [0.0, 50.0], [250.0, 50.0], NONE);
    let a = ed.selection[0].clone();
    assert_eq!(
        binding(&ed, &a, "endBinding").as_deref(),
        Some(r.as_str()),
        "an end inside binds too"
    );
    ed.selection = vec![r.clone()];
    ed.key(Key::Delete, NONE);
    assert_eq!(binding(&ed, &a, "endBinding"), None);
    ed.undo();
    assert_eq!(binding(&ed, &a, "endBinding").as_deref(), Some(r.as_str()));
    ed.selection = vec![a.clone()];
    ed.key(Key::Delete, NONE);
    assert_eq!(get(&ed, &r).raw()["boundElements"], Value::Null);
}

#[test]
fn a_straight_arrow_does_not_bind_both_ends_to_one_shape() {
    let mut ed = empty();
    let r = rect(&mut ed, 0.0, 0.0, 200.0, 200.0);
    ed.tool = Tool::Arrow;
    drag(&mut ed, [50.0, 100.0], [150.0, 100.0], NONE);
    let a = ed.selection[0].clone();
    assert_eq!(
        binding(&ed, &a, "startBinding").as_deref(),
        Some(r.as_str())
    );
    assert_eq!(binding(&ed, &a, "endBinding"), None);
}
