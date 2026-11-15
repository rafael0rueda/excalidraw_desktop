//! Everything that reads a drawing, run over one: shared by
//! `fuzz_smoke.rs` and the cargo-fuzz target in `iced/fuzz`.

use editor::{Align, Axis, Editor};
use scene::Scene;

/// Parses `text` and, if it parses, draws it, exports it (PNG too if
/// `png`), runs the editor's actions over it, saves and exports again.
pub fn exercise(text: &str, png: bool) {
    let Ok(scene) = Scene::parse(text) else { return };
    for e in scene.visible() {
        let _ = render::prepare(e, &scene, "#ffffff");
    }
    let export = |scene: &Scene| {
        let x = export::Export { scene, only: None, canvas: "#ffffff", transparent: false, font: &|_| None };
        let _ = x.svg(None);
        if png {
            let _ = x.png(0.25, None);
        }
    };
    export(&scene);
    let mut ed = Editor::new(scene);
    ed.select_all();
    ed.translate_selected(5.0, -3.0);
    ed.align(Align::Left);
    ed.distribute(Axis::Y);
    ed.flip(Axis::X);
    ed.group();
    ed.duplicate();
    let copied = ed.copy();
    ed.toggle_lock();
    ed.unlock_all();
    ed.delete_selected();
    ed.undo();
    ed.undo();
    ed.redo();
    if let Some(c) = copied {
        ed.paste(&c, [10.0, 10.0]);
    }
    ed.select_all();
    ed.pointer_down([0.0, 0.0], editor::Modifiers::default());
    ed.pointer_move([40.0, 25.0], editor::Modifiers::default());
    ed.pointer_up([40.0, 25.0], editor::Modifiers::default());
    let _ = ed.scene.to_file_json();
    export(&ed.scene);
}
