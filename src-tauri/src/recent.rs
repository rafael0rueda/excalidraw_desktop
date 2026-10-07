use crate::files::write_atomic;
use crate::scope::Allowed;
use crate::store::{config_dir, now};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

const MAX_RECENT: usize = 10;

/// Held across each read-change-write of the list. Commands run on a thread
/// pool, and two pushes interleaving would drop one of the entries.
static RECENT: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RecentEntry {
    pub path: String,
    pub name: String,
    pub opened_at: u64,
}

fn recent_path() -> PathBuf {
    config_dir().join("recent.json")
}

// Each of these takes the list's file rather than finding it, so the tests
// can use a scratch one instead of `config_dir()`'s process-wide variable.

fn load(file: &Path) -> Vec<RecentEntry> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn store(file: &Path, entries: &[RecentEntry]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    // Atomic like every other file here: a crash mid-write would leave a
    // file that no longer parses, and so an empty list.
    write_atomic(file, json.as_bytes())
}

fn list(file: &Path, allowed: &Allowed) -> Vec<RecentEntry> {
    // Drop entries whose files have since been deleted or moved.
    let entries: Vec<_> = load(file)
        .into_iter()
        .filter(|e| Path::new(&e.path).exists())
        .collect();
    // Every entry got onto the list through `push`, which takes only allowed
    // paths, so choosing one from the menu may open it.
    for entry in &entries {
        allowed.allow(Path::new(&entry.path));
    }
    entries
}

fn push(file: &Path, allowed: &Allowed, path: String) -> Result<Vec<RecentEntry>, String> {
    let _recent = RECENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // The list is what `list` allows, so the renderer must not be able to put
    // just any path on it.
    if allowed.check(&path).is_err() {
        return Ok(load(file));
    }
    let name = Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&path)
        .to_string();

    let mut entries = load(file);
    entries.retain(|e| e.path != path);
    entries.insert(
        0,
        RecentEntry {
            path,
            name,
            opened_at: now(),
        },
    );
    entries.truncate(MAX_RECENT);
    // Reported rather than swallowed: handing back the new list as though it
    // had been kept told the caller something that was not true.
    store(file, &entries)?;
    Ok(entries)
}

#[tauri::command(async)]
pub fn list_recent(allowed: State<'_, Allowed>) -> Vec<RecentEntry> {
    list(&recent_path(), &allowed)
}

#[tauri::command(async)]
pub fn push_recent(allowed: State<'_, Allowed>, path: String) -> Result<Vec<RecentEntry>, String> {
    push(&recent_path(), &allowed, path)
}

#[tauri::command(async)]
pub fn clear_recent() -> Result<Vec<RecentEntry>, String> {
    let _recent = RECENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    store(&recent_path(), &[])?;
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_holds_only_what_was_allowed_newest_first_and_no_more_than_ten() {
        let dir = std::env::temp_dir().join(format!("excalidraw-recent-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("recent.json");
        let allowed = Allowed::default();
        let drawing = |n: usize| {
            let path = dir.join(format!("d{n}.excalidraw"));
            std::fs::write(&path, "{}").unwrap();
            allowed.allow(&path).unwrap().to_string_lossy().into_owned()
        };

        assert!(list(&file, &allowed).is_empty(), "no file yet is an empty list");

        let a = drawing(0);
        let b = drawing(1);
        push(&file, &allowed, a.clone()).unwrap();
        let entries = push(&file, &allowed, b.clone()).unwrap();
        assert_eq!(entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(), [&b, &a]);
        assert_eq!(entries[0].name, "d1.excalidraw");

        // Opening one again moves it to the front instead of listing it twice.
        let entries = push(&file, &allowed, a.clone()).unwrap();
        assert_eq!(entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(), [&a, &b]);

        // A path nothing allowed is not added: the list is what a later launch
        // allows, so it would otherwise be a way to read any file at all.
        let entries = push(&file, &allowed, "/etc/passwd".into()).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(load(&file).iter().all(|e| e.path != "/etc/passwd"));

        for n in 2..MAX_RECENT + 3 {
            push(&file, &allowed, drawing(n)).unwrap();
        }
        let entries = load(&file);
        assert_eq!(entries.len(), MAX_RECENT);
        assert!(entries.iter().all(|e| e.path != b), "the oldest fall off the end");

        // A later launch starts with nothing allowed. Listing allows what is
        // still there, and drops what has been deleted since.
        let newest = entries[0].path.clone();
        std::fs::remove_file(&entries[1].path).unwrap();
        let later = Allowed::default();
        assert!(later.check(&newest).is_err());
        let listed = list(&file, &later);
        assert_eq!(listed.len(), MAX_RECENT - 1);
        assert!(later.check(&newest).is_ok(), "an entry on the list may be opened");

        // A list that cannot be written is an error, not a quiet success.
        assert!(push(&dir.join("recent.json").join("under-a-file"), &allowed, a).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
