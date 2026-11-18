#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod tray_icons;

slint::include_modules!();

fn main() {
    let background = std::env::args().any(|arg| arg == "--background");
    if let Err(message) = app::run(background) {
        keyless_platform::desktop::show_error(&message);
        std::process::exit(1);
    }
}
