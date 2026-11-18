//! The keyboard state machine. It decides, for every physical key event, whether the
//! event is passed on or consumed.
//!
//! Runs inside the platform's input hook, so it does no I/O, no allocation and no locking.
//! Every key-down that reaches applications gets its matching key-up, and every key-down
//! that was blocked has its key-up blocked too, across lock and unlock transitions.

use crate::shortcut::{Key, Modifiers, Shortcut};

/// Upper bound (exclusive) for platform key codes.
/// Covers Windows virtual keys, macOS key codes and Linux evdev codes.
pub const MAX_KEY_CODE: u16 = 1024;

/// One physical key event, already classified by the platform layer.
#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    /// Platform key code, unique per physical key. Codes `>= MAX_KEY_CODE` are ignored.
    pub code: u16,
    pub down: bool,
    /// Set when the key is a modifier. Left and right variants map to the same value.
    pub modifier: Option<Modifiers>,
    /// Set when the key can complete a shortcut.
    pub key: Option<Key>,
}

/// Result of recording a new shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    Shortcut(Shortcut),
    /// A non-modifier key was pressed, but the combination is not a usable shortcut.
    Invalid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Decision {
    /// The event must not reach applications.
    pub consume: bool,
    /// The lock state changed because of this event.
    pub toggled: bool,
    /// Recording finished with this event.
    pub captured: Option<Capture>,
}

#[derive(Clone)]
struct KeySet([u64; MAX_KEY_CODE as usize / 64]);

impl KeySet {
    const fn new() -> Self {
        Self([0; MAX_KEY_CODE as usize / 64])
    }
    fn get(&self, code: u16) -> bool {
        self.0[code as usize / 64] & (1 << (code % 64)) != 0
    }
    fn set(&mut self, code: u16, value: bool) {
        let bit = 1 << (code % 64);
        let word = &mut self.0[code as usize / 64];
        if value { *word |= bit } else { *word &= !bit }
    }
    fn clear(&mut self) {
        self.0 = [0; MAX_KEY_CODE as usize / 64];
    }
    fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        self.0.iter().enumerate().flat_map(|(i, &word)| {
            (0..64).filter(move |b| word & (1 << b) != 0).map(move |b| (i * 64 + b) as u16)
        })
    }
}

