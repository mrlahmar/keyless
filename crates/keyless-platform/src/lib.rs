//! Everything Keyless needs from the operating system, behind one API.
//!
//! Each OS lives in its own module and provides the same items:
//!
//! - [`Blocker`]: suppresses physical keyboard input on a dedicated thread, using
//!   [`keyless_core::Keyboard`] for every decision. Optionally pauses the mouse too
//!   ([`keyless_core::Mouse`]). It resumes both on screen lock, logoff and sleep.
//! - [`autostart`]: start at sign-in.
//! - [`instance`]: single instance, with "activate the running copy".
//! - [`layout`]: what each drawn key does under the active keyboard layout.
//! - [`desktop`]: popup window styling and placement, links, system preferences.
//!
//! To port Keyless to another OS, add a module that provides these items and select it
//! below. The UI never calls OS APIs directly.

use std::fmt;

use keyless_core::{Capture, Edge, Shortcut};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub use unsupported::*;

/// Events reported by the [`Blocker`] from its input thread.
/// Handlers must return quickly: they run inside the keyboard hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockerEvent {
    /// The keyboard was paused (`true`) or resumed (`false`) by the shortcut or by the
    /// system (screen lock, sleep).
    LockChanged(bool),
    /// Shortcut recording finished.
    Captured(Capture),
}

/// Initial configuration for [`Blocker::start`].
#[derive(Clone, Copy, Debug)]
pub struct BlockerOptions {
    pub shortcut: Option<Shortcut>,
    pub shortcut_unlocks: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// This OS is not supported yet.
    Unsupported,
    /// Another application or the OS already uses this shortcut.
    ShortcutTaken,
    /// An OS call failed.
    Os(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unsupported => f.write_str("this operating system is not supported yet"),
            Error::ShortcutTaken => f.write_str("the shortcut is already used by another app"),
            Error::Os(message) => write!(f, "system error: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// Where to put a popup window.
#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    /// Next to the mouse pointer, kept inside the work area (for the tray panel).
    Pointer,
    /// Centered along one edge of the work area that contains the pointer.
    Edge(Edge),
}

/// How a popup window behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupKind {
    /// Takes focus, hidden from the taskbar (tray panel).
    Panel,
    /// Never takes focus, stays on top, hidden from the taskbar (paused indicator).
    Overlay,
}
