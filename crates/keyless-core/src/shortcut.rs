//! Portable shortcut model. Platforms translate native key codes to and from [`Key`].

use std::fmt;
use std::str::FromStr;

/// Modifier bit set. Values match Windows `MOD_*` so the Windows backend can pass them through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const ALT: Self = Self(1);
    pub const CTRL: Self = Self(2);
    pub const SHIFT: Self = Self(4);
    /// Windows key, Command key on macOS, Super on Linux.
    pub const SUPER: Self = Self(8);

    pub const fn bits(self) -> u8 {
        self.0
    }
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0b1111)
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for Modifiers {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

/// A key that can complete a shortcut: a letter, a digit or a function key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// Uppercase ASCII letter `A`..=`Z`.
    Letter(char),
    /// Digit `0`..=`9`.
    Digit(u8),
    /// Function key `F1`..=`F24`.
    Function(u8),
}

impl Key {
    pub fn is_valid(self) -> bool {
        match self {
            Key::Letter(c) => c.is_ascii_uppercase(),
            Key::Digit(d) => d <= 9,
            Key::Function(n) => (1..=24).contains(&n),
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Letter(c) => write!(f, "{c}"),
            Key::Digit(d) => write!(f, "{d}"),
            Key::Function(n) => write!(f, "F{n}"),
        }
    }
}

/// At least one modifier plus one [`Key`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self { modifiers: Modifiers::CTRL | Modifiers::ALT, key: Key::Letter('K') }
    }
}

impl Shortcut {
    pub fn is_valid(&self) -> bool {
        !self.modifiers.is_empty() && self.key.is_valid()
    }

    /// Display parts in conventional order, e.g. `["Ctrl", "Alt", "K"]`.
    /// `super_name` is the platform's name for the Super key ("Win", "Cmd", "Super").
    pub fn parts(&self, super_name: &str) -> Vec<String> {
        let mut parts = Vec::with_capacity(5);
        for (flag, name) in [
            (Modifiers::CTRL, "Ctrl"),
            (Modifiers::ALT, "Alt"),
            (Modifiers::SHIFT, "Shift"),
            (Modifiers::SUPER, super_name),
        ] {
            if self.modifiers.contains(flag) {
                parts.push(name.to_owned());
            }
        }
        parts.push(self.key.to_string());
        parts
    }
}

/// Stable, platform-neutral form used in the settings file: `Ctrl+Alt+K`.
impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.parts("Super").join("+"))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseShortcutError(String);

impl fmt::Display for ParseShortcutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid shortcut: {}", self.0)
    }
}

impl std::error::Error for ParseShortcutError {}

impl FromStr for Shortcut {
    type Err = ParseShortcutError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseShortcutError(s.to_owned());
        let mut modifiers = Modifiers::NONE;
        let mut key = None;
        for part in s.split('+').map(str::trim) {
            let upper = part.to_ascii_uppercase();
            let modifier = match upper.as_str() {
                "CTRL" | "CONTROL" => Some(Modifiers::CTRL),
                "ALT" | "OPTION" => Some(Modifiers::ALT),
                "SHIFT" => Some(Modifiers::SHIFT),
                "SUPER" | "WIN" | "CMD" | "META" => Some(Modifiers::SUPER),
                _ => None,
            };
            if let Some(m) = modifier {
                modifiers |= m;
                continue;
            }
            if key.is_some() {
                return Err(err());
            }
            let mut chars = upper.chars();
            key = Some(match (chars.next(), chars.as_str()) {
                (Some(c), "") if c.is_ascii_uppercase() => Key::Letter(c),
                (Some(c), "") if c.is_ascii_digit() => Key::Digit(c as u8 - b'0'),
                (Some('F'), n) => Key::Function(n.parse().map_err(|_| err())?),
                _ => return Err(err()),
            });
        }
        let shortcut = Shortcut { modifiers, key: key.ok_or_else(err)? };
        if shortcut.is_valid() { Ok(shortcut) } else { Err(err()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        for text in ["Ctrl+Alt+K", "Shift+Super+F12", "Ctrl+7"] {
            let shortcut: Shortcut = text.parse().unwrap();
            assert_eq!(shortcut.to_string(), text);
        }
    }

    #[test]
    fn rejects_incomplete_shortcuts() {
        for text in ["K", "Ctrl+Alt", "Ctrl+F25", "Ctrl+K+J", "Ctrl+Esc", ""] {
            assert!(text.parse::<Shortcut>().is_err(), "{text}");
        }
    }

    #[test]
    fn accepts_platform_aliases() {
        let shortcut: Shortcut = "cmd + option + k".parse().unwrap();
        assert_eq!(shortcut.parts("Cmd"), ["Alt", "Cmd", "K"]);
    }
}
