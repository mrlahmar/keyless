# Architecture

```text
crates/
  keyless-core/      Keyboard state machine, shortcut model, settings. No OS, no UI.
  keyless-platform/  Everything that needs the OS, one module per OS:
                       windows/      keyboard hook, layout labels, autostart, single instance, popups
                       linux/        evdev grab + uinput, logind, XDG autostart, single instance
                       unsupported   compiles everywhere else and reports Unsupported
  keyless-app/       Slint UI (ui/*.slint) and the controller that wires it together.
packaging/windows/   Inno Setup installer.
packaging/linux/     Desktop entry and udev rule; scripts/package-linux.sh builds .deb and AppImage.
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

Pausing the mouse too is optional. Only then, and only while paused, the same thread adds
a `WH_MOUSE_LL` hook; `keyless_core::Mouse` decides per event (clicks and wheel, plus
movement to freeze the cursor) and pairs every blocked button press with its release.
The hook is removed on resume, so the mouse costs nothing otherwise. Mouse pausing needs
a working shortcut, because the shortcut is then the only way back, and the shortcut
always resumes while it is on.

The pause state is never saved, so Keyless always starts with the keyboard on. If the
process ends, Windows removes the hook, so the keyboard can't stay paused.

winit's raw device events are turned off at startup. While they are on, Windows doesn't
call the hook for keys typed into Keyless's own windows, which broke shortcut recording.

The UI thread talks to the input thread through a channel; events come back through
`slint::invoke_from_event_loop`.

## How blocking works (Linux)

Linux has no global keyboard hook, and Wayland lets no app see other apps' input. Below
the display server, each keyboard is a device in `/dev/input`. A dedicated thread grabs
every physical keyboard with `EVIOCGRAB`, so only Keyless reads it, and creates a virtual
copy of it with uinput. Each event goes through `keyless_core::Keyboard`; the ones that
pass are written to the copy, so the desktop sees them as typed on the copy. The thread
sleeps in `poll()` on the devices, a wake-up socket for commands, and an inotify watch on
`/dev/input` for keyboards plugged in later.

Keyboards stay grabbed for as long as Keyless runs, because the desktop ignores a key
release from a device that never sent the press: grabbing only while paused would leave
held keys stuck. For the same reason a device is grabbed only when none of its keys are
down, and the copy always gets the release of every key it pressed. Mice and touchpads
are opened and grabbed only while the mouse is paused (and until blocked buttons are
released); `keyless_core::Mouse` decides per event as on Windows.

Devices under `/sys/devices/virtual` (uinput, including Keyless's own copies) are never
grabbed: software-sent input passes. If the process ends, the kernel ends the grabs and
removes the copies, releasing whatever they held.

A second thread listens to systemd-logind on the system bus: `PrepareForSleep` and the
session's `LockedHint` resume the keyboard; when the session stops being `Active`
(another user at the screen), Keyless lets go of every device until it is back.

There is no hotkey registry, so shortcut conflicts aren't detected: Keyless consumes the
shortcut before the desktop sees it. Access to `/dev/input` and `/dev/uinput` comes from
a udev rule with `uaccess` (`packaging/linux/70-keyless.rules`). See
[LINUX.md](LINUX.md) for what differs from Windows.

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
- `mouse.slint`: the drawn mouse on the Mouse tab, acting out the chosen mode.
- `app.slint`: main window (Control, Shortcut, Mouse, Settings, About), the "Are you
  sure?" dialog, tray panel, paused tag and tray menu.

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
- Linux: done, see above.
