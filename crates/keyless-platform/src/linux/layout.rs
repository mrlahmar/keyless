//! What each key does under the active keyboard layout, for the drawn keyboard.
//!
//! Keyless reads keys below the display server, where a key is a position, not a
//! character, and shortcuts follow that position ([`super::keys`]). The drawn keyboard
//! therefore keeps its built-in US labels, which name exactly those positions. Wayland
//! doesn't tell other apps which layout is active, so showing local labels would need a
//! per-desktop source (GNOME settings, KDE's config, X11's XKB); that can come later.

use keyless_core::KeyCap;

pub fn active_layout() -> usize {
    0
}

/// Empty: the drawn keyboard keeps its built-in US labels.
pub fn key_caps() -> Vec<KeyCap> {
    Vec::new()
}
