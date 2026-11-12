//! The app runs once. A second launch (a double-click in the file manager)
//! hands its drawings to the window already open and exits: two copies
//! would share one config directory and prune each other's autosaves.
//!
//! The lock is core's `flock` (`store::instance_lock`), released by the
//! kernel when its holder dies. The hand-over is a Unix socket beside it:
//! paths, one per line. This is the plan's open question 2, answered with a
//! socket rather than D-Bus, which needs nothing but the filesystem.

use iced::futures::channel::mpsc;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Mutex;

fn socket_path() -> PathBuf {
    excalidraw_desktop_core::store::config_dir().join("instance.sock")
}

/// What `claim` found.
pub enum Claim {
    /// This is the only copy: keep the lock for as long as the app runs.
    First(Option<std::fs::File>),
    /// Another copy took the drawings.
    HandedOver,
}

static INCOMING: Mutex<Option<mpsc::UnboundedReceiver<Vec<PathBuf>>>> = Mutex::new(None);

/// Takes the lock, or gives `paths` to the copy that holds it.
pub fn claim(paths: &[PathBuf]) -> Result<Claim, String> {
    match excalidraw_desktop_core::store::instance_lock() {
        Ok(lock) => {
            listen();
            Ok(Claim::First(lock))
        }
        Err(why) => {
            let mut stream = UnixStream::connect(socket_path()).map_err(|_| why)?;
            let mut message = String::new();
            for p in paths {
                message.push_str(&p.to_string_lossy());
                message.push('\n');
            }
            stream.write_all(message.as_bytes()).map_err(|e| e.to_string())?;
            Ok(Claim::HandedOver)
        }
    }
}

/// Accepts hand-overs on a thread of its own, for `incoming` to deliver.
fn listen() {
    let path = socket_path();
    // Left by a copy that died: the lock says nobody is listening on it.
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else { return };
    let (tx, rx) = mpsc::unbounded();
    if let Ok(mut slot) = INCOMING.lock() {
        *slot = Some(rx);
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let paths: Vec<PathBuf> = BufReader::new(stream).lines().map_while(Result::ok).filter(|l| !l.is_empty()).map(PathBuf::from).collect();
            if tx.unbounded_send(paths).is_err() {
                break;
            }
        }
    });
}

/// The drawings later launches hand over, for a subscription.
pub fn incoming() -> impl iced::futures::Stream<Item = Vec<PathBuf>> {
    use iced::futures::StreamExt;
    match INCOMING.lock().ok().and_then(|mut slot| slot.take()) {
        Some(rx) => rx.boxed(),
        None => iced::futures::stream::pending().boxed(),
    }
}

/// Removes the socket on the way out, so a stale one is not left behind.
pub fn release() {
    let _ = std::fs::remove_file(socket_path());
}
