//! Screen lock, sleep and user switching, from systemd-logind on the system bus.
//!
//! - Sleep: `PrepareForSleep(true)` on the manager.
//! - Screen lock: the session's `LockedHint` turns true (GNOME, KDE and most lock screens
//!   set it), or logind asks the session to `Lock`.
//! - Another user at the screen: the session's `Active` turns false.
//! - Logout ends the process, which releases the keyboard by itself.
//!
//! Runs on its own thread that sleeps until logind sends something. Without logind (or
//! without a system bus) Keyless still works, it just doesn't resume on these events.

use std::collections::HashMap;

use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::message::Type;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{MatchRule, Message};

const LOGIND: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionEvent {
    /// Screen lock or sleep: resume.
    Interrupt,
    /// The session came to the front (`true`) or another one did (`false`).
    Active(bool),
}

pub fn watch(on_event: impl Fn(SessionEvent) + Send + 'static) {
    let _ = std::thread::Builder::new().name("keyless-session".into()).spawn(move || {
        if let Err(e) = run(&on_event) {
            eprintln!("{}: not watching screen lock and sleep: {e}", keyless_core::APP_NAME);
        }
    });
}

fn run(on_event: &dyn Fn(SessionEvent)) -> zbus::Result<()> {
    let connection = Connection::system()?;
    let manager = Proxy::new(&connection, LOGIND, MANAGER_PATH, "org.freedesktop.login1.Manager")?;
    // "auto": the session of this process, or else the user's graphical session (apps
    // started at sign-in often run outside the session itself).
    let session: OwnedObjectPath = manager.call("GetSession", &("auto",))?;
    let rule = MatchRule::builder().msg_type(Type::Signal).path_namespace(MANAGER_PATH)?.build();
    for message in MessageIterator::for_match_rule(rule, &connection, None)? {
        if let Some(event) = message.ok().and_then(|m| event(&m, &session)) {
            on_event(event);
        }
    }
    Ok(())
}

fn event(message: &Message, session: &OwnedObjectPath) -> Option<SessionEvent> {
    let header = message.header();
    let path = header.path()?.as_str();
    match header.member()?.as_str() {
        "PrepareForSleep" if path == MANAGER_PATH => {
            let (sleeping,): (bool,) = message.body().deserialize().ok()?;
            sleeping.then_some(SessionEvent::Interrupt)
        }
        "Lock" if path == session.as_str() => Some(SessionEvent::Interrupt),
        "PropertiesChanged" if path == session.as_str() => {
            let (_, changed, _): (String, HashMap<String, OwnedValue>, Vec<String>) =
                message.body().deserialize().ok()?;
            let flag = |name: &str| changed.get(name).and_then(|v| bool::try_from(v).ok());
            if flag("Active") == Some(false) {
                Some(SessionEvent::Active(false))
            } else if flag("LockedHint") == Some(true) {
                Some(SessionEvent::Interrupt)
            } else if flag("Active") == Some(true) {
                Some(SessionEvent::Active(true))
            } else {
                None
            }
        }
        _ => None,
    }
}
