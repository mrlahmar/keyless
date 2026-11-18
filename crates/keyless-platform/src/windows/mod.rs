//! Windows 10 1809+ backend.

mod blocker;

pub mod autostart;
pub mod desktop;
pub mod instance;
pub mod layout;

pub use blocker::Blocker;
