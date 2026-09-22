//! Autosave snapshots.
//!
//! The renderer hands us every open tab a few seconds after it stops changing;
//! we keep exactly one snapshot per tab. Their job is not versioning — it is
//! answering "the app died, what was on screen?" without ever writing to the
//! user's own files behind their back.

use crate::files::write_atomic;
use crate::scope::Allowed;
use crate::store::{config_dir, ensure_private_dir, now, safe_id};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::State;

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

/// Held by everything that touches the session directory. Commands run on a
/// thread pool, so a snapshot, `mark_clean_exit` and a prune could otherwise
/// interleave.
static SESSION: Mutex<()> = Mutex::new(());

fn session_lock() -> MutexGuard<'static, ()> {
    SESSION.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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

#[tauri::command(async)]
pub fn save_session(
    allowed: State<'_, Allowed>,
    tabs: Vec<TabInput>,
    active: Option<String>,
) -> Result<(), String> {
    save(&allowed, tabs, active)
}

fn save(allowed: &Allowed, tabs: Vec<TabInput>, active: Option<String>) -> Result<(), String> {
    let _session = session_lock();
    // Checked up front: a bad id must not leave half a session behind.
    for tab in &tabs {
        safe_id(&tab.id)?;
    }
    ensure_private_dir(&session_dir())?;
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
                // `load` allows every path it reads back, so only a path that
                // is already allowed may be written here.
                path: t.path.clone().filter(|p| allowed.check(p).is_ok()),
                dirty: t.dirty,
            })
            .collect(),
        active,
        saved_at: now(),
        clean_exit: false,
        ..Default::default()
    })
}

#[tauri::command(async)]
pub fn load_session(allowed: State<'_, Allowed>) -> Option<Session> {
    load(&allowed)
}

