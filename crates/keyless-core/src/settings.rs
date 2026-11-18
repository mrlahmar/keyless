//! User settings, stored as a small TOML file in the platform's config directory.
//! Unknown or invalid values fall back to defaults, so an edited file never stops the app.

use std::path::Path;
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::shortcut::Shortcut;

/// Auto-resume choices offered in the UI, in minutes. `0` means never.
pub const AUTO_RESUME_CHOICES: [u32; 5] = [0, 5, 15, 30, 60];

/// Light or dark appearance. `System` follows the operating system setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    /// Order used by the UI's segmented control.
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];
}

/// Screen edge where the paused tag appears, unless the user dragged it somewhere else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    /// Order used by the UI's position picker.
    pub const ALL: [Self; 4] = [Self::Top, Self::Bottom, Self::Left, Self::Right];
}

/// Saved preferences. The pause state is deliberately not one of them: Keyless always
/// starts with the keyboard on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub shortcut: Shortcut,
    /// The shortcut also resumes the keyboard. When off, only the mouse can resume it.
    pub shortcut_unlocks: bool,
    /// Show a small "Keyboard paused" tag on screen while paused.
    pub show_indicator: bool,
    /// Where the tag appears.
    pub indicator_edge: Edge,
    /// Where the user last dragged the tag: its top-left corner in physical screen pixels.
    /// Overrides `indicator_edge` while that spot is still on a screen.
    pub indicator_spot: Option<(i32, i32)>,
    /// Resume automatically after this many minutes. `0` disables it.
    pub auto_resume_minutes: u32,
    pub appearance: Appearance,
    /// UI language code such as "fr". `None` follows the operating system.
    pub language: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: Shortcut::default(),
            shortcut_unlocks: true,
            show_indicator: true,
            indicator_edge: Edge::Top,
            indicator_spot: None,
            auto_resume_minutes: 0,
            appearance: Appearance::System,
            language: None,
        }
    }
}

/// On-disk form. Every field is optional so partial files still load.
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct File {
    shortcut: Option<String>,
    shortcut_unlocks: Option<bool>,
    show_indicator: Option<bool>,
    indicator_edge: Option<toml::Value>,
    indicator_spot: Option<[i32; 2]>,
    auto_resume_minutes: Option<u32>,
    appearance: Option<toml::Value>,
    language: Option<String>,
}

impl Settings {
    pub fn from_toml(text: &str) -> Self {
        let file: File = toml::from_str(text).unwrap_or_default();
        let d = Self::default();
        Self {
            shortcut: file.shortcut.and_then(|s| s.parse().ok()).unwrap_or(d.shortcut),
            shortcut_unlocks: file.shortcut_unlocks.unwrap_or(d.shortcut_unlocks),
            show_indicator: file.show_indicator.unwrap_or(d.show_indicator),
            indicator_edge: file.indicator_edge.and_then(|v| v.try_into().ok()).unwrap_or(d.indicator_edge),
            indicator_spot: file.indicator_spot.map(|[x, y]| (x, y)),
            auto_resume_minutes: file
                .auto_resume_minutes
                .filter(|m| AUTO_RESUME_CHOICES.contains(m))
                .unwrap_or(d.auto_resume_minutes),
            appearance: file.appearance.and_then(|v| v.try_into().ok()).unwrap_or(d.appearance),
            language: file.language.filter(|code| is_language_code(code)),
        }
    }

    pub fn to_toml(&self) -> String {
        let file = File {
            shortcut: Some(self.shortcut.to_string()),
            shortcut_unlocks: Some(self.shortcut_unlocks),
            show_indicator: Some(self.show_indicator),
            indicator_edge: toml::Value::try_from(self.indicator_edge).ok(),
            indicator_spot: self.indicator_spot.map(|(x, y)| [x, y]),
            auto_resume_minutes: Some(self.auto_resume_minutes),
            appearance: toml::Value::try_from(self.appearance).ok(),
            language: self.language.clone(),
        };
        toml::to_string(&file).expect("settings always serialize")
    }

    /// Missing or unreadable files give defaults.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).map(|t| Self::from_toml(&t)).unwrap_or_default()
    }

    /// Writes atomically: a temporary file is renamed over the old one.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, self.to_toml())?;
        fs::rename(&tmp, path)
    }
}

/// A language tag such as "fr" or "pt-BR". Anything else in the file is ignored.
fn is_language_code(code: &str) -> bool {
    (2..=12).contains(&code.len()) && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let settings = Settings {
            shortcut: "Ctrl+Shift+F9".parse().unwrap(),
            shortcut_unlocks: false,
            show_indicator: false,
            indicator_edge: Edge::Left,
            indicator_spot: Some((-1200, 340)),
            auto_resume_minutes: 15,
            appearance: Appearance::Dark,
            language: Some("ar".into()),
        };
        assert_eq!(Settings::from_toml(&settings.to_toml()), settings);
    }

    #[test]
    fn bad_values_fall_back_to_defaults() {
        let s = Settings::from_toml("shortcut = \"K\"\nauto-resume-minutes = 7\nshow-indicator = false");
        assert_eq!(s.shortcut, Shortcut::default());
        assert_eq!(s.auto_resume_minutes, 0);
        assert!(!s.show_indicator);
        assert_eq!(s.appearance, Appearance::System);
        assert_eq!(Settings::from_toml("language = \"../x\"").language, None);
        assert_eq!(Settings::from_toml("not toml ["), Settings::default());
    }

    #[test]
    fn saves_and_loads_from_disk() {
        let dir = std::env::temp_dir().join(format!("keyless-test-{}", std::process::id()));
        let path = dir.join("settings.toml");
        let settings = Settings { auto_resume_minutes: 30, ..Settings::default() };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        fs::remove_dir_all(dir).unwrap();
    }
}
