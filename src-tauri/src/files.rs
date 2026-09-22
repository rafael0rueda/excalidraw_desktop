use crate::scope::Allowed;
use base64::Engine;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::State;

/// What the renderer may write as text: a drawing, or an SVG export. Being in
/// `Allowed` already means the user chose the path in a dialog — this is the
/// second half of the same rule `write_binary_file` applies to PNG, so that a
/// flaw in the page cannot turn one kind of file into another.
const TEXT_EXTENSIONS: &[&str] = &["excalidraw", "svg"];

/// Checks the extension of a path that has already been resolved, so a symlink
/// is judged by what it points at rather than by what it is called.
fn require_extension(path: &Path, extensions: &[&str]) -> Result<(), String> {
    let found = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    if extensions.iter().any(|want| found.eq_ignore_ascii_case(want)) {
        return Ok(());
    }
    Err(format!("{}: not a {} file", path.display(), extensions.join(" or ")))
}

// File commands run on Tauri's thread pool rather than inline in the IPC
// handler, which is the GTK main thread: a large drawing read or written there
// froze the whole window for as long as it took.
#[tauri::command(async)]
pub fn read_text_file(allowed: State<'_, Allowed>, path: String) -> Result<String, String> {
    let path = allowed.check(&path)?;
    std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

#[tauri::command(async)]
pub fn write_text_file(allowed: State<'_, Allowed>, path: String, contents: String) -> Result<(), String> {
    let target = allowed.check(&path)?;
    require_extension(&target, TEXT_EXTENSIONS)?;
    write_atomic(&target, contents.as_bytes())
}

/// Header carrying the target of a raw-body write, base64 so that any file name
/// survives header encoding.
const PATH_HEADER: &str = "x-path";

fn header_path(request: &tauri::ipc::Request<'_>) -> Result<String, String> {
    let encoded = request
        .headers()
        .get(PATH_HEADER)
        .ok_or("missing path")?
        .to_str()
        .map_err(|e| e.to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("bad path header: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("bad path header: {e}"))
}

/// A PNG export. The bytes are the raw request body: base64 inside JSON meant
/// encoding, copying and decoding a multi-megabyte image on every export.
#[tauri::command(async)]
pub fn write_binary_file(allowed: State<'_, Allowed>, request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let path = header_path(&request)?;
    let target = allowed.check(&path)?;
    // Only a PNG export is binary. Without this, an allowed drawing could be
    // overwritten with image bytes.
    require_extension(&target, &["png"])?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("expected the image as raw bytes".into());
    };
    write_atomic(&target, bytes)
}

/// Unique within this process, and across processes by the pid.
fn temp_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!(
        "{}-{nanos}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Writes to a sibling temp file and renames, so an interrupted save can never
/// leave a half-written drawing where the original used to be.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    // Resolve a symlink to what it points at first: renaming a temp file onto
    // the *link's* own directory entry would destroy the symlink and replace
    // it with a plain file, leaving whatever it pointed to untouched. A path
    // that does not exist yet (an ordinary new save) has nothing to resolve,
    // so it is used as given.
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let parent = target.parent().ok_or_else(|| "invalid path".to_string())?;
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let name = target
        .file_name()
        .ok_or_else(|| "invalid path".to_string())?
        .to_string_lossy();
    // A fresh name per write, so two writes to one file cannot share a temp
    // file, and `create_new` besides: a fixed, guessable name could be planted
    // in a shared directory as a symlink, which a plain write would follow.
    let tmp = parent.join(format!(".{name}.{}.tmp", temp_suffix()));

    let written = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        // The rename hands the new inode the temp file's own permissions
        // (governed by umask), silently loosening a more restrictive mode set
        // on the file it replaces — carry the existing mode over instead.
        if let Ok(meta) = std::fs::metadata(&target) {
            file.set_permissions(meta.permissions())?;
        }
        file.write_all(bytes)?;
        // Otherwise the rename can reach the disk before the data does, and a
        // power cut in between leaves an empty file where the drawing was.
        file.sync_all()?;
        std::fs::rename(&tmp, &target)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: {e}", target.display()));
    }
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{require_extension, write_atomic, TEXT_EXTENSIONS};
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::Path;

    #[test]
    fn only_the_kinds_of_file_this_app_produces_may_be_written() {
        for good in ["/tmp/plan.excalidraw", "/tmp/plan.EXCALIDRAW", "/tmp/export.svg"] {
            assert!(require_extension(Path::new(good), TEXT_EXTENSIONS).is_ok(), "{good}");
        }
        // An allowed path is one the user picked in a dialog, so these are not
        // reachable today — the point is that they stay unreachable if some
        // other path into `Allowed` is ever added.
        for bad in ["/tmp/.bashrc", "/tmp/notes.txt", "/tmp/plan", "/tmp/plan.excalidraw.txt"] {
            assert!(require_extension(Path::new(bad), TEXT_EXTENSIONS).is_err(), "{bad}");
        }
        assert!(require_extension(Path::new("/tmp/a.png"), &["png"]).is_ok());
        assert!(require_extension(Path::new("/tmp/a.excalidraw"), &["png"]).is_err());
    }

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("excalidraw-write-atomic-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writing_through_a_symlink_leaves_the_link_in_place() {
        let dir = scratch_dir("symlink");
        let real = dir.join("real.excalidraw");
        let link = dir.join("link.excalidraw");
        std::fs::write(&real, "old").unwrap();
        symlink(&real, &link).unwrap();

        write_atomic(&link, b"new").unwrap();

        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "new");
    }

    #[test]
    fn an_existing_files_permissions_survive_a_rewrite() {
        let dir = scratch_dir("perms");
        let path = dir.join("secret.excalidraw");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        write_atomic(&path, b"new").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn no_temp_file_is_left_behind() {
        let dir = scratch_dir("tidy");
        let path = dir.join("plan.excalidraw");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();

        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("plan.excalidraw")]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
    }
}
