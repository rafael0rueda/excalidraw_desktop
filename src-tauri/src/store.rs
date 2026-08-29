//! Where the app keeps its own state on disk.

use std::path::PathBuf;

/// Config lives under XDG_CONFIG_HOME so it sits alongside the rest of the
/// user's desktop configuration rather than in an app-private blob.
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("excalidraw-desktop")
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
    use super::safe_id;

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
