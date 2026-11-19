//! Fallback for operating systems without a backend yet. Everything compiles, and every
//! call that would need the OS reports [`Error::Unsupported`] instead of pretending to work.

use keyless_core::Shortcut;
use raw_window_handle::RawWindowHandle;

use crate::{Anchor, BlockerEvent, BlockerOptions, Error, PopupKind};

pub struct Blocker(());

impl Blocker {
    pub fn start(
        _options: BlockerOptions,
        _on_event: impl Fn(BlockerEvent) + Send + 'static,
    ) -> Result<Self, Error> {
        Err(Error::Unsupported)
    }
    pub fn is_locked(&self) -> bool {
        false
    }
    pub fn set_locked(&self, _locked: bool) {}
    pub fn set_shortcut(&self, _shortcut: Option<Shortcut>) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
    pub fn set_shortcut_unlocks(&self, _unlocks: bool) {}
    pub fn set_recording(&self, _recording: bool) {}
}

pub mod autostart {
    use crate::Error;

    pub fn is_enabled() -> bool {
        false
    }
    pub fn set_enabled(_enabled: bool) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
}

pub mod instance {
    use crate::Error;

    pub struct Guard(());

    pub fn acquire(_on_activate: impl Fn() + Send + 'static) -> Result<Option<Guard>, Error> {
        Err(Error::Unsupported)
    }
}

pub mod layout {
    use keyless_core::KeyCap;

    pub fn active_layout() -> usize {
        0
    }
    /// Empty: the drawn keyboard keeps its built-in US labels.
    pub fn key_caps() -> Vec<KeyCap> {
        Vec::new()
    }
}

pub mod desktop {
    use super::*;

    pub const SUPER_KEY_NAME: &str = "Super";

    pub fn animations_enabled() -> bool {
        true
    }
    pub fn font_for_language(_language: &str) -> Option<&'static str> {
        None
    }
    pub fn open_url(_url: &str) {}
    pub fn show_error(message: &str) {
        eprintln!("{}: {message}", keyless_core::APP_NAME);
    }
    pub fn style_popup(_window: RawWindowHandle, _kind: PopupKind) {}
    pub fn set_visible_passive(_window: RawWindowHandle, _visible: bool) {}
    pub fn bring_to_front(_window: RawWindowHandle) {}
    pub fn place(_anchor: Anchor, _logical_size: (f32, f32)) -> Option<(i32, i32)> {
        None
    }
    pub fn fit(_spot: (i32, i32), _logical_size: (f32, f32)) -> Option<(i32, i32)> {
        None
    }
}
