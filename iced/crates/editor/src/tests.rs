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
    // By its body, away from the handle in its middle that adds a point.
    drag(&mut ed, [60.0, 50.0], [60.0, 400.0], NONE);
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

// --- the line editor

fn handle_point(ed: &Editor, i: usize) -> Point {
    ed.point_handles()
        .into_iter()
        .find_map(|h| match h {
            PointHandle::Point(j, p) if j == i => Some(p),
            _ => None,
        })
        .unwrap()
}

#[test]
fn dragging_adding_and_removing_line_points() {
    let mut ed = empty();
    ed.tool = Tool::Line;
    drag(&mut ed, [0.0, 0.0], [200.0, 0.0], NONE);
    let l = ed.selection[0].clone();
    // The end, dragged down.
    let end = handle_point(&ed, 1);
    drag(&mut ed, end, [200.0, 100.0], NONE);
    assert_eq!(
        get(&ed, &l).linear.clone().unwrap().points,
        vec![[0.0, 0.0], [200.0, 100.0]]
    );
    // The start, dragged: the element moves so its first point stays at the origin.
    drag(&mut ed, [0.0, 0.0], [-50.0, 0.0], NONE);
    let e = get(&ed, &l).clone();
    assert_eq!((e.x, e.y), (-50.0, 0.0));
    assert_eq!(e.linear.unwrap().points, vec![[0.0, 0.0], [250.0, 100.0]]);
    // The middle handle adds a point.
    let mid = ed
        .point_handles()
        .into_iter()
        .find_map(|h| match h {
            PointHandle::Mid(_, p) => Some(p),
            _ => None,
        })
        .unwrap();
    drag(&mut ed, mid, [100.0, -80.0], NONE);
    let pts = get(&ed, &l).linear.clone().unwrap().points;
    assert_eq!(pts.len(), 3);
    assert_eq!(pts[1], [150.0, -80.0]);
    // Delete removes the point just dragged; undo brings it back.
    ed.key(Key::Delete, NONE);
    assert_eq!(get(&ed, &l).linear.clone().unwrap().points.len(), 2);
    assert!(!get(&ed, &l).is_deleted, "only the point goes");
    ed.undo();
    assert_eq!(get(&ed, &l).linear.clone().unwrap().points.len(), 3);
}

#[test]
fn an_arrow_end_dragged_onto_a_shape_binds() {
    let mut ed = empty();
    let r = rect(&mut ed, 300.0, 0.0, 100.0, 100.0);
    ed.tool = Tool::Arrow;
    drag(&mut ed, [0.0, 50.0], [150.0, 50.0], NONE);
    let a = ed.selection[0].clone();
    assert_eq!(binding(&ed, &a, "endBinding"), None);
    let end = handle_point(&ed, 1);
    ed.pointer_down(end, NONE);
    ed.pointer_move([296.0, 50.0], NONE);
    assert_eq!(
        ed.binding_hint.as_deref(),
        Some(r.as_str()),
        "the target is highlighted on the way"
    );
    ed.pointer_up([296.0, 50.0], NONE);
    assert_eq!(binding(&ed, &a, "endBinding").as_deref(), Some(r.as_str()));
}

