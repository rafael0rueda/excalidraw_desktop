//! Reading and writing files safely: atomic replacement, size limits, and
//! refusing anything that is not a regular file.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// What the renderer may write as text: a drawing, or an SVG export. Being in
/// `Allowed` already means the user chose the path in a dialog — this is the
/// second half of the same rule `write_binary_file` applies to PNG, so that a
/// flaw in the page cannot turn one kind of file into another.
pub const TEXT_EXTENSIONS: &[&str] = &["excalidraw", "svg"];

/// Checks the extension of a path that has already been resolved, so a symlink
/// is judged by what it points at rather than by what it is called.
pub fn require_extension(path: &Path, extensions: &[&str]) -> Result<(), String> {
    let found = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    if extensions.iter().any(|want| found.eq_ignore_ascii_case(want)) {
        return Ok(());
    }
    let wanted: Vec<String> = extensions.iter().map(|ext| format!(".{ext}")).collect();
    Err(format!("{}: the name has to end in {}", path.display(), wanted.join(" or ")))
}

/// A drawing larger than this is not a drawing. Generous — a scene with images
/// embedded in it runs to tens of megabytes — but bounded, so that a mistaken
/// pick cannot pull an arbitrary file into the webview's memory.
pub const MAX_TEXT_BYTES: u64 = 256 * 1024 * 1024;

/// Whether `path` is something this app can sensibly read whole.
///
/// The Open dialog will hand back whatever the user typed into it, and
/// `read_to_string` on a FIFO or a character device blocks the thread it runs
/// on for good, with nothing on screen to say why. `cli_drawings` already
/// refuses those with `is_file`; this is the same check on the other way in.
pub fn readable(path: &Path, max: u64) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{}: not a regular file", path.display()));
    }
    if meta.len() > max {
        return Err(format!(
            "{}: too large to open ({} bytes, limit {max})",
            path.display(),
            meta.len()
        ));
    }
    Ok(())
}

/// What every temp file `write_atomic` makes starts and ends with, which is
/// how `sweep_temp_files` knows one when it finds it.
const TEMP_PREFIX: &str = ".excalidraw-desktop-";
const TEMP_SUFFIX: &str = ".tmp";

/// A name unique within this process, and across processes by the pid.
///
/// It no longer carries the target's own name. That was only decoration, and
/// it made a drawing whose name was within forty-odd bytes of the 255 the
/// filesystem allows impossible to save.
fn temp_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!(
        "{TEMP_PREFIX}{}-{nanos}-{}{TEMP_SUFFIX}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Removes temp files a previous run left in `dir` by dying mid-write.
///
/// For the app's own directories only, and only at startup, before anything
/// here is writing: autosave rewrites whole drawings every second or two, so
/// a kill at the wrong moment is not rare, and each one left a dead copy of a
/// drawing in `session/` for good.
pub fn sweep_temp_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(TEMP_PREFIX) && name.ends_with(TEMP_SUFFIX) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// What `write_atomic` is about to replace, if `path` names something that
/// can be replaced.
fn resolve_target(path: &Path) -> Result<std::path::PathBuf, String> {
    // Resolve a symlink to what it points at first: renaming a temp file onto
    // the *link's* own directory entry would destroy the symlink and replace
    // it with a plain file, leaving whatever it pointed to untouched.
    if let Ok(real) = std::fs::canonicalize(path) {
        // Before the probe below: opening a FIFO for writing blocks until
        // something reads it, so the save would never return (review
        // 2026-11-04, SEC-4). Nothing but a regular file is a drawing to
        // replace.
        if std::fs::metadata(&real).is_ok_and(|meta| !meta.is_file()) {
            return Err(format!(
                "{}: not a regular file. Save under another name.",
                real.display()
            ));
        }
        // Renaming over a file needs only its directory to be writable, so a
        // file the user has made read-only was replaced without a word. Every
        // other editor refuses; so does this one.
        let writable = std::fs::OpenOptions::new().write(true).open(&real);
        if matches!(&writable, Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied) {
            return Err(format!(
                "{}: the file is read-only. Change its permissions, or save under another name.",
                real.display()
            ));
        }
        return Ok(real);
    }
    // Nothing there to resolve: an ordinary new save, used as given — unless
    // it is a link to a file that does not exist, which the rename would
    // replace with a plain file just the same.
    if path.symlink_metadata().is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(format!(
            "{}: a link to a file that does not exist. Save under another name.",
            path.display()
        ));
    }
    Ok(path.to_path_buf())
}

