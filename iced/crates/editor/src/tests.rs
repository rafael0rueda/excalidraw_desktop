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
