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

/// True in debug builds (`cargo run`). They call themselves "Keyless Dev" and keep their own
/// settings folder, single-instance lock and sign-in entry, so they run next to an
/// installed Keyless without touching it.
pub const DEV_BUILD: bool = cfg!(debug_assertions);

/// The app's name in titles, the settings folder and the sign-in entry.
pub const APP_NAME: &str = if DEV_BUILD { "Keyless Dev" } else { "Keyless" };

/// True for versions shown as beta in the app: `0.x.y`, or any version with a suffix such
/// as `1.2.0-beta.1` or `1.2.0-rc.1`. GitHub marks only the suffixed ones as pre-releases.
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
