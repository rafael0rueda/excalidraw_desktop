//! Where the app keeps its own state on disk.

use std::path::{Path, PathBuf};

/// Config lives under XDG_CONFIG_HOME so it sits alongside the rest of the
/// user's desktop configuration rather than in an app-private blob.
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("excalidraw-desktop")
}

/// Creates `dir`, and keeps it readable by its owner alone.
///
/// Everything under the config directory is the user's own work — whole
/// drawings in `session/`, and in `recent.json` the path of everything they
/// have opened — so none of it should be left at whatever the umask happens
/// to be. `recursive` so that an existing directory is not an error, and the
/// mode is reapplied either way, since the directory may predate this rule.
pub fn ensure_private_dir(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .and_then(|()| std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)))
        .map_err(|e| format!("{}: {e}", dir.display()))
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Validates an id that is about to become a file name.
///
/// Ids reach us from the renderer — a theme id, a tab id — and are joined onto
/// one of our directories, so `../` and absolute paths have to be impossible
/// rather than merely unlikely.
pub fn safe_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "invalid id {id:?} — use lower-case letters, digits and dashes"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_private_dir, safe_id};
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_private_directory_is_the_users_alone_however_it_started() {
        let dir = std::env::temp_dir().join(format!("excalidraw-private-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

        // A directory that predates the rule, left at whatever the umask made it.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure_private_dir(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700, "an existing directory is tightened, not left alone");

        // And one this call creates itself.
        let nested = dir.join("session");
        ensure_private_dir(&nested).unwrap();
        assert_eq!(mode(&nested), 0o700);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_that_would_escape_their_directory_are_rejected() {
        assert!(safe_id("kanagawa-wave").is_ok());
        assert!(safe_id("6f1a2b3c-4d5e-6f70-8192-a3b4c5d6e7f8").is_ok());

        for bad in ["", "../escape", "/etc/passwd", "Upper", "with space", "dot.dot"] {
            assert!(safe_id(bad).is_err(), "{bad:?} should be rejected");
        }
        assert!(safe_id(&"a".repeat(65)).is_err());
    }
}
