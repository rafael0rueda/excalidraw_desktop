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
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How long drawing has to pause before a snapshot is taken.
pub const QUIET: Duration = Duration::from_millis(1200);

/// What was found at start-up.
pub enum Restored {
    /// Nothing, or a clean session: these tabs, as they were.
    Tabs(Vec<Tab>, Option<String>),
    /// Unsaved work from a run that did not end normally: ask first.
    Ask(Session),
}

pub fn load(app: &App) -> Restored {
    let Some(s) = session::load(&app.allowed) else { return Restored::Tabs(Vec::new(), None) };
    if !s.clean_exit && s.tabs.iter().any(|t| t.dirty) {
        return Restored::Ask(s);
    }
    let active = s.active.clone();
    Restored::Tabs(open(app, s, true).0, active)
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
                tab.unsnapshotted = false;
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

/// Writes every tab's state; scenes only for tabs changed since their last
/// snapshot. `discarded` tabs are written as their files are (or left out,
/// if they have none): what was discarded on the way out stays discarded.
pub fn snapshot(app: &mut App, discarded: &HashSet<String>) -> Result<(), String> {
    let mut inputs = Vec::new();
    for tab in &mut app.tabs {
        let path = tab.path.as_ref().map(|p| p.to_string_lossy().into_owned());
        if discarded.contains(&tab.id) {
            let Some(p) = &tab.path else { continue };
            let Ok(text) = std::fs::read_to_string(p) else { continue };
            inputs.push(TabInput { id: tab.id.clone(), path, dirty: false, scene: Some(text) });
            continue;
        }
        let scene = tab.unsnapshotted.then(|| tab.doc.editor.scene.to_file_json());
        tab.unsnapshotted = false;
        inputs.push(TabInput { id: tab.id.clone(), path, dirty: tab.dirty(), scene });
    }
    let active = app.tabs.get(app.active).map(|t| t.id.clone());
    session::save(&app.allowed, inputs, active)
}

/// The autosave clock: a snapshot once drawing has paused.
pub fn tick(app: &mut App, now: Instant) {
    if app.last_change.is_some_and(|t| now.duration_since(t) >= QUIET) {
        app.last_change = None;
        if let Err(e) = snapshot(app, &HashSet::new()) {
            app.status = Some(format!("Autosave failed: {e}"));
        }
    }
}

/// The last snapshot, and the mark that says this run ended on purpose.
pub fn end(app: &mut App, discarded: &HashSet<String>) {
    for tab in &mut app.tabs {
        tab.unsnapshotted = true;
    }
    let _ = snapshot(app, discarded);
    let _ = session::mark_clean_exit();
}
