//! `anyxboxx-launcher`: the graphical launcher a player double-clicks. It checks the setup,
//! verifies the game folder, preloads VIctory's shader cache, picks a profile for the host and the
//! GPU, and starts the game through the same steps as `anyxboxx run`.
//!
//! GUI library: egui through eframe (pure Rust, no C++ binding, no installer). The window is a
//! handful of pages with a log and progress bars, which immediate-mode egui draws in a few hundred
//! lines, and it renders through the OS's own graphics API. The alternative, raw Win32 through the
//! `windows` crate, would have meant hand-written message loops and control layout for the same
//! pages, for no gain in what the launcher does. The logic stays in plain modules that the window
//! only calls, so the tests never need a window.

mod app;
mod cache;
mod env;
mod format;
mod host;
mod job;
mod launch;
mod processes;
mod profile;
mod progress;
mod session;
mod setup;
mod ui;
mod ui_pages;
mod verify;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AnyXboxX launcher")
            .with_inner_size([980.0, 720.0])
            .with_min_inner_size([640.0, 480.0]),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "AnyXboxX launcher",
        options,
        Box::new(|_creation| {
            let state = app::App::new()?;
            Ok(Box::new(state))
        }),
    )
}
