//! Linux input event codes (`linux/input-event-codes.h`) and what they mean to Keyless.
//!
//! Key codes name a physical position (`KEY_Q` is the key right of Tab), not a character.
//! Shortcuts therefore follow the US position of a key, whatever layout is active.

use keyless_core::{Key, Modifiers, MouseEvent};

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_MSC: u16 = 0x04;

pub const SYN_REPORT: u16 = 0;
pub const SYN_DROPPED: u16 = 3;

/// Number of key codes (`KEY_CNT`).
pub const KEY_COUNT: u16 = 0x300;

const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;

/// Letters by position on a US keyboard, as rows of consecutive codes.
const LETTER_ROWS: [(u16, &str); 3] = [(16, "QWERTYUIOP"), (30, "ASDFGHJKL"), (44, "ZXCVBNM")];

pub fn modifier(code: u16) -> Option<Modifiers> {
    match code {
        42 | 54 => Some(Modifiers::SHIFT),   // KEY_LEFTSHIFT, KEY_RIGHTSHIFT
        29 | 97 => Some(Modifiers::CTRL),    // KEY_LEFTCTRL, KEY_RIGHTCTRL
        56 | 100 => Some(Modifiers::ALT),    // KEY_LEFTALT, KEY_RIGHTALT
        125 | 126 => Some(Modifiers::SUPER), // KEY_LEFTMETA, KEY_RIGHTMETA
        _ => None,
    }
}

pub fn shortcut_key(code: u16) -> Option<Key> {
    for (start, letters) in LETTER_ROWS {
        if let Some(letter) = code.checked_sub(start).and_then(|i| letters.as_bytes().get(usize::from(i))) {
            return Some(Key::Letter(char::from(*letter)));
        }
    }
    match code {
        2..=10 => Some(Key::Digit((code - 1) as u8)), // KEY_1 to KEY_9
        11 => Some(Key::Digit(0)),
        59..=68 => Some(Key::Function((code - 58) as u8)), // KEY_F1 to KEY_F10
        87 | 88 => Some(Key::Function((code - 76) as u8)), // KEY_F11, KEY_F12
        183..=194 => Some(Key::Function((code - 170) as u8)), // KEY_F13 to KEY_F24
        _ => None,
    }
}

/// Mouse buttons, numbered like [`MouseEvent::Button`]: left, right, middle, back, forward.
pub fn mouse_button(code: u16) -> Option<u8> {
    match code {
        0x110 => Some(0),         // BTN_LEFT
        0x111 => Some(1),         // BTN_RIGHT
        0x112 => Some(2),         // BTN_MIDDLE
        0x113 | 0x116 => Some(3), // BTN_SIDE, BTN_BACK
        0x114 | 0x115 => Some(4), // BTN_EXTRA, BTN_FORWARD
        0x117 => Some(5),         // BTN_TASK
        _ => None,
    }
}

/// What a relative or absolute axis event means for [`keyless_core::Mouse`].
pub fn axis_event(kind: u16, code: u16) -> MouseEvent {
    match (kind, code) {
        (EV_REL, REL_X | REL_Y) | (EV_ABS, _) => MouseEvent::Move,
        // Wheels, high-resolution wheels and dials.
        _ => MouseEvent::Wheel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_letter_digit_and_function_key_has_one_code() {
        let mut seen = std::collections::HashSet::new();
        for code in 0..KEY_COUNT {
            if let Some(key) = shortcut_key(code) {
                assert!(seen.insert(key), "{key:?} appears twice");
            }
        }
        let letters = seen.iter().filter(|k| matches!(k, Key::Letter(_))).count();
        let digits = seen.iter().filter(|k| matches!(k, Key::Digit(_))).count();
        let functions = seen.iter().filter(|k| matches!(k, Key::Function(_))).count();
        assert_eq!((letters, digits, functions), (26, 10, 24));
    }

    const KEY_A: u16 = 30;
    const KEY_Z: u16 = 44;

    #[test]
    fn known_codes() {
        assert_eq!(shortcut_key(KEY_A), Some(Key::Letter('A')));
        assert_eq!(shortcut_key(KEY_Z), Some(Key::Letter('Z')));
        assert_eq!(shortcut_key(37), Some(Key::Letter('K')));
        assert_eq!(shortcut_key(2), Some(Key::Digit(1)));
        assert_eq!(shortcut_key(11), Some(Key::Digit(0)));
        assert_eq!(shortcut_key(59), Some(Key::Function(1)));
        assert_eq!(shortcut_key(88), Some(Key::Function(12)));
        assert_eq!(shortcut_key(194), Some(Key::Function(24)));
        assert_eq!(shortcut_key(1), None); // KEY_ESC
        assert_eq!(shortcut_key(26), None); // KEY_LEFTBRACE, after P
        assert_eq!(modifier(29), Some(Modifiers::CTRL));
        assert_eq!(modifier(100), Some(Modifiers::ALT));
        assert_eq!(modifier(30), None);
    }

    #[test]
    fn mouse_codes() {
        assert_eq!(mouse_button(0x110), Some(0));
        assert_eq!(mouse_button(0x116), Some(3));
        assert_eq!(mouse_button(0x14a), None); // BTN_TOUCH belongs to touchpads, not to buttons
        assert_eq!(axis_event(EV_REL, REL_X), MouseEvent::Move);
        assert_eq!(axis_event(EV_REL, 0x08), MouseEvent::Wheel); // REL_WHEEL
        assert_eq!(axis_event(EV_ABS, 0x35), MouseEvent::Move); // ABS_MT_POSITION_X
    }
}