#[test]
fn the_panel_shows_what_the_tool_and_selection_can_change() {
    let mut ed = empty();
    assert!(ed.panel().is_none(), "no tool, no selection, no panel");
    ed.tool = Tool::Rectangle;
    let p = ed.panel().unwrap();
    assert_eq!(p.stroke_color, Some(Some("#1e1e1e".into())));
    assert_eq!(p.background_color, Some(Some("transparent".into())));
    assert_eq!(p.fill_style, None, "no fill choices for a transparent background");
    assert_eq!(p.round, Some(Some(true)));
    assert_eq!(p.arrowheads, None);
    assert_eq!(p.font_family, None);
    assert!(!p.actions);

    ed.tool = Tool::Arrow;
    let p = ed.panel().unwrap();
    assert_eq!(p.arrowheads, Some((None, Some("arrow".into()))));
    assert_eq!(p.arrow_round, Some(Some(true)));
    assert_eq!(p.round, None, "arrows have an arrow type instead of edges");

    ed.tool = Tool::Text;
    let p = ed.panel().unwrap();
    assert_eq!((p.font_family, p.font_size), (Some(Some(5.0)), Some(Some(20.0))));
    assert_eq!(p.text_align, Some(Some("left".into())));
    assert_eq!(p.stroke_width, None);

    // A selection: its own values, or nothing picked when they disagree.
    let a = rect(&mut ed, 0.0, 0.0, 50.0, 50.0);
    let b = rect(&mut ed, 100.0, 0.0, 50.0, 50.0);
    ed.selection = vec![a.clone(), b.clone()];
    ed.set_prop(Prop::StrokeWidth(4.0));
    assert_eq!(get(&ed, &a).stroke_width, 4.0);
    assert_eq!(get(&ed, &b).stroke_width, 4.0);
    ed.selection = vec![a.clone()];
    ed.set_prop(Prop::StrokeWidth(1.0));
    ed.selection = vec![a.clone(), b.clone()];
    let p = ed.panel().unwrap();
    assert_eq!(p.stroke_width, Some(None), "1 and 4: nothing picked");
    assert!(p.actions);
    assert_eq!(ed.style.stroke_width, 1.0, "the last choice is what new shapes get");
}

#[test]
fn changing_properties_is_one_undo_step_each() {
    let mut ed = empty();
    let r = rect(&mut ed, 0.0, 0.0, 80.0, 40.0);
    let seed = get(&ed, &r).seed;
    ed.set_prop(Prop::BackgroundColor("#a5d8ff".into()));
    assert_eq!(ed.panel().unwrap().fill_style, Some(Some("solid".into())), "fill choices appear with a colour");
    ed.set_prop(Prop::FillStyle("cross-hatch".into()));
    ed.set_prop(Prop::Roughness(2.0));
    ed.set_prop(Prop::Round(false));
    ed.set_prop(Prop::StrokeStyle("dashed".into()));
    ed.set_prop(Prop::Opacity(60.0));
    let e = get(&ed, &r);
    assert_eq!(
        (e.background_color.as_str(), e.fill_style.as_str(), e.roughness, e.roundness, e.stroke_style.as_str(), e.opacity),
        ("#a5d8ff", "cross-hatch", 2.0, None, "dashed", 60.0)
    );
    assert_ne!(e.seed, seed, "a new sloppiness draws afresh");
    ed.undo();
    assert_eq!(get(&ed, &r).opacity, 100.0);
    assert_eq!(get(&ed, &r).stroke_style, "dashed", "one step back is one change back");
    ed.set_prop(Prop::Round(true));
    assert_eq!(get(&ed, &r).roundness.map(|r| r.kind), Some(3.0), "rectangles round adaptively");

    // New elements take the choices.
    let n = rect(&mut ed, 200.0, 0.0, 30.0, 30.0);
    let e = get(&ed, &n);
    assert_eq!((e.background_color.as_str(), e.fill_style.as_str(), e.roughness), ("#a5d8ff", "cross-hatch", 2.0));
}

#[test]
fn arrow_type_and_arrowheads() {
    let mut ed = empty();
    ed.set_prop(Prop::ArrowRound(false));
    ed.set_prop(Prop::StartArrowhead(Some("dot".into())));
    ed.tool = Tool::Arrow;
    drag(&mut ed, [0.0, 0.0], [100.0, 0.0], NONE);
    let a = ed.selection[0].clone();
    let e = get(&ed, &a);
    assert_eq!(e.roundness, None, "a sharp arrow");
    assert_eq!(e.linear.as_ref().unwrap().start_arrowhead.as_deref(), Some("dot"));
    ed.set_prop(Prop::EndArrowhead(Some("crowfoot_many".into())));
    ed.set_prop(Prop::ArrowRound(true));
    let e = get(&ed, &a);
    assert_eq!(e.linear.as_ref().unwrap().end_arrowhead.as_deref(), Some("crowfoot_many"));
    assert_eq!(e.roundness.map(|r| r.kind), Some(2.0));
    assert_eq!(e.raw()["endArrowhead"], json!("crowfoot_many"));
}

