//! `--script FILE`: editor actions replayed at start-up, before the first
//! frame, so a `--screenshot` can show the app after edits. One action per
//! line, in scene coordinates:
//!
//!   tool rectangle|diamond|ellipse|arrow|line|draw|selection
//!   drag X0 Y0 X1 Y1 [shift] [alt]
//!   click X Y [shift]
//!   key K [ctrl] [shift]       K: a letter, delete, escape, left, right, up, down
//!   copy-paste X Y             copy the selection, paste it centred on X Y
//!   save PATH                  write the drawing as Ctrl+S does
//!   text X Y WORDS…            type new text at X Y (`\n` for a new line)
//!   dbl X Y WORDS…             double-click there, type, finish
//!   type X Y WORDS…            double-click there and type, still typing
//!
//! Lines starting with `#` are comments.

use editor::{Editor, Key, Modifiers, Tool};

pub fn run(editor: &mut Editor, text: &str) -> Result<(), String> {
    for (n, line) in text.lines().enumerate().map(|(n, l)| (n + 1, l.trim())) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let flag = |name: &str| words.contains(&name);
        let m = Modifiers {
            shift: flag("shift"),
            alt: flag("alt"),
            ctrl: flag("ctrl"),
        };
        let num = |i: usize| -> Result<f64, String> {
            words
                .get(i)
                .and_then(|w| w.parse().ok())
                .ok_or_else(|| format!("line {n}: expected a number in {line:?}"))
        };
        match words[0] {
            "tool" => {
                editor.tool = match words.get(1).copied() {
                    Some("rectangle") => Tool::Rectangle,
                    Some("diamond") => Tool::Diamond,
                    Some("ellipse") => Tool::Ellipse,
                    Some("arrow") => Tool::Arrow,
                    Some("line") => Tool::Line,
                    Some("draw") => Tool::FreeDraw,
                    Some("selection") => Tool::Selection,
                    other => return Err(format!("line {n}: unknown tool {other:?}")),
                }
            }
            "drag" => {
                let (a, b) = ([num(1)?, num(2)?], [num(3)?, num(4)?]);
                editor.pointer_down(a, m);
                for i in 1..=8 {
                    let t = f64::from(i) / 8.0;
                    editor.pointer_move([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t], m);
                }
                editor.pointer_up(b, m);
            }
            "click" => {
                let p = [num(1)?, num(2)?];
                editor.pointer_down(p, m);
                editor.pointer_up(p, m);
            }
            "key" => {
                let key = match words.get(1).copied() {
                    Some("delete") => Key::Delete,
                    Some("escape") => Key::Escape,
                    Some("left") => Key::Left,
                    Some("right") => Key::Right,
                    Some("up") => Key::Up,
                    Some("down") => Key::Down,
                    Some(k) if k.chars().count() == 1 => Key::Char(k.chars().next().unwrap_or(' ')),
                    other => return Err(format!("line {n}: unknown key {other:?}")),
                };
                editor.key(key, m);
            }
            "copy-paste" => {
                let text = editor
                    .copy()
                    .ok_or_else(|| format!("line {n}: nothing selected to copy"))?;
                editor.paste(&text, [num(1)?, num(2)?]);
            }
            "type" => {
                editor.double_click([num(1)?, num(2)?]);
                editor.set_text(&words[3..].join(" ").replace("\\n", "\n"));
            }
            "text" | "dbl" => {
                let p = [num(1)?, num(2)?];
                let typed = words[3..].join(" ").replace("\\n", "\n");
                if words[0] == "text" {
                    editor.start_text(p);
                } else {
                    editor.double_click(p);
                }
                editor.set_text(&typed);
                editor.finish_text();
            }
            "save" => {
                let path = words
                    .get(1)
                    .ok_or_else(|| format!("line {n}: save needs a path"))?;
                excalidraw_desktop_core::files::write_atomic(
                    std::path::Path::new(path),
                    editor.scene.to_file_json().as_bytes(),
                )?;
            }
            other => return Err(format!("line {n}: unknown action {other:?}")),
        }
    }
    Ok(())
}
