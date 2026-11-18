//! Platform-independent parts of Keyless: the keyboard state machine, the shortcut
//! model, keyboard layout positions and settings. Nothing here touches the OS or the UI.

pub mod keyboard;
pub mod layout;
pub mod settings;
pub mod shortcut;

pub use keyboard::{Capture, Decision, KeyEvent, Keyboard, MAX_KEY_CODE};
pub use layout::{KeyCap, LAYOUT_KEYS};
pub use settings::{AUTO_RESUME_CHOICES, Appearance, Edge, Settings};
pub use shortcut::{Key, Modifiers, Shortcut};

/// True for versions that are not yet stable: `0.x.y`, or any version with a pre-release
/// part such as `1.2.0-rc.1`. Matches how the release workflow marks pre-releases.
pub fn is_prerelease(version: &str) -> bool {
    version.starts_with("0.") || version.contains('-')
}

#[cfg(test)]
mod tests {
    use super::is_prerelease;

    #[test]
    fn prerelease_versions() {
        assert!(is_prerelease("0.1.0"));
        assert!(is_prerelease("1.2.0-rc.1"));
        assert!(!is_prerelease("1.0.0"));
        assert!(!is_prerelease("10.2.3"));
    }
}
