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
    send(&mut app, Message::ToggleMenu(Menu::File));
    autosave(&mut app);
    send(&mut app, Message::Input(Input::Move([5.0, 5.0], NONE)));
    assert_eq!(app.menu, Some(Menu::File));
    let before = app.tab().doc.editor.scene.elements.len();
    send(&mut app, Message::Input(Input::Down([300.0, 300.0], NONE)));
    send(&mut app, Message::Input(Input::Up([300.0, 300.0], NONE)));
    assert_eq!(app.menu, None);
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

    // --- themes. Following the desktop (the default): its light theme until
    // it turns dark.
    use theme_editor::Edit;
    assert_eq!(app.themes.chosen().id, "kanagawa-lotus");
    send(&mut app, Message::SystemDark(true));
    assert_eq!(app.themes.chosen().id, "kanagawa-wave");

    // New elements draw in the theme's stroke; a colour picked by hand
    // survives a change of theme, the theme's own does not.
    draw_rectangle(&mut app, [0.0, 0.0]);
    let last = |app: &App| app.tab().doc.editor.scene.visible().last().unwrap().stroke_color.clone();
    assert_eq!(last(&app), "#DCD7BA");
    app.tab_mut().doc.editor.style.stroke_color = "#ff0000".into();
    send(&mut app, Message::SelectTheme("nord".into()));
    assert_eq!(app.tab().doc.editor.style.stroke_color, "#ff0000");
    app.tab_mut().doc.editor.style.stroke_color = "#d8dee9".into();
    send(&mut app, Message::SelectTheme("dracula".into()));
    assert_eq!(app.tab().doc.editor.style.stroke_color, "#f8f8f2");
    let settings: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap()).unwrap();
    assert_eq!(settings["theme"], "dracula");

    // A saved drawing carries the theme's canvas colour, as Tauri's do.
    send(&mut app, Message::Save);
    let saved = scene::Scene::parse(&std::fs::read_to_string(tmp.join("c.excalidraw")).unwrap()).unwrap();
    assert_eq!(saved.view_background_color(), Some("#282a36"));

    // The editor paints its draft, a half-typed colour as it was, and
    // nothing is kept until Save.
    send(&mut app, Message::Theme(Edit::Open));
    assert_eq!(app.panel.as_ref().unwrap().draft.id, "dracula");
    send(&mut app, Message::Theme(Edit::Color(5, "#12".into())));
    assert_eq!(app.look().accent, theme::color("#bd93f9"));
    send(&mut app, Message::Theme(Edit::Color(5, " #ff00aa".into())));
    assert_eq!(app.look().accent, theme::color("#ff00aa"));
    assert_eq!(app.themes.chosen().colors.accent, "#bd93f9");
    // Closing with changes asks; keeping them goes on editing.
    send(&mut app, Message::Theme(Edit::Close));
    assert!(matches!(app.prompt, Some(Prompt::DiscardTheme { .. })));
    send(&mut app, Message::Answer(Answer::Cancel));
    assert!(app.panel.is_some());
    // Save as new: a fresh id, the original left alone, and it is put on.
    send(&mut app, Message::Theme(Edit::Name("My Dracula".into())));
    send(&mut app, Message::Theme(Edit::SaveAsNew));
    let file = dir.join("themes/my-dracula.json");
    let written = theme::parse(&serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap()).unwrap();
    assert_eq!((written.name.as_str(), written.colors.accent.as_str()), ("My Dracula", "#ff00aa"));
    assert_eq!(app.themes.settings.theme, "my-dracula");
    assert!(!app.panel.as_ref().unwrap().modified());
    assert_eq!(app.themes.find("dracula").unwrap().colors.accent, "#bd93f9");
    // Delete asks first; then the file goes, and so does the editor.
    send(&mut app, Message::Theme(Edit::Delete));
    assert!(matches!(app.prompt, Some(Prompt::DeleteTheme { .. })));
    send(&mut app, Message::Answer(Answer::Confirm));
    assert!(!file.exists() && app.panel.is_none());

    // Reload says what it skipped, and why.
    std::fs::write(dir.join("themes/broken.json"), "{").unwrap();
    std::fs::write(dir.join("themes/misnamed.json"), theme::serialize(&theme::PRESETS[2])).unwrap();
    send(&mut app, Message::ReloadThemes);
    let Some(Prompt::Info(text)) = &app.prompt else { panic!("expected what Reload found") };
    assert!(text.starts_with("Loaded 0 user themes from "), "{text}");
    assert!(text.contains("broken.json: ") && text.contains("so it has to be named nord.json"), "{text}");
    send(&mut app, Message::Answer(Answer::Cancel));

    // Export: the choices are remembered in export.json, as Tauri's are.
    use exporting::{Choice, Kind};
    send(&mut app, Message::ExportChoice(Choice::Embed));
    send(&mut app, Message::ExportChoice(Choice::Scale(1)));
    let prefs = excalidraw_desktop_core::settings::load_export_preferences();
    assert!(prefs.embed_scene && prefs.scale == 1 && !prefs.transparent);
    // The selection, with nothing selected, is a question answered first.
    app.tab_mut().doc.editor.selection.clear();
    send(&mut app, Message::Export(Kind::PngSelection));
    assert!(matches!(&app.prompt, Some(Prompt::Error(e)) if e.contains("Select something")));
    send(&mut app, Message::Answer(Answer::Cancel));
    // The image carries the drawing, on the theme's canvas.
    send(&mut app, Message::SelectTheme("dracula".into()));
    let png = app.job(false).unwrap().bytes(Kind::Png).unwrap();
    let inside = scene::Scene::parse(&export::decode_png(&png).expect("embedded")).unwrap();
    assert_eq!(inside.view_background_color(), Some("#282a36"));
    assert_eq!(inside.visible().count(), app.tab().doc.editor.scene.visible().count());
    let svg = String::from_utf8(app.job(false).unwrap().bytes(Kind::Svg).unwrap()).unwrap();
    assert!(svg.contains(r##"fill="#282a36"/>"##) && export::decode_svg(&svg).is_some());

    // The library: a first run starts with the ER shapes; sets merge
    // without doubles; it is all saved where the Tauri app keeps it.
    use library::{Msg, Set};
    let lib_file = dir.join("library.excalidrawlib");
    assert_eq!(app.library.items.len(), 15);
    assert!(!lib_file.exists(), "nothing written until something changes");
    send(&mut app, Message::Library(Msg::AddSet(Set::Er)));
    assert_eq!(app.library.items.len(), 15, "already there");
    send(&mut app, Message::Library(Msg::AddSet(Set::Wave)));
    assert_eq!(app.library.items.len(), 44);
    assert!(app.library.open);
    let saved = library::parse(&std::fs::read_to_string(&lib_file).unwrap()).unwrap();
    assert_eq!(saved.len(), 44);
    assert_eq!(library::name(&saved[0]), Some("Kanagawa Wave · Table"), "new items first");
    // An item goes on the drawing as new elements, selected.
    let before = app.tab().doc.editor.scene.visible().count();
    let first = library::id(&app.library.items[0]).to_owned();
    send(&mut app, Message::Library(Msg::Insert(first.clone())));
    let added = app.tab().doc.editor.scene.visible().count() - before;
    assert!(added > 1 && app.tab().doc.editor.selection.len() == added);
    // The selection back into the library, as a new item.
    send(&mut app, Message::Library(Msg::AddSelection));
    assert_eq!(app.library.items.len(), 45);
    // Removing asks first.
    send(&mut app, Message::Library(Msg::Remove(first.clone())));
    assert!(matches!(app.prompt, Some(Prompt::RemoveLibraryItem { .. })));
    send(&mut app, Message::Answer(Answer::Confirm));
    assert_eq!(app.library.items.len(), 44);
    assert_eq!(library::parse(&std::fs::read_to_string(&lib_file).unwrap()).unwrap().len(), 44);
    // An old (version 1) library file imports; a broken one says why.
    let v1 = tmp.join("old.excalidrawlib");
    std::fs::write(&v1, r#"{"type":"excalidrawlib","version":1,"library":[[{"type":"rectangle","id":"x","x":0,"y":0,"width":10,"height":10}]]}"#).unwrap();
    let bad = tmp.join("bad.excalidrawlib");
    std::fs::write(&bad, "{").unwrap();
    send(&mut app, Message::Library(Msg::Imported(vec![v1, bad])));
    assert_eq!(app.library.items.len(), 45);
    assert!(matches!(&app.prompt, Some(Prompt::Error(e)) if e.contains("bad.excalidrawlib")));
    send(&mut app, Message::Answer(Answer::Cancel));
    // A library file that cannot be read is never written over.
    std::fs::write(&lib_file, "not json").unwrap();
    let (broken, problem) = library::Library::load(true);
    assert!(problem.is_some() && broken.items.is_empty());
    app.library = broken;
    send(&mut app, Message::Library(Msg::AddSet(Set::Lotus)));
    assert_eq!(app.library.items.len(), 29, "added, in memory");
    assert_eq!(std::fs::read_to_string(&lib_file).unwrap(), "not json");
    assert!(app.status.as_deref().is_some_and(|s| s.contains("not saved")), "{:?}", app.status);

    let _ = std::fs::remove_dir_all(&tmp);
}

/// The properties panel through `App::update`, in a development run (no
/// config directory).
#[test]
fn the_properties_panel() {
    use properties::{Layer, Msg, Popup};
    let mut app = App::boot(Args { screenshot: Some("/dev/null".into()), chrome: true, ..Args::default() }).0;
    assert!(properties::view(&app).is_none(), "nothing to show yet");
    draw_rectangle(&mut app, [0.0, 0.0]);
    draw_rectangle(&mut app, [200.0, 0.0]);
    let ed = |app: &App| app.tab().doc.editor.selection.clone();
    let last = ed(&app)[0].clone();
    assert!(properties::view(&app).is_some(), "a selection shows it");

    send(&mut app, Message::Props(Msg::Set(editor::Prop::StrokeColor("#2f9e44".into()))));
    let stroke = |app: &App, id: &str| app.tab().doc.editor.scene.by_id(id).unwrap().stroke_color.clone();
    assert_eq!(stroke(&app, &last), "#2f9e44");

    // The palette: opened with the colour in its hex field, typed into.
    send(&mut app, Message::Props(Msg::Popup(Popup::Stroke)));
    assert_eq!((app.popup, app.hex.as_str()), (Some(Popup::Stroke), "2f9e44"));
    send(&mut app, Message::Props(Msg::Hex("#ab".into())));
    assert_eq!(stroke(&app, &last), "#2f9e44", "half a colour changes nothing");
    send(&mut app, Message::Props(Msg::Hex("#abc".into())));
    assert_eq!(stroke(&app, &last), "#abc");
    assert_eq!(app.hex, "abc");
    // Escape closes the popup before anything else.
    let esc = Message::Key(keyboard::Event::KeyPressed {
        key: Key::Named(Named::Escape),
        modified_key: Key::Named(Named::Escape),
        physical_key: keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    });
    send(&mut app, esc.clone());
    assert_eq!(app.popup, None);
    assert_eq!(ed(&app), vec![last.clone()], "the selection stays");

    // Layers and actions.
    send(&mut app, Message::Props(Msg::Layer(Layer::ToBack)));
    assert_eq!(app.tab().doc.editor.scene.order()[0], last);
    send(&mut app, Message::Props(Msg::Duplicate));
    let copy = ed(&app)[0].clone();
    assert_ne!(copy, last);
    send(&mut app, Message::Props(Msg::Delete));
    assert!(app.tab().doc.editor.scene.by_id(&copy).is_none_or(|e| e.is_deleted));

    // Ctrl+Alt+C on one, Ctrl+Alt+V on the other.
    let key = |c: &str, modifiers| {
        Message::Key(keyboard::Event::KeyPressed {
            key: Key::Character(c.into()),
            modified_key: Key::Character(c.into()),
            physical_key: keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    };
    let ctrl_alt = keyboard::Modifiers::CTRL | keyboard::Modifiers::ALT;
    app.tab_mut().doc.editor.selection = vec![last.clone()];
    send(&mut app, key("c", ctrl_alt));
    assert_eq!(app.status.as_deref(), Some("Copied styles"));
    let other = app.tab().doc.editor.scene.visible().map(|e| e.id.clone()).find(|id| *id != last).unwrap();
    app.tab_mut().doc.editor.selection = vec![other.clone()];
    send(&mut app, key("v", ctrl_alt));
    assert_eq!(stroke(&app, &other), "#abc");

    // Plain text pasted: a text per line, selected.
    send(&mut app, Message::Paste(Some("first\nsecond".into())));
    let texts: Vec<String> = ed(&app);
    assert_eq!(texts.len(), 2);
    assert!(texts.iter().all(|id| app.tab().doc.editor.scene.by_id(id).is_some_and(|e| e.text.is_some())));
}

#[test]
fn links_through_the_app() {
    let mut app = App::boot(Args { screenshot: Some("/dev/null".into()), chrome: true, ..Args::default() }).0;
    draw_rectangle(&mut app, [0.0, 0.0]);
    let id = app.tab().doc.editor.selection[0].clone();
    send(&mut app, Message::EditLink);
    assert!(matches!(app.prompt, Some(Prompt::Link { .. })));
    send(&mut app, Message::LinkDraft(format!("https://excalidraw.com/?element={id}")));
    send(&mut app, Message::Answer(Answer::Confirm));
    assert!(app.prompt.is_none());
    assert!(app.tab().doc.editor.scene.by_id(&id).unwrap().link.as_deref().unwrap().ends_with(&id));

    // Its icon, clicked with the element not selected, brings it into view
    // and does not start a gesture.
    app.tab_mut().doc.editor.selection.clear();
    let e = app.tab().doc.editor.scene.by_id(&id).unwrap().clone();
    let [x, y, w, h] = editor::link_handle(&e, f64::from(app.tab().view.zoom));
    app.tab_mut().view = View { pan: iced::Vector::new(5000.0, 5000.0), zoom: 1.0 };
    send(&mut app, Message::Input(Input::Down([x + w / 2.0, y + h / 2.0], NONE)));
    send(&mut app, Message::Input(Input::Up([x + w / 2.0, y + h / 2.0], NONE)));
    assert_ne!(app.tab().view.pan.x, 5000.0, "scrolled to it");
    assert!(app.tab().doc.editor.selection.is_empty());

    // A script link is cleaned and never opened.
    app.tab_mut().doc.editor.selection = vec![id.clone()];
    send(&mut app, Message::EditLink);
    send(&mut app, Message::LinkDraft("javascript:alert(1)".into()));
    send(&mut app, Message::Answer(Answer::Confirm));
    assert_eq!(app.tab().doc.editor.scene.by_id(&id).unwrap().link.as_deref(), Some("about:blank"));
    app.follow_link("about:blank");
    assert!(app.status.as_deref().is_some_and(|s| s.starts_with("Only web and mail links")));
    // Remove.
    send(&mut app, Message::EditLink);
    send(&mut app, Message::Answer(Answer::Discard));
    assert_eq!(app.tab().doc.editor.scene.by_id(&id).unwrap().link, None);
}

#[test]
fn the_right_click_menu_and_the_grid() {
    use context_menu::Action;
    let mut app = App::boot(Args { screenshot: Some("/dev/null".into()), chrome: true, ..Args::default() }).0;
    draw_rectangle(&mut app, [0.0, 0.0]);
    let id = app.tab().doc.editor.selection[0].clone();
    app.tab_mut().doc.editor.selection.clear();
    // On its outline: it is selected, and the menu is the selection's.
    send(&mut app, Message::Input(Input::Context([0.0, 25.0], Point::new(10.0, 10.0))));
    assert_eq!(app.context, Some(Point::new(10.0, 10.0)));
    assert_eq!(app.tab().doc.editor.selection, vec![id.clone()]);
    assert!(context_menu::view(&app).is_some());
    send(&mut app, Message::Context(Action::Lock));
    assert!(app.context.is_none(), "an entry closes it");
    assert!(app.tab().doc.editor.selection_locked());
    // On nothing: the selection goes; the canvas menu offers Unlock all.
    send(&mut app, Message::Input(Input::Context([500.0, 500.0], Point::new(10.0, 10.0))));
    assert!(app.tab().doc.editor.selection.is_empty());
    send(&mut app, Message::Context(Action::UnlockAll));
    assert_eq!(app.tab().doc.editor.selection, vec![id.clone()]);
    // A click away closes it and does nothing else.
    send(&mut app, Message::Input(Input::Context([500.0, 500.0], Point::new(10.0, 10.0))));
    send(&mut app, Message::Input(Input::Down([0.0, 25.0], NONE)));
    send(&mut app, Message::Input(Input::Up([0.0, 25.0], NONE)));
    assert!(app.context.is_none());
    assert!(app.tab().doc.editor.selection.is_empty(), "the click did not select");
    // The grid, from the menu and from Ctrl+'.
    send(&mut app, Message::Context(Action::ToggleGrid));
    assert_eq!(app.tab().doc.editor.grid_size(), Some(20.0));
}

#[test]
fn zoom_buttons_step_by_a_tenth() {
    let mut app = App::boot(Args { screenshot: Some("/dev/null".into()), chrome: true, ..Args::default() }).0;
    send(&mut app, Message::Zoom(zoom::Zoom::Reset));
    assert_eq!(app.tab().view.zoom, 1.0);
    send(&mut app, Message::Zoom(zoom::Zoom::In));
    send(&mut app, Message::Zoom(zoom::Zoom::In));
    assert_eq!(app.tab().view.zoom, 1.2);
    assert_eq!(app.tab().doc.editor.zoom, f64::from(1.2f32), "the editor's hit distances follow");
    for _ in 0..30 {
        send(&mut app, Message::Zoom(zoom::Zoom::Out));
    }
    assert_eq!(app.tab().view.zoom, 0.1, "10% at least");
    send(&mut app, Message::Zoom(zoom::Zoom::Reset));
    assert_eq!(app.tab().view.zoom, 1.0);
}
