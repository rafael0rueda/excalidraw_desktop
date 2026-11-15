//! Where the app keeps its own state on disk.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The directory's name under the config base. Set once, before anything
/// reads or writes, by an app that must not share the default: the Iced
/// build, until it replaces the Tauri one, keeps out of the real session.
static APP_DIR: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// Names this app's config directory. Only the first call counts; returns
/// whether it was that one.
pub fn set_app_dir(name: &'static str) -> bool {
    APP_DIR.set(name).is_ok()
}

/// Config lives under XDG_CONFIG_HOME so it sits alongside the rest of the
/// user's desktop configuration rather than in an app-private blob.
pub fn config_dir() -> PathBuf {
    config_root(std::env::var_os("XDG_CONFIG_HOME"), std::env::var_os("HOME"))
        .join(APP_DIR.get().copied().unwrap_or("excalidraw-desktop"))
}

/// The config base, always absolute: with `HOME` unset, empty or relative
/// as well, the password database is asked, as glib's `home_dir` did before
/// this moved out of the Tauri crate. `std::env::home_dir` would not do:
/// it returns `HOME` whenever it is set, so the relative `HOME` just
/// rejected came straight back and config was written under the working
/// directory again (BE-4).
fn config_root(xdg: Option<OsString>, home: Option<OsString>) -> PathBuf {
    config_base(xdg, home).unwrap_or_else(|| passwd_home().unwrap_or_else(|| PathBuf::from("/")).join(".config"))
}

/// This user's home directory from the password database, if it has an
/// absolute one.
fn passwd_home() -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let mut buf = vec![0 as libc::c_char; 4096];
    loop {
        // SAFETY: `pwd` and `buf` outlive the call, and `buf.len()` is the
        // buffer's real size; `pw_dir` points into `buf`, read before it goes.
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let err = unsafe { libc::getpwuid_r(libc::getuid(), &mut pwd, buf.as_mut_ptr(), buf.len(), &mut found) };
        if err == libc::ERANGE && buf.len() < 1 << 20 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        if err != 0 || found.is_null() || pwd.pw_dir.is_null() {
            return None;
        }
        let dir = unsafe { std::ffi::CStr::from_ptr(pwd.pw_dir) };
        let dir = PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes()));
        return dir.is_absolute().then_some(dir);
    }
}

/// The XDG base directory for config, as the spec defines it.
///
/// An empty or relative `XDG_CONFIG_HOME` is invalid, and the spec says to
/// ignore it. Used as-is, it resolved against the working directory, so a
/// launch from a terminal wrote `session/` (whole drawings) and
/// `recent.json` into whatever project the shell was in (review 2026-11-04,
/// BE-4). The same holds for `HOME`.
fn config_base(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let absolute = |value: Option<OsString>| value.map(PathBuf::from).filter(|p| p.is_absolute());
    absolute(xdg).or_else(|| absolute(home).map(|home| home.join(".config")))
}

/// Held for as long as this process runs, so that a second copy of the app
/// can tell it is not alone.
///
/// The single-instance plugin is the first line, but it needs the session
/// bus. When the bus can't be reached (a stale `DBUS_SESSION_BUS_ADDRESS`
/// after `su`, over ssh), it quietly lets a second app start. That app's
/// startup sweep deletes the first one's in-flight temp files, and each
/// app's autosave prunes the other's snapshots (review 2026-11-04, BE-5).
/// The kernel releases a `flock` when its holder dies, so a crash never
/// leaves the lock behind.
///
/// `Ok(None)` when locking is not possible here (a read-only config
/// directory, a filesystem without `flock`): starting anyway is what the
/// app did before this lock existed. `Err` only when another process
/// holds the lock.
pub fn instance_lock() -> Result<Option<std::fs::File>, String> {
    instance_lock_at(&config_dir())
}

fn instance_lock_at(dir: &Path) -> Result<Option<std::fs::File>, String> {
    use std::os::fd::AsRawFd;
    if ensure_private_dir(dir).is_err() {
        return Ok(None);
    }
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("instance.lock"))
    else {
        return Ok(None);
    };
    // SAFETY: `flock` on a descriptor this function owns and keeps open; it
    // touches no memory.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(Some(file));
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::EWOULDBLOCK) => Err(
            "Excalidraw Desktop is already running, and this copy could not hand its files over to it.\n\n\
             Two copies would delete each other's autosaved drawings, so this one will close. \
             Open the drawing from the window that is already open."
                .to_string(),
        ),
        _ => Ok(None),
    }
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
    use super::{config_base, config_root, ensure_private_dir, instance_lock_at, safe_id};
    use std::ffi::OsString;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    #[test]
    fn a_second_holder_of_the_instance_lock_is_refused_until_the_first_lets_go() {
        let dir = std::env::temp_dir().join(format!("excalidraw-instance-lock-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = instance_lock_at(&dir).unwrap().expect("the first copy gets the lock");
        // A separate open file description, as a second process would have.
        assert!(instance_lock_at(&dir).is_err(), "a second copy is told it is not alone");
        drop(first);
        // Retried: other tests spawn `mkfifo` meanwhile, and a forked child
        // shares this lock until its `exec` closes the descriptor.
        let again = (0..100).find_map(|_| {
            instance_lock_at(&dir).ok().flatten().or_else(|| {
                std::thread::sleep(std::time::Duration::from_millis(10));
                None
            })
        });
        assert!(again.is_some(), "released when its holder goes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_or_relative_xdg_config_home_is_ignored() {
        let s = |v: &str| Some(OsString::from(v));
        let home = s("/home/u");
        assert_eq!(config_base(s("/x/cfg"), home.clone()), Some(PathBuf::from("/x/cfg")));
        assert_eq!(config_base(s(""), home.clone()), Some(PathBuf::from("/home/u/.config")));
        assert_eq!(config_base(s(".config"), home.clone()), Some(PathBuf::from("/home/u/.config")));
        assert_eq!(config_base(None, home), Some(PathBuf::from("/home/u/.config")));
        assert_eq!(config_base(s("rel"), s("also-rel")), None);
    }

    #[test]
    fn a_relative_or_empty_home_never_gives_a_relative_config_directory() {
        let s = |v: &str| Some(OsString::from(v));
        for home in [s("relative"), s(""), None] {
            let root = config_root(None, home.clone());
            assert!(root.is_absolute(), "HOME={home:?} gave {}", root.display());
            assert!(root.ends_with(".config"));
        }
        assert_eq!(config_root(None, s("/home/u")), PathBuf::from("/home/u/.config"));
    }

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
