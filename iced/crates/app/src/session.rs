//! Autosave and recovery, through core's session store: the same snapshot
//! files, the same rules, as the Tauri app.
//!
//! Every open tab is snapshotted a second or so after drawing stops, never
//! to the drawing's own file. A run that ends normally marks the session
//! clean, and the next launch reopens it as it was; one that crashed leaves
//! it unclean, and the next launch asks before bringing unsaved work back.

use crate::doc::{self, Document};
use crate::tabs::Tab;
use crate::App;
use excalidraw_desktop_core::session::{self, Session, TabInput};
use excalidraw_desktop_core::scope::Allowed;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long drawing has to pause before a snapshot is taken.
pub const QUIET: Duration = Duration::from_millis(1200);

/// What was found at start-up.
pub enum Restored {
    /// Nothing, or a clean session: these tabs, as they were, and what
    /// could not be read.
    Tabs(Vec<Tab>, Option<String>, Vec<String>),
    /// Unsaved work from a run that did not end normally: ask first.
    Ask(Session),
}

pub fn load(app: &App) -> Restored {
    let Some(s) = session::load(&app.allowed) else { return Restored::Tabs(Vec::new(), None, Vec::new()) };
    if !s.clean_exit && s.tabs.iter().any(|t| t.dirty) {
        return Restored::Ask(s);
    }
    let active = s.active.clone();
    let (tabs, problems) = open(app, s, true);
    Restored::Tabs(tabs, active, problems)
}

/// The session's tabs as tabs: with their snapshots when `recover`, else as
/// their files are on disk (untitled ones dropped). Also returns what could
/// not be read, said once.
pub fn open(app: &App, s: Session, recover: bool) -> (Vec<Tab>, Vec<String>) {
    let mut tabs = Vec::new();
    let mut problems = Vec::new();
    for t in s.tabs {
        let path = t.path.as_ref().map(PathBuf::from);
        let parsed = if recover {
            scene::Scene::parse(&t.scene).map_err(|e| e.to_string())
        } else {
            match &path {
                Some(p) => doc::read(p),
                None => continue,
            }
        };
        match parsed {
            Ok(scene) => {
                let mut d = Document::new(scene);
                if recover && t.dirty {
                    d.editor.mark_dirty();
                }
                let view = doc::fit(d.layers.bounds(), app.size);
                let mut tab = Tab::new(t.id, path, d, view);
                // Its snapshot is the one just read.
                tab.snapshotted = Some(tab.changes);
                tabs.push(tab);
            }
            Err(e) => {
                // Moved aside rather than pruned: it may be the only copy.
                if recover {
                    match session::keep_unreadable_snapshot(t.id.clone()) {
                        Ok(kept) => problems.push(format!("A snapshot could not be read and was kept at {kept}: {e}")),
                        Err(_) => problems.push(format!("A snapshot could not be read: {e}")),
                    }
                } else {
                    problems.push(e);
                }
            }
        }
    }
    (tabs, problems)
}

/// "One drawing was" / "3 drawings were", for the recovery question.
pub fn describe(s: &Session) -> String {
    let unsaved: Vec<_> = s.tabs.iter().filter(|t| t.dirty).collect();
    match unsaved.as_slice() {
        [one] => format!(
            "{} was",
            one.path.as_deref().and_then(|p| std::path::Path::new(p).file_name()).map_or("An unsaved drawing".into(), |n| n.to_string_lossy().into_owned())
        ),
        many => format!("{} drawings were", many.len()),
    }
}

/// A snapshot taken on the UI thread, to be written off it: the scenes are
/// copies, serialized and fsynced where they cannot freeze the window
/// (review 2026-11-15, IR-7; the Tauri app kept this off the UI thread too).
pub struct Pending {
    tabs: Vec<(TabInput, Option<Box<scene::Scene>>)>,
    active: Option<String>,
    /// Each tab whose scene is in it, at the change count it was taken at.
    pub taken: Vec<(String, u64)>,
}

/// Every tab's state; scenes only for tabs changed since their last
/// snapshot. `discarded` tabs are taken as their files are (or left out, if
/// they have none): what was discarded on the way out stays discarded.
/// Nothing is marked snapshotted here: that waits for the write (IR-2).
pub fn prepare(app: &mut App, discarded: &HashSet<String>) -> Pending {
    let mut tabs = Vec::new();
    let mut taken = Vec::new();
    let canvas = app.canvas();
    for tab in &mut app.tabs {
        if let Some(canvas) = &canvas {
            tab.doc.editor.scene.set_view_background_color(canvas);
        }
        let path = tab.path.as_ref().map(|p| p.to_string_lossy().into_owned());
        if discarded.contains(&tab.id) {
            let Some(p) = &tab.path else { continue };
            let Ok(text) = std::fs::read_to_string(p) else { continue };
            tabs.push((TabInput { id: tab.id.clone(), path, dirty: false, scene: Some(text) }, None));
            continue;
        }
        let scene = tab.unsnapshotted().then(|| Box::new(tab.doc.editor.scene.clone()));
        if scene.is_some() {
            taken.push((tab.id.clone(), tab.changes));
        }
        tabs.push((TabInput { id: tab.id.clone(), path, dirty: tab.dirty(), scene: None }, scene));
    }
    let active = app.tabs.get(app.active).map(|t| t.id.clone());
    Pending { tabs, active, taken }
}

/// Writes a snapshot, unless the session has ended since it was taken:
/// `ended` is held for the whole write, so one still in flight at quit can
/// never land after the last snapshot and the clean-exit mark.
pub fn write(pending: Pending, allowed: &Allowed, ended: &Mutex<bool>) -> Result<(), String> {
    let ended = ended.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if *ended {
        return Ok(());
    }
    write_now(pending, allowed)
}

fn write_now(pending: Pending, allowed: &Allowed) -> Result<(), String> {
    let inputs = pending
        .tabs
        .into_iter()
        .map(|(mut input, scene)| {
            if let Some(scene) = scene {
                input.scene = Some(scene.to_file_json());
            }
            input
        })
        .collect();
    session::save(allowed, inputs, pending.active)
}

/// A write finished. Saved tabs are marked snapshotted at the change count
/// they were taken at, so a change made meanwhile is still to be written.
/// A failed one is tried again once drawing pauses, rather than never.
pub fn written(app: &mut App, taken: &[(String, u64)], result: Result<(), String>) {
    match result {
        Ok(()) => {
            for (id, changes) in taken {
                if let Some(tab) = app.tabs.iter_mut().find(|t| &t.id == id) {
                    tab.snapshotted = Some(*changes);
                }
            }
        }
        Err(e) => {
            app.status = Some(format!("Autosave failed: {e}"));
            app.last_change.get_or_insert_with(Instant::now);
        }
    }
}

/// The autosave clock: a snapshot once drawing has paused.
pub fn tick(app: &mut App, now: Instant) {
    if app.last_change.is_some_and(|t| now.duration_since(t) >= QUIET) {
        app.last_change = None;
        app.snapshot_due = true;
    }
}

/// The last snapshot, and the mark that says this run ended on purpose.
/// Written here and now, with no write still in flight able to follow it.
pub fn end(app: &mut App, discarded: &HashSet<String>) {
    let gate = app.session_ended.clone();
    let mut ended = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *ended = true;
    for tab in &mut app.tabs {
        tab.snapshotted = None;
    }
    let pending = prepare(app, discarded);
    let _ = write_now(pending, &app.allowed);
    let _ = session::mark_clean_exit();
}
