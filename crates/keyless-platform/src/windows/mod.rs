//! Windows 10 1809+ backend.

mod blocker;

pub mod autostart;
pub mod desktop;
pub mod instance;
pub mod layout;

pub use blocker::Blocker;

pub mod notice {
    /// Windows shows the paused tag instead.
    pub struct Notice(());

    impl Notice {
        pub fn start(_on_resume: impl Fn() + Send + 'static) -> Option<Self> {
            None
        }
        pub fn show(&self, _title: &str, _body: &str, _resume: Option<&str>) {}
        pub fn hide(&self) {}
    }
}