#[test]
fn font_changes_lay_text_out_again() {
    let mut ed = empty();
    ed.key(Key::Char('t'), NONE);
    ed.pointer_down([100.0, 100.0], NONE);
    let id = ed.editing.clone().unwrap().id;
    ed.set_text("abcd");
    ed.key(Key::Escape, NONE);
    let before = get(&ed, &id).clone();
    assert_eq!((before.width, before.height), (40.0, 25.0));
    ed.set_prop(Prop::FontSize(36.0));
    let e = get(&ed, &id).clone();
    // Approx measures half the size a character: 4 × 18 wide, 36 × 1.25 tall.
    assert_eq!((e.width, e.height), (72.0, 45.0));
    assert_eq!((e.x, e.y), (100.0, 100.0 - 10.0), "left-aligned keeps x, centred vertically");
    ed.set_prop(Prop::FontFamily(6.0));
    let e = get(&ed, &id).clone();
    assert_eq!(e.text.as_ref().unwrap().line_height, 1.35, "Nunito's line height");
    assert_eq!(e.height, 36.0 * 1.35);
    ed.step_font_size(true);
    assert_eq!(get(&ed, &id).text.as_ref().unwrap().font_size, 40.0, "36 + 10%, rounded");
    assert_eq!(ed.font_size, 40.0);

    // A shape's label: the shape is selected, the label changes.
    let r = rect(&mut ed, 0.0, 300.0, 120.0, 60.0);
    ed.selection.clear();
    ed.double_click([0.0, 330.0]);
    ed.set_text("one two");
    ed.key(Key::Escape, NONE);
    ed.selection = vec![r.clone()];
    let p = ed.panel().unwrap();
    assert_eq!(p.font_size, Some(Some(40.0)), "the label's size, shown for its shape");
    assert_eq!(p.vertical_align, Some(Some("middle".into())));
    ed.set_prop(Prop::FontSize(16.0));
    ed.set_prop(Prop::VerticalAlign("top".into()));
    let l = label_of(&ed, &r);
    assert_eq!(l.text.as_ref().unwrap().font_size, 16.0);
    assert_eq!(l.y, get(&ed, &r).y + 5.0, "at the top, inside the padding");
    ed.set_prop(Prop::StrokeColor("#e03131".into()));
    assert_eq!(label_of(&ed, &r).stroke_color, "#e03131", "colour reaches the label");
}

#[test]
fn copying_and_pasting_styles() {
    let mut ed = empty();
    let a = rect(&mut ed, 0.0, 0.0, 50.0, 50.0);
    ed.set_prop(Prop::StrokeColor("#2f9e44".into()));
    ed.set_prop(Prop::StrokeWidth(4.0));
    ed.style = Style::default();
    ed.tool = Tool::Ellipse;
    drag(&mut ed, [100.0, 0.0], [150.0, 50.0], NONE);
    let b = ed.selection[0].clone();
    ed.selection = vec![a.clone()];
    assert!(ed.copy_styles());
    ed.selection = vec![b.clone()];
    ed.paste_styles();
    let e = get(&ed, &b);
    assert_eq!((e.stroke_color.as_str(), e.stroke_width), ("#2f9e44", 4.0));
    assert_eq!(e.roundness, None, "an ellipse cannot take the rectangle's corners");
    ed.undo();
    assert_eq!(get(&ed, &b).stroke_width, 2.0);
}

#[test]
fn inserting_an_image_and_pasting_text() {
    let mut ed = empty();
    let file = ImageFile { id: "abc".into(), mime: "image/png".into(), data_url: "data:image/png;base64,AAAA".into(), width: 400.0, height: 200.0 };
    let id = ed.insert_image(&file, [100.0, 100.0], 150.0).unwrap();
    let e = get(&ed, &id).clone();
    assert_eq!((e.x, e.y, e.width, e.height), (100.0 - 150.0, 100.0 - 75.0, 300.0, 150.0), "kept within the height, centred");
    assert_eq!(e.raw()["strokeColor"], json!("transparent"));
    assert_eq!(e.raw()["fileId"], json!("abc"));
    assert_eq!(e.raw()["roundness"], Value::Null);
    assert_eq!(ed.scene.file("abc").map(|(m, _)| m), Some("image/png"));
    assert_eq!(ed.selection, vec![id.clone()]);
    ed.undo();
    assert!(ed.scene.by_id(&id).is_none());

    // Two lines, a blank between: two texts, the second lower.
    assert!(ed.paste_text("hello\n\nworld wide", [0.0, 0.0], 200.0));
    assert_eq!(ed.selection.len(), 2);
    let a = get(&ed, &ed.selection[0].clone()).clone();
    let b = get(&ed, &ed.selection[1].clone()).clone();
    assert_eq!((a.x, a.width), (-25.0, 50.0), "centred on the point");
    assert!(b.y > a.y + a.height + 10.0, "a blank line leaves a gap");
    // Longer than the width: wrapped, and no longer auto-sized.
    let mut ed = empty();
    ed.paste_text("one two three four five six", [0.0, 0.0], 100.0);
    let t = get(&ed, &ed.selection[0].clone()).clone();
    assert!(t.text.as_ref().unwrap().text.contains('\n'));
    assert_eq!(t.raw()["autoResize"], json!(false));
    assert!(!ed.paste_text("  \n ", [0.0, 0.0], 100.0), "nothing to paste");
}