/// Writes to a sibling temp file and renames, so an interrupted save can never
/// leave a half-written drawing where the original used to be.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let target = resolve_target(path)?;
    let parent = target.parent().ok_or_else(|| "invalid path".to_string())?;
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    if target.file_name().is_none() {
        return Err("invalid path".to_string());
    }
    // A fresh name per write, so two writes to one file cannot share a temp
    // file, and `create_new` besides: a fixed, guessable name could be planted
    // in a shared directory as a symlink, which a plain write would follow.
    let tmp = parent.join(temp_name());

    let written = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        // The rename hands the new inode the temp file's own permissions
        // (governed by umask), silently loosening a more restrictive mode set
        // on the file it replaces — carry the existing mode over instead.
        if let Ok(meta) = std::fs::metadata(&target) {
            use std::os::unix::fs::PermissionsExt;
            let wanted = meta.permissions();
            // Some filesystems refuse a chmod outright — network and FUSE
            // mounts, mostly — and every save after the first then failed.
            // That is only worth failing over when the new file would be
            // open to someone the old one was not.
            if let Err(e) = file.set_permissions(wanted.clone()) {
                let got = file.metadata()?.permissions().mode();
                if got & !wanted.mode() & 0o777 != 0 {
                    return Err(e);
                }
            }
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
    use super::{
        readable, require_extension, sweep_temp_files, temp_name, write_atomic, MAX_TEXT_BYTES,
        TEXT_EXTENSIONS,
    };
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
    fn saving_over_a_fifo_is_refused_rather_than_blocking() {
        let dir = scratch_dir("fifo");
        let fifo = dir.join("pipe.excalidraw");
        let made = std::process::Command::new("mkfifo").arg(&fifo).status().unwrap();
        assert!(made.success());
        // Without the check this blocks forever in `open(O_WRONLY)`.
        let err = write_atomic(&fifo, b"{}").unwrap_err();
        assert!(err.contains("not a regular file"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_a_regular_file_of_a_sane_size_is_read() {
        let dir = scratch_dir("readable");
        let file = dir.join("plan.excalidraw");
        std::fs::write(&file, "{}").unwrap();

        assert!(readable(&file, MAX_TEXT_BYTES).is_ok());
        assert!(readable(&dir, MAX_TEXT_BYTES).is_err(), "a directory is not a drawing");
        assert!(readable(&dir.join("gone.excalidraw"), MAX_TEXT_BYTES).is_err());
        // The limit itself rather than a 256 MiB fixture: what is under test is
        // that the size is looked at at all.
        assert!(readable(&file, 1).is_err(), "a file over the limit is refused");

        let _ = std::fs::remove_dir_all(&dir);
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
    fn a_read_only_file_is_not_replaced() {
        let dir = scratch_dir("readonly");
        let path = dir.join("kept.excalidraw");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();

        // Root can open anything for writing, so there is nothing to refuse.
        if std::fs::OpenOptions::new().write(true).open(&path).is_ok() {
            return;
        }
        let err = write_atomic(&path, b"new").unwrap_err();
        assert!(err.contains("read-only"), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "and no temp file is left");
    }

    #[test]
    fn a_link_to_nothing_is_not_turned_into_a_file() {
        let dir = scratch_dir("dangling");
        let link = dir.join("link.excalidraw");
        symlink(dir.join("gone.excalidraw"), &link).unwrap();

        assert!(write_atomic(&link, b"new").is_err());
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(!dir.join("gone.excalidraw").exists());
    }

    #[test]
    fn temp_files_from_a_dead_run_are_swept_and_nothing_else_is() {
        let dir = scratch_dir("sweep");
        let orphan = dir.join(temp_name());
        std::fs::write(&orphan, "half a drawing").unwrap();
        for kept in ["aaa.excalidraw", "meta.json", ".hidden", "notes.tmp"] {
            std::fs::write(dir.join(kept), "x").unwrap();
        }

        sweep_temp_files(&dir);

        assert!(!orphan.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 4);
        // A name at the filesystem's limit still has room for its temp file.
        assert!(temp_name().len() < 64);
        write_atomic(&dir.join(format!("{}.excalidraw", "n".repeat(240))), b"x").unwrap();
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
