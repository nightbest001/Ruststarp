//! Front end: one web page (see `web.rs`) shown in a WebView2 window for every screen.

pub mod web;

use crate::models::LaunchMode;
use web::Mode;

/// Setup wizard (shown when Ruststrap is started from outside its install folder).
pub mod setup {
    pub fn run() -> i32 {
        super::web::run(super::Mode::Setup)
    }
}
/// The launch menu you get when you open the installed app with no arguments.
pub mod home {
    pub fn run() -> i32 {
        super::web::run(super::Mode::Home)
    }
}
/// The settings window.
pub mod settings {
    pub fn run() -> i32 {
        super::web::run(super::Mode::Settings)
    }
}
/// The progress window shown while Roblox is installed / updated / started.
pub mod launch {
    pub fn run(mode: super::LaunchMode) -> i32 {
        super::web::run(super::Mode::Launch(mode))
    }
}