#[test]
fn the_eraser_marks_what_it_passes_and_deletes_on_release() {
    let mut ed = empty();
    let a = rect(&mut ed, 0.0, 0.0, 50.0, 50.0);
    let b = rect(&mut ed, 100.0, 0.0, 50.0, 50.0);
    let c = rect(&mut ed, 200.0, 0.0, 50.0, 50.0);
    // a and b grouped: touching b takes a too.
    ed.selection = vec![a.clone(), b.clone()];
    ed.group();
    ed.key(Key::Char('e'), NONE);
    assert_eq!(ed.tool, Tool::Eraser);
    ed.pointer_down([150.0, 25.0], NONE);
    ed.pointer_move([150.0, 60.0], NONE);
    assert_eq!(ed.erasing, HashSet::from([a.clone(), b.clone()]), "the group, marked");
    ed.pointer_move([200.0, 25.0], NONE);
    assert!(ed.erasing.contains(&c));
    ed.pointer_move([210.0, 25.0], Modifiers { alt: true, ..NONE });
    assert!(!ed.erasing.contains(&c), "Alt takes it back");
    ed.pointer_up([210.0, 25.0], NONE);
    assert!(ed.erasing.is_empty());
    assert!(get(&ed, &a).is_deleted && get(&ed, &b).is_deleted);
    assert!(!get(&ed, &c).is_deleted);
    ed.undo();
    assert!(!get(&ed, &a).is_deleted, "one undo brings them back");

    // Escape erases nothing.
    ed.pointer_down([225.0, 0.0], NONE);
    assert!(ed.erasing.contains(&c));
    ed.key(Key::Escape, NONE);
    assert!(ed.erasing.is_empty());
    assert!(!get(&ed, &c).is_deleted);
}

#[test]
fn align_distribute_flip_and_lock() {
    let mut ed = empty();
    let a = rect(&mut ed, 0.0, 0.0, 50.0, 50.0);
    let b = rect(&mut ed, 100.0, 40.0, 30.0, 30.0);
    let c = rect(&mut ed, 300.0, 10.0, 50.0, 20.0);
    ed.selection = vec![a.clone(), b.clone(), c.clone()];
    ed.key(Key::Up, CTRL_SHIFT);
    assert_eq!([get(&ed, &a).y, get(&ed, &b).y, get(&ed, &c).y], [0.0, 0.0, 0.0], "aligned to the top");
    ed.align(Align::CenterY);
    assert_eq!(get(&ed, &b).y, 25.0 - 15.0);
    // Widths 50 + 30 + 50 across 0..350: gaps of 110.
    ed.key(Key::Char('h'), Modifiers { alt: true, ..NONE });
    assert_eq!(get(&ed, &b).x, 160.0);
    assert_eq!((get(&ed, &a).x, get(&ed, &c).x), (0.0, 300.0), "the outer ones stay");
    ed.undo();
    assert_eq!(get(&ed, &b).x, 100.0, "one undo step");

    // Flip: mirrored about the selection's middle, turns reversed.
    ed.selection = vec![a.clone(), c.clone()];
    ed.key(Key::Char('h'), SHIFT);
    assert_eq!((get(&ed, &a).x, get(&ed, &c).x), (300.0, 0.0));
    ed.tool = Tool::Line;
    drag(&mut ed, [0.0, 200.0], [100.0, 250.0], NONE);
    let l = ed.selection[0].clone();
    ed.flip(Axis::X);
    let e = get(&ed, &l);
    assert_eq!(e.linear.as_ref().unwrap().points, vec![[0.0, 0.0], [-100.0, 50.0]]);
    assert_eq!(e.x, 100.0, "the same place, mirrored");

    // Lock: cannot be picked or moved; unlock all brings it back selected.
    ed.selection = vec![a.clone()];
    ed.key(Key::Char('l'), CTRL_SHIFT);
    assert_eq!(get(&ed, &a).raw()["locked"], json!(true));
    ed.selection.clear();
    assert!(ed.element_at([310.0, 10.0]).is_none_or(|e| e.id != a));
    ed.select_all();
    assert!(!ed.selection.contains(&a));
    ed.selection.clear();
    ed.unlock_all();
    assert_eq!(ed.selection, vec![a.clone()]);
    assert_eq!(get(&ed, &a).raw()["locked"], json!(false));
}

