//! Autosave snapshots.
//!
//! The renderer hands us every open tab a few seconds after it stops changing;
//! we keep exactly one snapshot per tab. Their job is not versioning — it is
//! answering "the app died, what was on screen?" without ever writing to the
//! user's own files behind their back.

use crate::files::write_atomic;
use crate::store::{config_dir, now, safe_id};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Id given to the one drawing recovered from a session written before tabs
/// existed. Chosen rather than generated because it has to be a valid file name
/// and Rust has no uuid dependency here.
const LEGACY_TAB_ID: &str = "restored";

/// What we knew about one open drawing when the snapshot was taken.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TabMeta {
    /// Also names the tab's snapshot file, so it is validated as one.
    pub id: String,
    /// File the scene came from, or `None` for a drawing never saved anywhere.
    pub path: Option<String>,
    /// The snapshot holds changes the file on disk does not.
    pub dirty: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SessionMeta {
    #[serde(default)]
    pub tabs: Vec<TabMeta>,
    /// Id of the tab that was on screen.
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub saved_at: u64,
    /// False for as long as the app is running, set on an orderly shutdown. A
    /// snapshot still holding `false` is the fingerprint of a crash or a kill,
    /// and is the only case where recovery has anything to offer.
    #[serde(default)]
    pub clean_exit: bool,

    /// Written by versions from before tabs existed, when a session was one
    /// drawing in `scene.excalidraw`. Read so that upgrading does not throw the
    /// session away; never written back.
    #[serde(default, skip_serializing)]
    pub path: Option<String>,
    #[serde(default, skip_serializing)]
    pub dirty: bool,
}

