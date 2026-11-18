# Architecture

```text
crates/
  keyless-core/      Keyboard state machine, shortcut model, settings. No OS, no UI.
  keyless-platform/  Everything that needs the OS, one module per OS:
                       windows/      keyboard hook, layout labels, autostart, single instance, popups
                       unsupported   compiles everywhere else and reports Unsupported
  keyless-app/       Slint UI (ui/*.slint) and the controller that wires it together.
packaging/windows/   Inno Setup installer.
```

## How blocking works (Windows)

A dedicated thread installs a `WH_KEYBOARD_LL` hook and runs its own message loop, so a
busy UI never delays input. For each physical key event the hook asks
`keyless_core::Keyboard` whether to consume it. The state machine pairs every key-down
with its key-up across pause and resume, so no key gets stuck, and keys still held when
you resume stay blocked until released.

A hidden window on the same thread receives screen-lock, sign-out and sleep notifications
(the keyboard resumes) and registers the shortcut with `RegisterHotKey` only to detect
conflicts with other apps. Synthetic input (`LLKHF_INJECTED`) is never blocked.

The pause state is never saved, so Keyless always starts with the keyboard on. If the
process ends, Windows removes the hook, so the keyboard can't stay paused.

winit's raw device events are turned off at startup. While they are on, Windows doesn't
call the hook for keys typed into Keyless's own windows, which broke shortcut recording.

The UI thread talks to the input thread through a channel; events come back through
`slint::invoke_from_event_loop`.

## UI

- `theme.slint`: design tokens (colors, type, radii) and the embedded fonts (Space
  Grotesk for text, JetBrains Mono for hardware labels and keys). Light or dark follows
  the system unless chosen in settings.
- `controls.slint`: accessible controls (ON/PAUSE switch, small switches, auto-resume
  detents, buttons, key caps, tabs).
- `face.slint`: the keyboard face and the app mark, with their blink and sleep animations.
- `keyboard.slint`: the drawn keyboard, also used to set a shortcut by clicking keys.
  Keys are drawn by position (US ANSI shape). The 47 keys whose meaning depends on the
  layout (`LAYOUT_KEYS` in `keyless-core`) take their labels from
  `keyless_platform::layout::key_caps()`, which reads the Windows layout of the app in
  front. That is the layout the hook sees keys through. Shortcuts store the key's meaning
  (virtual key), not its position, so they follow the letter like Windows shortcuts do.
  Windows sends no notice when the layout changes in the taskbar, with Win+Space or in
  another app, so while the main window is open `app.rs` checks the active layout every
  250 ms and redraws only when it changed.
- `app.slint`: main window (Control, Shortcut, Settings, About), tray panel, paused tag
  and tray menu.

Nothing polls. Timers run only while paused with auto-resume on (the countdown), while
recording a shortcut, or while the main window is open (the face blinks, the drawn
keyboard follows layout changes). Animations stop
when Windows' "Show animations" is off.

Translations are gettext `.po` files in `crates/keyless-app/lang/`, compiled into the
executable by `slint-build`. `app.rs` picks the saved language or the first supported
Windows UI language, and sets `Theme.rtl` (Arabic), `Theme.tracking` (letter spacing only
for Latin and Cyrillic) and, for Chinese, a system font that covers Simplified characters.
Slint does not mirror layouts by itself: `Row` is a flexbox row that reverses for
right-to-left, and rows with wrapping text are grids with computed columns.

The paused tag is a separate never-focused window. Dragging it moves the native window;
on drop its top-left corner is saved in physical pixels and restored later if that spot is
still on a screen, otherwise it falls back to the chosen edge.

## Porting

A new OS needs a module in `keyless-platform` providing `Blocker`, `autostart`,
`instance` and `desktop` with the same signatures as `src/windows/`. Expected backends:

- macOS: `CGEventTap`, which requires the user to grant Accessibility permission.
- Linux: exclusive `evdev` grab (`EVIOCGRAB`) on keyboard devices, which needs access to
  `/dev/input` (for example the `input` group). Wayland offers no global alternative.
