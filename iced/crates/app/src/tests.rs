//! The file, tab and session flow, run through `App::update` as the window
//! would, against a config directory of its own. One test, in order: the
//! config directory is process-wide.

use super::*;
use editor::Modifiers;
use excalidraw_desktop_core::store;

const NONE: Modifiers = Modifiers { shift: false, alt: false, ctrl: false };

fn boot(paths: Vec<PathBuf>) -> App {
    App::boot(Args { paths, ..Args::default() }).0
}

fn send(app: &mut App, m: Message) {
    let _ = app.update(m);
}

fn draw_rectangle(app: &mut App, at: [f64; 2]) {
    send(app, Message::Tool(Tool::Rectangle));
    send(app, Message::Input(Input::Down(at, NONE)));
    send(app, Message::Input(Input::Move([at[0] + 80.0, at[1] + 50.0], NONE)));
    send(app, Message::Input(Input::Up([at[0] + 80.0, at[1] + 50.0], NONE)));
}

fn autosave(app: &mut App) {
    send(app, Message::Autosave(Instant::now() + session::QUIET * 2));
}

fn meta(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("session/meta.json")).unwrap()).unwrap()
}

fn rectangles(json: &str) -> usize {
    scene::Scene::parse(json).unwrap().elements.iter().filter(|e| e.kind == scene::Kind::Rectangle && !e.is_deleted).count()
}

