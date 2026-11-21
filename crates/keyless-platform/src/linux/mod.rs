//! Linux backend: X11 and Wayland alike, since it works below the display server.

mod blocker;
mod device;
mod keys;
mod session;

pub mod autostart;
pub mod desktop;
pub mod instance;
pub mod layout;
pub mod notice;

pub use blocker::Blocker;

/// File name for this app's files: `keyless`, or `keyless-dev` for debug builds.
fn app_id() -> String {
    keyless_core::APP_NAME.to_lowercase().replace(' ', "-")
}
