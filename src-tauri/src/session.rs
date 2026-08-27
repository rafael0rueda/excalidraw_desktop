//! Autosave snapshots.
//!
//! The renderer hands us the whole scene every few seconds; we keep exactly one
//! snapshot. Its job is not versioning — it is answering "the app died, what was
//! on screen?" without ever writing to the user's own file behind their back.

use crate::files::write_atomic;
use crate::store::{config_dir, now};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What we knew about the drawing when the snapshot was taken.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SessionMeta {
    /// File the scene came from, or `None` for a drawing never saved anywhere.
    pub path: Option<String>,
    /// The snapshot holds changes the file on disk does not.
    pub dirty: bool,
    pub saved_at: u64,
    /// False for as long as the app is running, set on an orderly shutdown. A
    /// snapshot still holding `false` is the fingerprint of a crash or a kill,
    /// and is the only case where recovery has anything to offer.
    pub clean_exit: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    #[serde(flatten)]
    pub meta: SessionMeta,
    pub scene: String,
}

fn session_dir() -> PathBuf {
    config_dir().join("session")
}

/// Kept under its real extension so a failed recovery still leaves the user a
/// file they can open by hand.
fn scene_path() -> PathBuf {
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

#[tauri::command]
pub fn save_session(path: Option<String>, dirty: bool, scene: String) -> Result<(), String> {
    write_atomic(&scene_path(), scene.as_bytes())?;
    write_meta(&SessionMeta {
        path,
        dirty,
        saved_at: now(),
        clean_exit: false,
    })
}

#[tauri::command]
pub fn load_session() -> Option<Session> {
    let meta = read_meta()?;
    let scene = std::fs::read_to_string(scene_path()).ok()?;
    Some(Session { meta, scene })
}

/// Records that we are shutting down on purpose, so the next launch reopens the
/// last drawing instead of offering to recover from it.
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
    for path in [scene_path(), meta_path()] {
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

        save_session(Some("/tmp/a.excalidraw".into()), true, "{\"scene\":1}".into()).unwrap();
        let s = load_session().expect("snapshot readable");
        assert_eq!(s.meta.path.as_deref(), Some("/tmp/a.excalidraw"));
        assert!(s.meta.dirty);
        assert!(!s.meta.clean_exit, "a live app has not exited cleanly");
        assert_eq!(s.scene, "{\"scene\":1}");

        mark_clean_exit().unwrap();
        let s = load_session().expect("still readable after a clean exit");
        assert!(s.meta.clean_exit);
        assert_eq!(s.scene, "{\"scene\":1}", "clean exit must not disturb the scene");

        // A fresh snapshot reopens the recovery window.
        save_session(None, false, "{\"scene\":2}".into()).unwrap();
        let s = load_session().unwrap();
        assert!(!s.meta.clean_exit);
        assert!(s.meta.path.is_none());
        assert_eq!(s.scene, "{\"scene\":2}");

        clear_session().unwrap();
        assert!(load_session().is_none());
        clear_session().expect("clearing twice is not an error");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