#[test]
fn files_tabs_autosave_recovery_and_quit() {
    let tmp = std::env::temp_dir().join(format!("excalidraw-iced-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("config")).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", tmp.join("config"));
    store::set_app_dir(APP_DIR);
    let dir = store::config_dir();
    store::ensure_private_dir(&dir).unwrap();
    let a = tmp.join("a.excalidraw");
    std::fs::write(&a, doc::EMPTY).unwrap();
    let a = std::fs::canonicalize(&a).unwrap();

    // A first run, with a drawing named on the command line.
    let mut app = boot(vec![a.clone()]);
    assert!(app.prompt.is_none());
    assert_eq!(app.tabs.len(), 1, "the drawing takes the place of the blank tab");
    assert_eq!(app.tab().path.as_deref(), Some(a.as_path()));
    assert_eq!(meta(&dir)["clean_exit"], false, "a running session is not clean");
    assert_eq!(app.recent.len(), 1);

    // Drawing makes it dirty; a pause autosaves it, to the session only.
    draw_rectangle(&mut app, [0.0, 0.0]);
    assert!(app.tab().dirty());
    autosave(&mut app);
    let id_a = app.tab().id.clone();
    assert_eq!(rectangles(&std::fs::read_to_string(dir.join(format!("session/{id_a}.excalidraw"))).unwrap()), 1);
    assert_eq!(rectangles(&std::fs::read_to_string(&a).unwrap()), 0, "never to the drawing's own file");

    // A second, untitled tab with its own drawing.
    send(&mut app, Message::NewTab);
    assert_eq!((app.tabs.len(), app.active), (2, 1));
    draw_rectangle(&mut app, [0.0, 0.0]);
    draw_rectangle(&mut app, [200.0, 0.0]);
    autosave(&mut app);
    let m = meta(&dir);
    assert_eq!(m["tabs"].as_array().unwrap().len(), 2);
    assert_eq!(m["tabs"][1]["dirty"], true);

    // Ctrl+W on a dirty tab asks; Escape cancels and keeps it.
    let ctrl = keyboard::Modifiers::CTRL;
    let press = |key: Key, modifiers| {
        Message::Key(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    };
    send(&mut app, press(Key::Character("w".into()), ctrl));
    assert!(matches!(app.prompt, Some(Prompt::Unsaved { then: After::Close, .. })));
    send(&mut app, press(Key::Named(Named::Escape), keyboard::Modifiers::empty()));
    assert!(app.prompt.is_none());
    assert_eq!(app.tabs.len(), 2);

    // The File menu stays open while the clocks tick and the pointer moves,
    // and a click on the drawing only closes it.
    send(&mut app, Message::ToggleMenu);
    autosave(&mut app);
    send(&mut app, Message::Input(Input::Move([5.0, 5.0], NONE)));
    assert!(app.menu_open);
    let before = app.tab().doc.editor.scene.elements.len();
    send(&mut app, Message::Input(Input::Down([300.0, 300.0], NONE)));
    send(&mut app, Message::Input(Input::Up([300.0, 300.0], NONE)));
    assert!(!app.menu_open);
    assert_eq!(app.tab().doc.editor.scene.elements.len(), before);

    // Ctrl+Tab and Ctrl+1 move between tabs.
    send(&mut app, press(Key::Named(Named::Tab), ctrl));
    assert_eq!(app.active, 0);
    send(&mut app, press(Key::Character("2".into()), ctrl));
    assert_eq!(app.active, 1);

    // The run dies: no clean exit. The next launch asks, and recovers.
    drop(app);
    let mut app = boot(Vec::new());
    let Some(Prompt::Recover { what, .. }) = &app.prompt else { panic!("expected the recovery question") };
    assert_eq!(what, "2 drawings were");
    send(&mut app, Message::Answer(Answer::Save));
    assert!(app.prompt.is_none());
    assert_eq!(app.tabs.len(), 2);
    assert!(app.tabs.iter().all(Tab::dirty), "recovered work is still unsaved");
    assert_eq!(rectangles(&app.tabs[0].doc.editor.scene.to_file_json()), 1);
    assert_eq!(rectangles(&app.tabs[1].doc.editor.scene.to_file_json()), 2);

    // Ctrl+S saves the titled drawing in place.
    send(&mut app, Message::SelectTab(0));
    send(&mut app, press(Key::Character("s".into()), ctrl));
    assert!(!app.tab().dirty());
    assert_eq!(rectangles(&std::fs::read_to_string(&a).unwrap()), 1);

    // More on it, then quit, discarding everything: the file stays as
    // saved, the untitled drawing goes, and the session is clean.
    draw_rectangle(&mut app, [0.0, 200.0]);
    send(&mut app, Message::Quit);
    let Some(Prompt::Unsaved { title, then: After::Quit, .. }) = &app.prompt else { panic!("expected a question") };
    assert_eq!(title, "a.excalidraw");
    send(&mut app, Message::Answer(Answer::Discard));
    let Some(Prompt::Unsaved { title, .. }) = &app.prompt else { panic!("expected a second question") };
    assert_eq!(title, "Untitled");
    send(&mut app, Message::Answer(Answer::Discard));
    assert!(app.prompt.is_none());
    let m = meta(&dir);
    assert_eq!(m["clean_exit"], true);
    assert_eq!(m["tabs"].as_array().unwrap().len(), 1, "the discarded untitled drawing is gone");
    assert_eq!(m["tabs"][0]["dirty"], false);
    assert_eq!(rectangles(&std::fs::read_to_string(&a).unwrap()), 1);

    // A clean session comes back without a question, as the files are.
    drop(app);
    let mut app = boot(Vec::new());
    assert!(app.prompt.is_none());
    assert_eq!(app.tabs.len(), 1);
    assert!(!app.tab().dirty());
    assert_eq!(rectangles(&app.tab().doc.editor.scene.to_file_json()), 1);

    // Handed over from a second launch: opened in a new tab; again, brought
    // forward. A file that is not a drawing is an error said once.
    let b = tmp.join("b.excalidraw");
    std::fs::write(&b, doc::EMPTY).unwrap();
    let bad = tmp.join("bad.excalidraw");
    std::fs::write(&bad, "not json").unwrap();
    send(&mut app, Message::HandedOver(vec![b.clone()]));
    assert_eq!((app.tabs.len(), app.active), (2, 1));
    send(&mut app, Message::SelectTab(0));
    send(&mut app, Message::HandedOver(vec![b.clone()]));
    assert_eq!((app.tabs.len(), app.active), (2, 1));
    send(&mut app, Message::OpenPath(bad));
    assert!(matches!(app.prompt, Some(Prompt::Error(_))));
    assert_eq!(app.tabs.len(), 2);
    send(&mut app, Message::Answer(Answer::Cancel));

    // Closing the last tab leaves an empty one.
    { let id = app.tabs[1].id.clone(); send(&mut app, Message::CloseTab(id)); }
    { let id = app.tabs[0].id.clone(); send(&mut app, Message::CloseTab(id)); }
    assert_eq!(app.tabs.len(), 1);
    assert!(app.tab().path.is_none() && app.tab().doc.is_blank());

    // Save As refuses a file open in another tab.
    send(&mut app, Message::OpenPath(a.clone()));
    send(&mut app, Message::NewTab);
    let id = app.tab().id.clone();
    send(&mut app, Message::SaveAsPicked(id.clone(), Some(a.clone()), None));
    assert!(matches!(app.prompt, Some(Prompt::Error(_))));
    send(&mut app, Message::Answer(Answer::Cancel));
    // And adds the extension a name without one is missing.
    send(&mut app, Message::SaveAsPicked(id, Some(tmp.join("c")), None));
    assert!(tmp.join("c.excalidraw").is_file());
    assert_eq!(app.tab().title(), "c.excalidraw");

    let _ = std::fs::remove_dir_all(&tmp);
}
