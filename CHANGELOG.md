# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Debug builds run as "Keyless Dev", with a tinted icon and their own settings,
  single-instance lock and start-at-sign-in entry, so they run next to an installed Keyless.
  Their default shortcut is Ctrl + Alt + J.
- Mouse tab: optionally pause the mouse too, either clicks and scrolling or the pointer
  as well. Only the shortcut resumes then, so it always resumes, and the paused tag shows
  it instead of the Resume button. Keyless asks first before the paused tag is hidden
  while the mouse pauses. Screen lock, sign-out, sleep and quitting free the mouse.

## [0.1.0] - 2026-11-18

### Added

- Pause and resume the physical keyboard from the tray panel, the main window, a
  global shortcut (default Ctrl + Alt + K) or the tray menu. The mouse is never affected.
- Paused tag on screen that never takes focus. Only its Resume button resumes. Choose
  top, bottom, left or right, or drag it anywhere; Keyless remembers the spot.
- Set the shortcut by typing it or by clicking keys on a drawn keyboard. The drawn
  keyboard follows the keyboard layout set in Windows (AZERTY, QWERTZ, Dvorak and
  others) and updates when you switch layouts.
- Option to stop the shortcut from resuming, so resting hands can't resume by accident.
- Auto-resume after 5, 15, 30 or 60 minutes, with a countdown.
- Start at sign-in, in the tray.
- Automatic resume on screen lock, sign-out and sleep.
- Light, dark or system appearance; full keyboard and screen-reader support.
- Ten languages: English, Chinese (Simplified), Hindi, Spanish, Arabic (right to left),
  French, Bengali, Portuguese, Russian and Indonesian. Follows Windows or pick one.
- Per-user installer and portable build.
- A BETA tag and "(beta)" in the window title while the version is 0.x.

[Unreleased]: https://github.com/mrlahmar/keyless/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/mrlahmar/keyless/releases/tag/v0.1.0
