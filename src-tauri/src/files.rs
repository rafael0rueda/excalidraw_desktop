use base64::Engine;
use std::path::Path;

#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))
}

/// Resolves symlinks and `..`/`.` so the same drawing reached two ways (a
/// path and a symlink to it, say) compares equal. `cli_drawings` already does
/// this for the command line; this is the same normalisation for a path the
/// renderer got from the native Open dialog. Falls back to the input
/// unchanged if the path cannot be resolved (broken symlink, since removed).
#[tauri::command]
pub fn canonicalize_path(path: String) -> String {
    std::fs::canonicalize(&path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or(path)
}

#[tauri::command]
pub fn write_text_file(path: String, contents: String) -> Result<(), String> {
    write_atomic(Path::new(&path), contents.as_bytes())
}

#[tauri::command]
pub fn write_binary_file(path: String, data: String) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map_err(|e| format!("bad image payload: {e}"))?;
    write_atomic(Path::new(&path), &bytes)
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
    let tmp = target.with_extension(format!(
        "{}.tmp",
        target.extension().and_then(|e| e.to_str()).unwrap_or("out")
    ));
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    // The rename below hands the new inode the temp file's own permissions
    // (governed by umask), silently loosening a more restrictive mode set on
    // the file it is replacing — carry the existing mode over instead.
    if let Ok(meta) = std::fs::metadata(&target) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, &target).map_err(|e| format!("{}: {e}", target.display()))
}

#[cfg(test)]
mod tests {
    use super::write_atomic;
    use std::os::unix::fs::{symlink, PermissionsExt};

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
}