/// One restored tab, scene included. Separate from `TabMeta` because the
/// renderer wants the contents and the file deliberately does not hold them.
#[derive(Serialize, Clone, Debug)]
pub struct SessionTab {
    pub id: String,
    pub path: Option<String>,
    pub dirty: bool,
    pub scene: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Session {
    pub tabs: Vec<SessionTab>,
    pub active: Option<String>,
    pub clean_exit: bool,
}

/// A tab on its way to disk.
#[derive(Deserialize, Clone, Debug)]
pub struct TabInput {
    pub id: String,
    pub path: Option<String>,
    pub dirty: bool,
    /// Left out when this tab's scene has not changed since the last snapshot,
    /// which is every tab but the one being drawn in. Its file is then kept as
    /// it is rather than rewritten with identical bytes.
    #[serde(default)]
    pub scene: Option<String>,
}

fn session_dir() -> PathBuf {
    config_dir().join("session")
}

/// Kept under its real extension so a failed recovery still leaves the user a
/// file they can open by hand.
fn scene_path(id: &str) -> PathBuf {
    session_dir().join(format!("{id}.excalidraw"))
}

/// Where a pre-tabs version kept its single snapshot.
fn legacy_scene_path() -> PathBuf {
    session_dir().join("scene.excalidraw")
}

fn meta_path() -> PathBuf {
    session_dir().join("meta.json")
}

fn read_meta() -> Option<SessionMeta> {
    let text = std::fs::read_to_string(meta_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_meta(meta: &SessionMeta) -> Result<(), String> {
    let json = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    write_atomic(&meta_path(), json.as_bytes())
}

/// Every snapshot in the session directory, whatever it belongs to.
fn snapshot_files() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(session_dir()) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("excalidraw"))
        .collect()
}

/// Drops snapshots of tabs that are no longer open — including the single
/// `scene.excalidraw` of a pre-tabs session, once its drawing has been adopted
/// into a tab of its own.
fn prune(keep: &[TabInput]) {
    for path in snapshot_files() {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        if !keep.iter().any(|t| t.id == stem) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[tauri::command]
pub fn save_session(tabs: Vec<TabInput>, active: Option<String>) -> Result<(), String> {
    // Checked up front: a bad id must not leave half a session behind.
    for tab in &tabs {
        safe_id(&tab.id)?;
    }
    for tab in &tabs {
        if let Some(scene) = &tab.scene {
            write_atomic(&scene_path(&tab.id), scene.as_bytes())?;
        }
    }
    prune(&tabs);
    write_meta(&SessionMeta {
        tabs: tabs
            .iter()
            .map(|t| TabMeta {
                id: t.id.clone(),
                path: t.path.clone(),
                dirty: t.dirty,
            })
            .collect(),
        active,
        saved_at: now(),
        clean_exit: false,
        ..Default::default()
    })
}

#[tauri::command]
pub fn load_session() -> Option<Session> {
    let meta = read_meta()?;

    let mut tabs: Vec<SessionTab> = meta
        .tabs
        .iter()
        // A tab whose snapshot has gone missing is dropped rather than restored
        // empty, which would look like the drawing was lost rather than absent.
        .filter(|t| safe_id(&t.id).is_ok())
        .filter_map(|t| {
            Some(SessionTab {
                id: t.id.clone(),
                path: t.path.clone(),
                dirty: t.dirty,
                scene: std::fs::read_to_string(scene_path(&t.id)).ok()?,
            })
        })
        .collect();

    if meta.tabs.is_empty() {
        if let Ok(scene) = std::fs::read_to_string(legacy_scene_path()) {
            tabs.push(SessionTab {
                id: LEGACY_TAB_ID.into(),
                path: meta.path.clone(),
                dirty: meta.dirty,
                scene,
            });
        }
    }

    if tabs.is_empty() {
        return None;
    }
    let active = meta
        .active
        .filter(|id| tabs.iter().any(|t| &t.id == id))
        .or_else(|| tabs.first().map(|t| t.id.clone()));
    Some(Session {
        tabs,
        active,
        clean_exit: meta.clean_exit,
    })
}

/// Records that we are shutting down on purpose, so the next launch reopens the
/// last drawings instead of offering to recover from them.
#[tauri::command]
pub fn mark_clean_exit() -> Result<(), String> {
    let Some(mut meta) = read_meta() else {
        return Ok(());
    };
    meta.clean_exit = true;
    write_meta(&meta)
}

#[tauri::command]
pub fn clear_session() -> Result<(), String> {
    let mut paths = snapshot_files();
    paths.push(meta_path());
    for path in paths {
        if let Err(e) = std::fs::remove_file(&path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(format!("{}: {e}", path.display()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: &str, path: Option<&str>, scene: Option<&str>) -> TabInput {
        TabInput {
            id: id.into(),
            path: path.map(Into::into),
            dirty: true,
            scene: scene.map(Into::into),
        }
    }

    /// One test, not several: `config_dir()` reads a process-wide environment
    /// variable, so parallel tests would fight over it.
    #[test]
    fn snapshot_lifecycle() {
        let dir = std::env::temp_dir().join(format!("excalidraw-session-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("XDG_CONFIG_HOME", &dir);

        assert!(load_session().is_none(), "nothing saved yet");
        // Marking a clean exit with no snapshot is a no-op, not an error.
        mark_clean_exit().unwrap();

        // --- a session written before tabs existed is adopted as one tab
        write_atomic(&legacy_scene_path(), b"{\"scene\":0}").unwrap();
        // Written as text: the legacy fields are read-only, so `write_meta`
        // would not produce a file of the shape we are testing against.
        write_atomic(
            &meta_path(),
            br#"{"path":"/tmp/old.excalidraw","dirty":true,"saved_at":1,"clean_exit":false}"#,
        )
        .unwrap();
        let s = load_session().expect("a pre-tabs session still loads");
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.tabs[0].id, LEGACY_TAB_ID);
        assert_eq!(s.tabs[0].path.as_deref(), Some("/tmp/old.excalidraw"));
        assert_eq!(s.tabs[0].scene, "{\"scene\":0}");
        assert_eq!(s.active.as_deref(), Some(LEGACY_TAB_ID));

        // --- two tabs
        save_session(
            vec![
                tab("aaa", Some("/tmp/a.excalidraw"), Some("{\"scene\":1}")),
                tab("bbb", None, Some("{\"scene\":2}")),
            ],
            Some("bbb".into()),
        )
        .unwrap();
        assert!(!legacy_scene_path().exists(), "the migrated snapshot is pruned");

        let s = load_session().expect("snapshot readable");
        assert_eq!(s.tabs.len(), 2);
        assert_eq!(s.tabs[0].path.as_deref(), Some("/tmp/a.excalidraw"));
        assert_eq!(s.tabs[1].scene, "{\"scene\":2}");
        assert!(s.tabs[1].dirty);
        assert!(!s.clean_exit, "a live app has not exited cleanly");
        assert_eq!(s.active.as_deref(), Some("bbb"));

        // --- an omitted scene keeps the file it already has
        save_session(
            vec![tab("aaa", Some("/tmp/a.excalidraw"), None), tab("bbb", None, None)],
            Some("aaa".into()),
        )
        .unwrap();
        let s = load_session().unwrap();
        assert_eq!(s.tabs[0].scene, "{\"scene\":1}");
        assert_eq!(s.tabs[1].scene, "{\"scene\":2}");

        mark_clean_exit().unwrap();
        let s = load_session().expect("still readable after a clean exit");
        assert!(s.clean_exit);
        assert_eq!(s.tabs.len(), 2, "a clean exit must not disturb the scenes");

        // --- closing a tab takes its snapshot with it
        save_session(vec![tab("bbb", None, None)], Some("bbb".into())).unwrap();
        assert!(!scene_path("aaa").exists());
        let s = load_session().unwrap();
        assert_eq!(s.tabs.len(), 1);
        assert!(!s.clean_exit, "a fresh snapshot reopens the recovery window");

        // --- an id that would escape the directory is refused outright
        assert!(save_session(vec![tab("../escape", None, Some("x"))], None).is_err());
        assert!(!dir.join("escape.excalidraw").exists());

        clear_session().unwrap();
        assert!(load_session().is_none());
        clear_session().expect("clearing twice is not an error");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