#[derive(Clone)]
pub struct Keyboard {
    down: KeySet,
    swallowed: KeySet,
    delivered: KeySet,
    modifier_of: [u8; MAX_KEY_CODE as usize],
    locked: bool,
    recording: bool,
    shortcut: Option<Shortcut>,
    shortcut_unlocks: bool,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Keyboard {
    pub const fn new() -> Self {
        Self {
            down: KeySet::new(),
            swallowed: KeySet::new(),
            delivered: KeySet::new(),
            modifier_of: [0; MAX_KEY_CODE as usize],
            locked: false,
            recording: false,
            shortcut: None,
            shortcut_unlocks: true,
        }
    }

    pub fn locked(&self) -> bool {
        self.locked
    }
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }
    pub fn recording(&self) -> bool {
        self.recording
    }
    /// While recording, every key is consumed and the next non-modifier key-down ends the recording.
    pub fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
    }
    /// `None` disables the keyboard shortcut (mouse control still works).
    pub fn set_shortcut(&mut self, shortcut: Option<Shortcut>) {
        self.shortcut = shortcut;
    }
    /// When `false`, the shortcut can pause the keyboard but not resume it.
    pub fn set_shortcut_unlocks(&mut self, unlocks: bool) {
        self.shortcut_unlocks = unlocks;
    }

    /// Forget which keys are physically held. Call after sleep or session switches,
    /// when key-up events may have been missed.
    pub fn reset_physical_state(&mut self) {
        self.down.clear();
    }

    /// Modifiers currently held down.
    pub fn modifiers(&self) -> Modifiers {
        self.down
            .iter()
            .fold(Modifiers::NONE, |acc, code| acc | Modifiers::from_bits(self.modifier_of[code as usize]))
    }

    pub fn event(&mut self, event: KeyEvent) -> Decision {
        let code = event.code;
        if code >= MAX_KEY_CODE {
            return Decision { consume: self.locked, ..Decision::default() };
        }
        let repeat = event.down && self.down.get(code);
        self.down.set(code, event.down);
        self.modifier_of[code as usize] = event.modifier.map_or(0, Modifiers::bits);

        let mut decision = Decision::default();
        if !event.down {
            decision.consume = self.swallowed.get(code) && !self.delivered.get(code);
            self.swallowed.set(code, false);
            self.delivered.set(code, false);
            return decision;
        }

        if self.recording {
            decision.consume = true;
            if !repeat && event.modifier.is_none() {
                let shortcut = event.key.map(|key| Shortcut { modifiers: self.modifiers(), key });
                decision.captured = Some(match shortcut {
                    Some(s) if s.is_valid() => Capture::Shortcut(s),
                    _ => Capture::Invalid,
                });
                self.recording = false;
            }
        } else if !repeat && self.matches_shortcut(event.key) {
            decision.consume = true;
            if !self.locked || self.shortcut_unlocks {
                self.locked = !self.locked;
                decision.toggled = true;
            }
        } else {
            decision.consume = self.locked || self.swallowed.get(code);
        }

        if decision.consume {
            self.swallowed.set(code, true);
        } else {
            self.delivered.set(code, true);
        }
        decision
    }

    fn matches_shortcut(&self, key: Option<Key>) -> bool {
        match (self.shortcut, key) {
            (Some(s), Some(k)) => s.key == k && self.modifiers() == s.modifiers,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LCTRL: u16 = 0xA2;
    const RCTRL: u16 = 0xA3;
    const LALT: u16 = 0xA4;
    const LSHIFT: u16 = 0xA0;

    fn key(k: &mut Keyboard, code: u16, down: bool) -> Decision {
        let (modifier, key) = match code {
            LCTRL | RCTRL => (Some(Modifiers::CTRL), None),
            LALT => (Some(Modifiers::ALT), None),
            LSHIFT => (Some(Modifiers::SHIFT), None),
            c @ 0x41..=0x5A => (None, Some(Key::Letter(c as u8 as char))),
            _ => (None, None),
        };
        k.event(KeyEvent { code, down, modifier, key })
    }

    fn keyboard() -> Keyboard {
        let mut k = Keyboard::new();
        k.set_shortcut(Some(Shortcut::default()));
        k
    }

    const A: u16 = b'A' as u16;
    const B: u16 = b'B' as u16;
    const J: u16 = b'J' as u16;
    const K: u16 = b'K' as u16;

    /// The pause state is never saved, so every start (including at sign-in after a
    /// shutdown while paused) begins with a working keyboard.
    #[test]
    fn starts_unpaused() {
        assert!(!Keyboard::new().locked());
        assert!(!Keyboard::new().recording());
    }

    #[test]
    fn unlocked_keys_pass_and_locked_keys_are_consumed() {
        let mut k = keyboard();
        assert!(!key(&mut k, A, true).consume);
        assert!(!key(&mut k, A, false).consume);
        k.set_locked(true);
        assert!(key(&mut k, A, true).consume);
        assert!(key(&mut k, A, false).consume);
    }

    #[test]
    fn delivered_press_keeps_its_release_after_locking() {
        let mut k = keyboard();
        key(&mut k, A, true);
        k.set_locked(true);
        assert!(key(&mut k, A, true).consume, "auto-repeat after locking is blocked");
        assert!(!key(&mut k, A, false).consume, "the original press still gets its release");
    }

    #[test]
    fn key_held_through_unlock_stays_blocked_until_released() {
        let mut k = keyboard();
        k.set_locked(true);
        assert!(key(&mut k, B, true).consume);
        k.set_locked(false);
        assert!(key(&mut k, B, true).consume, "resting hand must not start typing on unlock");
        assert!(key(&mut k, B, false).consume);
        assert!(!key(&mut k, B, true).consume, "a fresh press types again");
    }

    #[test]
    fn shortcut_toggles_once_per_press_and_works_while_locked() {
        let mut k = keyboard();
        key(&mut k, LCTRL, true);
        key(&mut k, LALT, true);
        assert!(key(&mut k, K, true).toggled && k.locked());
        assert!(!key(&mut k, K, true).toggled, "auto-repeat cannot toggle");
        assert!(key(&mut k, K, false).consume);
        assert!(key(&mut k, K, true).toggled && !k.locked());
        key(&mut k, K, false);
        assert!(!key(&mut k, LCTRL, false).consume, "modifiers pressed before locking are released");
        assert!(!key(&mut k, LALT, false).consume);
    }

    #[test]
    fn shortcut_can_be_limited_to_pausing() {
        let mut k = keyboard();
        k.set_shortcut_unlocks(false);
        key(&mut k, LCTRL, true);
        key(&mut k, LALT, true);
        assert!(key(&mut k, K, true).toggled && k.locked());
        key(&mut k, K, false);
        let d = key(&mut k, K, true);
        assert!(d.consume && !d.toggled && k.locked(), "only the mouse can resume");
    }

    #[test]
    fn disabled_shortcut_never_toggles() {
        let mut k = keyboard();
        k.set_shortcut(None);
        key(&mut k, LCTRL, true);
        key(&mut k, LALT, true);
        let d = key(&mut k, K, true);
        assert!(!d.toggled && !d.consume);
    }

    #[test]
    fn recording_consumes_input_and_captures_a_shortcut() {
        let mut k = keyboard();
        k.set_recording(true);
        assert!(key(&mut k, LSHIFT, true).consume);
        let d = key(&mut k, J, true);
        assert!(d.consume);
        let expected = Shortcut { modifiers: Modifiers::SHIFT, key: Key::Letter('J') };
        assert_eq!(d.captured, Some(Capture::Shortcut(expected)));
        assert!(!k.recording());
        assert!(key(&mut k, J, false).consume, "releases of recorded keys stay hidden");
        assert!(key(&mut k, LSHIFT, false).consume);
    }

    #[test]
    fn recording_reports_unusable_combinations() {
        let mut k = keyboard();
        k.set_recording(true);
        assert_eq!(key(&mut k, J, true).captured, Some(Capture::Invalid), "no modifier");
        k.set_recording(true);
        key(&mut k, LCTRL, true);
        assert_eq!(key(&mut k, 0x1B, true).captured, Some(Capture::Invalid), "Esc is not allowed");
    }

    #[test]
    fn left_and_right_modifiers_are_one_logical_modifier() {
        let mut k = keyboard();
        key(&mut k, LCTRL, true);
        key(&mut k, RCTRL, true);
        key(&mut k, LCTRL, false);
        key(&mut k, LALT, true);
        assert!(key(&mut k, K, true).toggled);
    }

    #[test]
    fn reset_clears_stale_modifiers() {
        let mut k = keyboard();
        key(&mut k, LCTRL, true);
        key(&mut k, LALT, true);
        k.reset_physical_state();
        assert!(!key(&mut k, K, true).toggled);
    }

    #[test]
    fn out_of_range_codes_follow_lock_state() {
        let mut k = keyboard();
        assert!(!key(&mut k, MAX_KEY_CODE, true).consume);
        k.set_locked(true);
        assert!(key(&mut k, MAX_KEY_CODE, true).consume);
    }
}
