//! Keys whose meaning depends on the keyboard layout chosen in the OS.
//!
//! The drawn keyboard shows positions. The platform tells, for each position, what the key
//! there does under the active layout ([`KeyCap`]). Shortcuts stay tied to the key's
//! meaning (the letter it types), as OS shortcuts are, not to its position.

use crate::shortcut::Key;

/// Main-block positions that change with the layout, in the order the drawn keyboard shows
/// them (rows top to bottom, keys left to right). Names are W3C UI Events `code` values,
/// which name a position on a US keyboard, not a character.
#[rustfmt::skip]
pub const LAYOUT_KEYS: [&str; 47] = [
    "Backquote", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9", "Digit0", "Minus", "Equal",
    "KeyQ", "KeyW", "KeyE", "KeyR", "KeyT", "KeyY", "KeyU", "KeyI", "KeyO", "KeyP", "BracketLeft", "BracketRight", "Backslash",
    "KeyA", "KeyS", "KeyD", "KeyF", "KeyG", "KeyH", "KeyJ", "KeyK", "KeyL", "Semicolon", "Quote",
    "KeyZ", "KeyX", "KeyC", "KeyV", "KeyB", "KeyN", "KeyM", "Comma", "Period", "Slash",
];

/// What the key at one position does under the active layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyCap {
    /// Text for the drawn key. Empty when the layout gives nothing to show.
    pub label: String,
    /// Set when the key can end a shortcut.
    pub key: Option<Key>,
}

impl KeyCap {
    /// A key that can end a shortcut, labelled the way shortcuts name it.
    pub fn shortcut(key: Key) -> Self {
        Self { label: key.to_string(), key: Some(key) }
    }

    /// A key that types `character` but can't end a shortcut. Letters show in upper case,
    /// as keycaps do, unless that takes more than one character (`ß`).
    pub fn character(character: Option<char>) -> Self {
        let label = character.filter(|c| !c.is_control() && !c.is_whitespace()).map(|c| {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(u), None) => u.to_string(),
                _ => c.to_string(),
            }
        });
        Self { label: label.unwrap_or_default(), key: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_unique() {
        for (i, a) in LAYOUT_KEYS.iter().enumerate() {
            assert!(!LAYOUT_KEYS[i + 1..].contains(a), "{a} listed twice");
        }
    }

    #[test]
    fn shortcut_caps_use_shortcut_names() {
        assert_eq!(KeyCap::shortcut(Key::Letter('Q')).label, "Q");
        assert_eq!(KeyCap::shortcut(Key::Digit(1)).label, "1");
    }

    #[test]
    fn character_caps() {
        assert_eq!(KeyCap::character(Some('ù')).label, "Ù");
        assert_eq!(KeyCap::character(Some(';')).label, ";");
        assert_eq!(KeyCap::character(Some('ß')).label, "ß");
        assert_eq!(KeyCap::character(Some('\u{8}')).label, "");
        assert_eq!(KeyCap::character(None), KeyCap { label: String::new(), key: None });
    }
}
