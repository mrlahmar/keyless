//! "Keyboard paused" as a desktop notification, in place of the paused tag.
//!
//! Wayland doesn't let apps place windows, so the tag would land wherever the desktop puts
//! new windows (GNOME: the middle of the screen). Keyless doesn't place it on X11 either.
//! Notifications appear where the desktop puts them, out of the way, and stay in its
//! notification list with a Resume button.
//!
//! Uses `org.freedesktop.Notifications` on the session bus, which GNOME, KDE, Cinnamon,
//! Xfce, MATE and the notification daemons of tiling setups (mako, dunst) provide. Two
//! threads, both asleep until something happens: one sends, one waits for Resume clicks.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;

use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::message::Type;
use zbus::zvariant::Value;
use zbus::{MatchRule, Message};

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const RESUME: &str = "resume";

enum Command {
    Show { title: String, body: String, resume: Option<String> },
    Hide,
}

pub struct Notice {
    sender: Option<Sender<Command>>,
    worker: Option<JoinHandle<()>>,
}

impl Notice {
    /// `None` without a session bus: the app shows the paused tag instead.
    /// `on_resume` runs on another thread when the Resume button is clicked.
    pub fn start(on_resume: impl Fn() + Send + 'static) -> Option<Self> {
        let connection = Connection::session().ok()?;
        // The notification on screen, 0 for none.
        let shown = Arc::new(AtomicU32::new(0));

        let (sender, receiver) = mpsc::channel();
        let worker = {
            let connection = connection.clone();
            let shown = shown.clone();
            std::thread::Builder::new()
                .name("keyless-notice".into())
                .spawn(move || {
                    let Ok(proxy) = Proxy::new(&connection, SERVICE, PATH, SERVICE) else { return };
                    // Waits for the next command; ends when the app drops the notice.
                    for command in receiver {
                        match command {
                            Command::Show { title, body, resume } => {
                                let id = notify(
                                    &proxy,
                                    shown.load(Ordering::Relaxed),
                                    &title,
                                    &body,
                                    resume.as_deref(),
                                );
                                shown.store(id.unwrap_or(0), Ordering::Relaxed);
                            }
                            Command::Hide => close(&proxy, &shown),
                        }
                    }
                    close(&proxy, &shown);
                })
                .ok()?
        };

        let _ = std::thread::Builder::new().name("keyless-notice-actions".into()).spawn(move || {
            let Ok(messages) =
                action_rule().and_then(|rule| MessageIterator::for_match_rule(rule, &connection, None))
            else {
                return;
            };
            for message in messages.flatten() {
                if is_resume(&message, shown.load(Ordering::Relaxed)) {
                    on_resume();
                }
            }
        });

        Some(Self { sender: Some(sender), worker: Some(worker) })
    }

    /// Shows the notice, or updates the one on screen. `resume`: the Resume button's label,
    /// `None` for no button (while the mouse is paused, it couldn't be clicked).
    pub fn show(&self, title: &str, body: &str, resume: Option<&str>) {
        self.send(Command::Show { title: title.into(), body: body.into(), resume: resume.map(Into::into) });
    }

    pub fn hide(&self) {
        self.send(Command::Hide);
    }

    fn send(&self, command: Command) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(command);
        }
    }
}

impl Drop for Notice {
    /// Takes the notice off the screen before Keyless quits.
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Returns the notification's id.
fn notify(proxy: &Proxy, replaces: u32, title: &str, body: &str, resume: Option<&str>) -> Option<u32> {
    let actions: Vec<&str> = resume.map(|label| vec![RESUME, label]).unwrap_or_default();
    let mut hints: HashMap<&str, Value> = HashMap::new();
    // Ties it to keyless.desktop, for the app name and icon.
    hints.insert("desktop-entry", Value::from(super::app_id()));
    // Normal urgency: GNOME doesn't keep critical ones in its list.
    hints.insert("urgency", Value::U8(1));
    let app_name = keyless_core::APP_NAME;
    // -1: the desktop's usual time on screen, then into its notification list.
    let call: zbus::Result<u32> =
        proxy.call("Notify", &(app_name, replaces, "keyless", title, body, actions, hints, -1i32));
    call.ok()
}

fn close(proxy: &Proxy, shown: &AtomicU32) {
    let id = shown.swap(0, Ordering::Relaxed);
    if id != 0 {
        let _: zbus::Result<()> = proxy.call("CloseNotification", &(id,));
    }
}

fn action_rule() -> zbus::Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(SERVICE)?
        .path(PATH)?
        .member("ActionInvoked")?
        .build())
}

fn is_resume(message: &Message, shown: u32) -> bool {
    let header = message.header();
    if header.member().map(|m| m.as_str()) != Some("ActionInvoked") {
        return false;
    }
    let Ok((id, action)) = message.body().deserialize::<(u32, String)>() else { return false };
    shown != 0 && id == shown && action == RESUME
}
