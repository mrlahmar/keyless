//! Desktop integration: popup windows, links, system preferences.
//!
//! Wayland doesn't let apps place their windows, keep them on top or show them without
//! focus, and X11 window managers differ in how they handle it. So popups are plain
//! windows placed by the desktop for now: the functions that can't do their job say so,
//! and the app falls back to normal behavior.

use std::process::{Command, Stdio};

use raw_window_handle::RawWindowHandle;

use crate::{Anchor, PopupKind};

pub const SUPER_KEY_NAME: &str = "Super";

/// Neither GNOME's nor KDE's animation setting is read yet.
pub fn animations_enabled() -> bool {
    true
}

/// Without it, Simplified Chinese may fall back to a Japanese font that lacks many
/// Simplified characters. Fontconfig picks another font when this one is missing.
pub fn font_for_language(language: &str) -> Option<&'static str> {
    match language {
        "zh" => Some("Noto Sans CJK SC"),
        _ => None,
    }
}

/// Runs a desktop helper without waiting for it, and reaps it when it exits.
fn spawn(program: &str, args: &[&str]) -> bool {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match child {
        Ok(mut child) => {
            let _ = std::thread::Builder::new().spawn(move || child.wait());
            true
        }
        Err(_) => false,
    }
}

/// Opens a web page or file with the desktop's default app.
pub fn open_url(url: &str) {
    spawn("xdg-open", &[url]);
}

/// Error dialog, for failures that happen before any window exists. Uses the desktop's
/// dialog tool when there is one; the message always goes to stderr too.
pub fn show_error(message: &str) {
    let title = keyless_core::APP_NAME;
    eprintln!("{title}: {message}");
    let shown = Command::new("zenity")
        .args(["--error", "--title", title, "--text", message])
        .stderr(Stdio::null())
        .status()
        .is_ok()
        || Command::new("kdialog").args(["--title", title, "--error", message]).status().is_ok();
    if !shown {
        spawn("notify-send", &["--urgency=critical", title, message]);
    }
}

pub fn style_popup(_window: RawWindowHandle, _kind: PopupKind) {}

/// [`set_visible_passive`] doesn't work here: the app shows and hides popups normally.
pub const PASSIVE_POPUPS: bool = false;

/// [`place`] and [`fit`] don't work here: the window manager moves windows (Wayland doesn't
/// let apps place them at all), so the app asks it to when the paused tag is dragged, and
/// the tray opens the main window rather than a panel that can't sit next to it.
pub const PLACES_WINDOWS: bool = false;

/// The main window draws its own title bar. The one winit draws on Wayland always offers
/// maximize, which a fixed-size window can't honor.
pub const CUSTOM_TITLE_BAR: bool = true;

pub fn set_visible_passive(_window: RawWindowHandle, _visible: bool) {}

/// Desktops give a newly shown window focus by themselves.
pub fn bring_to_front(_window: RawWindowHandle) {}

/// `None`: the desktop places the window.
pub fn place(_anchor: Anchor, _logical_size: (f32, f32)) -> Option<(i32, i32)> {
    None
}

pub fn fit(_spot: (i32, i32), _logical_size: (f32, f32)) -> Option<(i32, i32)> {
    None
}

const TRAY_HOST: &str = "org.kde.StatusNotifierWatcher";

/// Whether the desktop can show tray icons yet.
///
/// At sign-in Keyless can start before the desktop's tray host (on GNOME, the AppIndicator
/// extension, which loads after autostart apps), and a tray icon created then never
/// appears: Slint tries once and gives up. When this returns `false`, `on_ready` runs once,
/// on another thread, as soon as the host is there, and the app creates the icon again.
/// Without a session bus it returns `true`: there is nothing to wait for.
pub fn tray_host_ready(on_ready: impl FnOnce() + Send + 'static) -> bool {
    use zbus::MatchRule;
    use zbus::blocking::{Connection, MessageIterator};
    use zbus::message::Type;

    let Ok(connection) = Connection::session() else { return true };
    let rule = || -> zbus::Result<MatchRule<'static>> {
        Ok(MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .add_arg(TRAY_HOST)?
            .build())
    };
    // Listen first, then ask, so a host that arrives in between isn't missed.
    let Ok(messages) = rule().and_then(|rule| MessageIterator::for_match_rule(rule, &connection, None))
    else {
        return true;
    };
    let has_owner = connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &(TRAY_HOST,),
        )
        .and_then(|reply| reply.body().deserialize::<bool>());
    if has_owner.unwrap_or(true) {
        return true;
    }
    // Sleeps until the host appears. Without one (GNOME without the extension) it waits
    // for good, at no cost.
    let _ = std::thread::Builder::new().name("keyless-tray-host".into()).spawn(move || {
        let _connection = connection;
        for message in messages.flatten() {
            let owner = message.body().deserialize::<(String, String, String)>();
            if owner.is_ok_and(|(_, _, new_owner)| !new_owner.is_empty()) {
                on_ready();
                return;
            }
        }
    });
    false
}