#[test]
fn links_are_set_cleaned_and_found_by_their_icon() {
    assert_eq!(normalize_link("  https://example.com/a \"b\" "), "https://example.com/a %22b%22");
    assert_eq!(normalize_link("JavaScript:alert(1)"), "about:blank");
    assert_eq!(normalize_link(" java\tscript:alert(1)"), "about:blank");
    assert_eq!(normalize_link("mailto:someone@example.com"), "mailto:someone@example.com");

    let mut ed = empty();
    let r = rect(&mut ed, 0.0, 0.0, 100.0, 50.0);
    assert_eq!(ed.link_target(), Some((r.clone(), String::new())));
    ed.set_link("https://example.com");
    assert_eq!(get(&ed, &r).raw()["link"], json!("https://example.com"));
    // The icon sits above the top right corner; only when not selected.
    let [x, y, w, h] = link_handle(get(&ed, &r), 1.0);
    let icon = [x + w / 2.0, y + h / 2.0];
    assert!(icon[0] > 95.0 && icon[1] < 0.0, "{icon:?}");
    assert_eq!(ed.link_at(icon), None, "selected: no link to follow");
    ed.selection.clear();
    assert_eq!(ed.link_at(icon).as_deref(), Some("https://example.com"));
    assert_eq!(ed.link_at([50.0, 25.0]), None);
    ed.selection = vec![r.clone()];
    ed.set_link("   ");
    assert_eq!(get(&ed, &r).raw()["link"], Value::Null, "empty removes it");
    ed.undo();
    assert_eq!(get(&ed, &r).link.as_deref(), Some("https://example.com"));

    // Element links point inside the drawing.
    let other = rect(&mut ed, 300.0, 0.0, 10.0, 10.0);
    assert_eq!(ed.link_targets(&format!("https://excalidraw.com/?element={other}")), vec![other.clone()]);
    assert!(ed.link_targets("https://example.com/?q=1").is_empty());
}

#[test]
fn the_grid_snaps_new_shapes_moves_and_resizes() {
    let mut ed = empty();
    assert_eq!(ed.grid_size(), None);
    ed.toggle_grid();
    assert_eq!(ed.grid_size(), Some(20.0));
    assert_eq!(ed.scene.app_state("gridModeEnabled"), Some(&json!(true)), "kept with the drawing");
    let r = rect(&mut ed, 13.0, 27.0, 50.0, 50.0);
    let e = get(&ed, &r);
    assert_eq!((e.x, e.y, e.width, e.height), (20.0, 20.0, 40.0, 60.0), "corners on the grid: 13→20, 63→60, 27→20, 77→80");
    // A move lands the top left on the grid; Ctrl moves freely.
    drag(&mut ed, [30.0, 30.0], [37.0, 52.0], NONE);
    assert_eq!((get(&ed, &r).x, get(&ed, &r).y), (20.0, 40.0));
    drag(&mut ed, [30.0, 50.0], [33.0, 50.0], CTRL);
    assert_eq!(get(&ed, &r).x, 23.0);
    ed.toggle_grid();
    assert_eq!(ed.grid_size(), None);
    let free = rect(&mut ed, 13.0, 207.0, 50.0, 50.0);
    assert_eq!(get(&ed, &free).x, 13.0, "off: no snapping");
}
