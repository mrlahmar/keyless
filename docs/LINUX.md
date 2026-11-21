# Keyless on Linux

Keyless works on X11 and Wayland alike, on any desktop, because it pauses the keyboard
below the display server.

## Install

Download from [Releases](https://github.com/mrlahmar/keyless/releases):

- `keyless_x.y.z_amd64.deb`: Debian, Ubuntu, Linux Mint, Pop!_OS, elementary OS and
  others. `sudo apt install ./keyless_x.y.z_amd64.deb`. Ready to use right away.
- `Keyless-x.y.z-x86_64.AppImage`: any distro. Make it executable and run it. It needs
  the udev rule below once.

Needs x86_64, glibc 2.36 or newer (Debian 12, Ubuntu 22.10, Fedora 37 and newer), and
systemd (for the udev rule and for resuming on screen lock and sleep).

### The udev rule

Keyless reads keyboards in `/dev/input` and passes allowed keys on through a virtual
keyboard (`/dev/uinput`). By default only root can open these. The `.deb` installs
[`70-keyless.rules`](../packaging/linux/70-keyless.rules), which gives the user at the
screen access while they are at the screen, the same way systemd handles game
controllers. For the AppImage, or a build from source, install it once:

```sh
sudo install -m644 packaging/linux/70-keyless.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=input --subsystem-match=misc
```

No sign-out needed. The rule is also inside the AppImage, in `usr/share/keyless/`.

Be aware: with this access, any program you run can read what you type. That is true of
every tool of this kind (key remappers, macro tools). Without systemd, add yourself to
the `input` group instead and give that group `/dev/uinput`.

### Tray icon

The tray uses StatusNotifierItem. KDE, Cinnamon, MATE, Budgie, Xfce (with the status
notifier plugin) and Ubuntu's GNOME show it. On other GNOME setups, install the
"AppIndicator and KStatusNotifierItem Support" extension. Without a tray, open Keyless
from the app menu.

## What is different from Windows

- The shortcut follows key positions on a US keyboard: Ctrl + Alt + K is the key right
  of J, whatever your layout prints on it. The drawn keyboard shows US labels.
- Wayland doesn't let apps place windows, so there is no small panel by the tray icon:
  clicking the icon opens the Keyless window (on GNOME: double-click; a single click opens
  the menu). Instead of the paused tag, a notification says the keyboard is paused, with a
  Resume button while the mouse works. It stays in the desktop's notification list until
  you resume. X11 works the same way. Settings calls it "Notification".
- The Keyless window draws its own title bar, with minimize and close only: it has a
  fixed size, and the desktop's title bar would offer to maximize it.
- With "Pause the mouse" on clicks only, tapping a touchpad still clicks; physical
  touchpad buttons are paused. The pointer mode pauses touchpads fully.
- Keyless doesn't detect shortcut conflicts: it sees keys before the desktop, so its
  shortcut always wins.
- The system setting for animations isn't read yet.
- Keys sent by software (on-screen keyboards, `ydotool`, remappers like `keyd`) are never
  paused. If you use a remapper that grabs your keyboard, Keyless can't pause it yet.

## How it works

See the comment at the top of
[`crates/keyless-platform/src/linux/blocker.rs`](../crates/keyless-platform/src/linux/blocker.rs)
and [ARCHITECTURE.md](ARCHITECTURE.md). In short: Keyless grabs each physical keyboard
(`EVIOCGRAB`), so only Keyless gets its events, and replays the allowed ones through a
virtual copy of that keyboard. Mice are grabbed only while the mouse is paused. logind
tells Keyless about screen lock, sleep and user switches. If Keyless quits or crashes,
the kernel hands the keyboard back.

## Build and run from source

```sh
# Debian/Ubuntu. Fedora: sudo dnf install gcc pkgconf fontconfig-devel
sudo apt install build-essential pkg-config libfontconfig1-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

cargo test --workspace
cargo run                      # settings window + tray
cargo run -- --background      # tray only, as at sign-in
```

Install the udev rule first (above). Debug builds run as "Keyless Dev" with Ctrl + Alt + J.
For their name and icon in the dock, install a desktop entry once:

```sh
sed -e 's/^Name=.*/Name=Keyless Dev/' -e "s|^Exec=.*|Exec=$PWD/target/debug/keyless|" \
    -e "s|^Icon=.*|Icon=$PWD/assets/keyless.svg|" -e 's/^StartupWMClass=.*/StartupWMClass=keyless-dev/' \
    packaging/linux/keyless.desktop > ~/.local/share/applications/keyless-dev.desktop
```

Package locally (`dpkg-deb` needed; set `APPIMAGETOOL` to an
[appimagetool](https://github.com/AppImage/appimagetool/releases) path to also build the
AppImage):

```sh
sh scripts/package-linux.sh
```

The packages include AppStream metadata for software centers, from
`packaging/linux/io.github.mrlahmar.Keyless.metainfo.xml`. The script adds one release
per version in `CHANGELOG.md` and checks the result with `appstreamcli`, if installed.

## Test checklist

Use a real keyboard and mouse. Keys sent by software are let through on purpose, so
`xdotool` or `ydotool` can't test pausing. A VM works if it passes a USB keyboard
through, or with its emulated keyboard (that is a real device to the guest). WSL can't:
it has no `/dev/input`.

Check on GNOME (Wayland), KDE Plasma (Wayland) and one X11 session if you can.

1. Start Keyless by pressing Enter in a terminal. Enter must not repeat.
2. Type in a text editor: every key, Shift, AltGr characters, dead keys, key repeat, Caps
   Lock (and its light), Num Lock. Nothing may be lost, doubled or delayed.
3. Press Ctrl + Alt + J (dev build): typing stops, the tray icon changes, and a
   notification appears. The mouse still clicks,
   scrolls and moves. The notification's Resume button resumes.
4. Hold Shift, pause with the mouse (tray menu), release Shift: no stuck Shift after
   resuming. Hold a letter while resuming: it must not start typing.
5. Pause, then lock the screen (Super + L): the keyboard works on the lock screen.
   Same with suspend and resume.
6. Unplug and replug a USB keyboard, while running and while paused.
7. Mouse tab: "Clicks", pause, check clicks and wheel are blocked and the pointer moves;
   "Pointer": the pointer freezes too. Resume with the shortcut; hold a button while
   resuming and release it: no stuck drag.
8. Record a new shortcut on the Shortcut tab, by typing and by clicking keys.
9. Start a second copy: the first one opens its window.
10. Settings: start at sign-in, sign out and in: Keyless is in the tray.
11. Switch to another user (if you can): their keyboard works; switch back: pausing works.
12. Remove the udev rule and start Keyless: a clear error, and the keyboard keeps working.