fn load(allowed: &Allowed) -> Option<Session> {
    let _session = session_lock();
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
    // These drawings were open last time, so reopening them may read them.
    for path in tabs.iter().filter_map(|t| t.path.as_deref()) {
        allowed.allow(Path::new(path));
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
#[tauri::command(async)]
pub fn mark_clean_exit() -> Result<(), String> {
    let _session = session_lock();
    let Some(mut meta) = read_meta() else {
        return Ok(());
    };
    meta.clean_exit = true;
    write_meta(&meta)
}

/// How many unreadable snapshots are kept. The app never reads them again —
/// they exist so the user can try the file in another tool — so without a
/// bound the directory grows for the life of the install.
const MAX_UNREADABLE: usize = 10;

/// Drops all but the newest `MAX_UNREADABLE`.
fn prune_unreadable(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|path| path.is_file())
        .collect();
    let Some(excess) = files.len().checked_sub(MAX_UNREADABLE).filter(|n| *n > 0) else {
        return;
    };
    // Oldest first. By modification time rather than by name: the name carries
    // a timestamp, but it carries the tab id ahead of it, so sorting by name
    // would order the pile by tab instead of by when.
    files.sort_by_key(|path| path.metadata().and_then(|meta| meta.modified()).ok());
    for path in files.iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

/// Moves a snapshot the renderer could not parse out of `prune`'s way, so that
/// closing its tab does not delete what may be the only copy. It keeps its
/// extension, for trying it in another tool. Returns where it went.
#[tauri::command(async)]
pub fn keep_unreadable_snapshot(id: String) -> Result<String, String> {
    safe_id(&id)?;
    let _session = session_lock();
    let from = scene_path(&id);
    let dir = session_dir().join("unreadable");
    ensure_private_dir(&dir)?;
    // `now()` counts seconds, and one tab failing twice inside the same one
    // would otherwise rename the second copy over the first — precisely the
    // loss this function exists to prevent.
    let stamp = now();
    let mut to = dir.join(format!("{id}-{stamp}.excalidraw"));
    let mut n = 2;
    while to.exists() {
        to = dir.join(format!("{id}-{stamp}-{n}.excalidraw"));
        n += 1;
    }
    std::fs::rename(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
    prune_unreadable(&dir);
    Ok(to.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

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
        let allowed = Allowed::default();

        assert!(load(&allowed).is_none(), "nothing saved yet");
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
        let s = load(&allowed).expect("a pre-tabs session still loads");
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.tabs[0].id, LEGACY_TAB_ID);
        assert_eq!(s.tabs[0].path.as_deref(), Some("/tmp/old.excalidraw"));
        assert_eq!(s.tabs[0].scene, "{\"scene\":0}");
        assert_eq!(s.active.as_deref(), Some(LEGACY_TAB_ID));
        assert!(allowed.check("/tmp/old.excalidraw").is_ok(), "a reloaded tab may be reopened");

        // --- two tabs
        allowed.allow(Path::new("/tmp/a.excalidraw")).unwrap();
        save(
            &allowed,
            vec![
                tab("aaa", Some("/tmp/a.excalidraw"), Some("{\"scene\":1}")),
                tab("bbb", None, Some("{\"scene\":2}")),
            ],
            Some("bbb".into()),
        )
        .unwrap();
        assert!(!legacy_scene_path().exists(), "the migrated snapshot is pruned");
        let mode = std::fs::metadata(session_dir()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "snapshots are private to the user");

        let s = load(&allowed).expect("snapshot readable");
        assert_eq!(s.tabs.len(), 2);
        assert_eq!(s.tabs[0].path.as_deref(), Some("/tmp/a.excalidraw"));
        assert_eq!(s.tabs[1].scene, "{\"scene\":2}");
        assert!(s.tabs[1].dirty);
        assert!(!s.clean_exit, "a live app has not exited cleanly");
        assert_eq!(s.active.as_deref(), Some("bbb"));

        // --- an omitted scene keeps the file it already has
        save(
            &allowed,
            vec![tab("aaa", Some("/tmp/a.excalidraw"), None), tab("bbb", None, None)],
            Some("aaa".into()),
        )
        .unwrap();
        let s = load(&allowed).unwrap();
        assert_eq!(s.tabs[0].scene, "{\"scene\":1}");
        assert_eq!(s.tabs[1].scene, "{\"scene\":2}");

        mark_clean_exit().unwrap();
        let s = load(&allowed).expect("still readable after a clean exit");
        assert!(s.clean_exit);
        assert_eq!(s.tabs.len(), 2, "a clean exit must not disturb the scenes");

        // --- closing a tab takes its snapshot with it
        save(&allowed, vec![tab("bbb", None, None)], Some("bbb".into())).unwrap();
        assert!(!scene_path("aaa").exists());
        let s = load(&allowed).unwrap();
        assert_eq!(s.tabs.len(), 1);
        assert!(!s.clean_exit, "a fresh snapshot reopens the recovery window");

        // --- an id that would escape the directory is refused outright
        assert!(save(&allowed, vec![tab("../escape", None, Some("x"))], None).is_err());
        assert!(!dir.join("escape.excalidraw").exists());

        // --- a path nothing allowed is not recorded, so a restart cannot allow it
        save(&allowed, vec![tab("ddd", Some("/etc/passwd"), Some("{}"))], None).unwrap();
        assert_eq!(load(&Allowed::default()).unwrap().tabs[0].path, None);

        // --- an unreadable snapshot is moved aside, where pruning cannot reach it
        save(&allowed, vec![tab("bbb", None, Some("not a drawing"))], Some("bbb".into())).unwrap();
        let kept = PathBuf::from(keep_unreadable_snapshot("bbb".into()).unwrap());
        assert!(!scene_path("bbb").exists());
        save(&allowed, vec![tab("ccc", None, Some("{}"))], Some("ccc".into())).unwrap();
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "not a drawing");
        assert!(keep_unreadable_snapshot("../escape".into()).is_err());

        // --- and the pile of them is bounded
        let unreadable = session_dir().join("unreadable");
        let mode = std::fs::metadata(&unreadable).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "they are whole drawings too");
        for n in 0..MAX_UNREADABLE + 5 {
            write_atomic(&unreadable.join(format!("filler-{n}.excalidraw")), b"x").unwrap();
        }
        prune_unreadable(&unreadable);
        let left = std::fs::read_dir(&unreadable).unwrap().flatten().count();
        // The count alone: these are written in the same instant, so which of
        // them survives a tie in modification time is not worth asserting on.
        assert_eq!(left, MAX_UNREADABLE);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
