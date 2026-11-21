//! One Keyless per user. Starting it again opens the running copy instead.
//!
//! A lock file decides who runs; a Unix socket next to it carries "open your window".
//! Both live in `$XDG_RUNTIME_DIR`, which only the user can access.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use crate::Error;

/// Keeps this process registered as the running instance until dropped.
pub struct Guard {
    _lock: File,
    socket: PathBuf,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        // SAFETY: getuid never fails.
        .unwrap_or_else(|| std::env::temp_dir().join(format!("keyless-{}", unsafe { libc::getuid() })))
}

/// Returns `Ok(None)` if another instance is running; that instance is asked to activate.
/// Otherwise `on_activate` runs (on a background thread) whenever a later launch asks.
pub fn acquire(on_activate: impl Fn() + Send + 'static) -> Result<Option<Guard>, Error> {
    let os = |e: std::io::Error| Error::Os(e.to_string());
    let dir = runtime_dir();
    std::fs::create_dir_all(&dir).map_err(os)?;
    // Named after the app, so a dev build and the installed app don't see each other.
    let id = super::app_id();
    let socket = dir.join(format!("{id}.sock"));
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(dir.join(format!("{id}.lock")))
        .map_err(os)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            if let Ok(mut stream) = UnixStream::connect(&socket) {
                let _ = stream.write_all(b"activate\n");
            }
            return Ok(None);
        }
        Err(TryLockError::Error(e)) => return Err(os(e)),
    }
    // Left over from a copy that crashed.
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).map_err(os)?;
    std::thread::Builder::new()
        .name("keyless-instance".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                drop(stream);
                on_activate();
            }
        })
        .map_err(os)?;
    Ok(Some(Guard { _lock: lock, socket }))
}
