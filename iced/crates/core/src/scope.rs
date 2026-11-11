//! Which files the renderer may read and write.
//!
//! The renderer names files by path, so without this a script running in the
//! page — through a flaw in how some hostile drawing is handled, say — could
//! read or overwrite anything in the user's home. A path is allowed only once
//! it has come from somewhere the renderer does not control: a native dialog
//! run from Rust, the command line, a second launch, or a list Rust wrote
//! itself (recent files, the session).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct Allowed(Mutex<HashSet<PathBuf>>);

/// The one spelling a path is remembered and checked by: absolute, with
/// symlinks and `..` resolved. A file that does not exist yet is resolved
/// through its directory. None for a relative path, or a directory that
/// cannot be resolved.
fn normalize(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    if let Ok(real) = std::fs::canonicalize(path) {
        return Some(real);
    }
    let parent = std::fs::canonicalize(path.parent()?).ok()?;
    Some(parent.join(path.file_name()?))
}

impl Allowed {
    /// Allows `path`, returning the spelling it is allowed under.
    pub fn allow(&self, path: &Path) -> Option<PathBuf> {
        let normal = normalize(path)?;
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(normal.clone());
        Some(normal)
    }

    /// The resolved path, if `path` names a file that was allowed.
    pub fn check(&self, path: &str) -> Result<PathBuf, String> {
        let normal = normalize(Path::new(path)).ok_or_else(|| format!("{path}: not a usable path"))?;
        let allowed = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(&normal);
        if allowed {
            Ok(normal)
        } else {
            Err(format!("{path}: not a file this app was asked to open or save"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Allowed;
    use std::os::unix::fs::symlink;

    #[test]
    fn only_allowed_files_pass_however_they_are_spelled() {
        let dir = std::env::temp_dir().join(format!("excalidraw-scope-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let file = dir.join("a.excalidraw");
        let link = dir.join("link.excalidraw");
        std::fs::write(&file, "{}").unwrap();
        symlink(&file, &link).unwrap();
        let path = |p: &std::path::Path| p.to_string_lossy().into_owned();

        let allowed = Allowed::default();
        assert!(allowed.check(&path(&file)).is_err(), "nothing is allowed up front");

        allowed.allow(&link).unwrap();
        assert!(allowed.check(&path(&file)).is_ok(), "a symlink allows what it points at");
        assert!(allowed.check(&path(&dir.join("sub/../a.excalidraw"))).is_ok());
        assert!(allowed.check(&path(&dir.join("b.excalidraw"))).is_err());
        assert!(allowed.check("a.excalidraw").is_err(), "a relative path is never allowed");

        // A save target does not exist until it is written.
        let new = dir.join("new.excalidraw");
        allowed.allow(&new).unwrap();
        assert!(allowed.check(&path(&new)).is_ok());
        std::fs::write(&new, "{}").unwrap();
        assert!(allowed.check(&path(&new)).is_ok(), "and is still allowed once it does");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
