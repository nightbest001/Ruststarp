//! Ruststrap: a Rust port of Bloxstrap (https://github.com/bloxstraplabs/bloxstrap).
//!
//! The window UI (settings, launch menu, launch progress, setup) is one HTML/CSS/JS page shown in a WebView2
//! window, so layout, icons and animation match Bloxstrap's WPF-UI. It is a GUI-subsystem program: no terminal opens on double-click or from a roblox:// link. When it
//! is started from cmd/PowerShell with a subcommand, it borrows that console for the text output.
//!
//! Differences from the original: no Discord Rich Presence (removed), no bootstrapper styles / languages, no in-app updater.
//! The launch/update/modification pipeline, FastFlags, activity tracking and the roblox:// protocol handlers
//! are ported.

// No console window. `console::attach_parent` gives the text subcommands their output back.
#![windows_subsystem = "windows"]

// Some ported helpers are kept for parity with Bloxstrap but not used yet.
#![allow(dead_code)]

#[cfg(not(windows))]
compile_error!("Ruststrap only supports Windows.");

mod activity_watcher;
mod app;
mod bootstrapper;
mod console;
mod deployment;
mod fast_flags;
mod gpu;
mod installer;
mod json_manager;
mod launch_settings;
mod logger;
mod menu;
mod models;
mod paths;
mod progress;
mod diagnostics;
mod registry;
mod shortcuts;
mod ui;
mod watcher;

use launch_settings::LaunchSettings;
use models::LaunchMode;

fn print_help() {
    println!(
        "{name} {ver}: alternative Roblox bootstrapper (Rust port of Bloxstrap)

USAGE
  {exe} install [-quiet]            install to %LOCALAPPDATA%\\{name} and register roblox:// links
  {exe} uninstall [-quiet]          remove {name} and Roblox
  {exe} -player [roblox-player:..]  launch (or update) Roblox
  {exe} -studio [file|uri]          launch (or update) Roblox Studio
  {exe} roblox-player:...           what the browser passes; same as -player
  {exe} flags <list|get|set|remove|preset>   edit FastFlags
  {exe} gpu [list|set <high|saving|default>]   show GPUs, pick GPU for Roblox
  {exe}                             open the settings window
  {exe} -settings                   open the settings window

OPTIONS
  -channel <name>   deployment channel (default production)
  -version <guid>   install a specific version-xxxxxxxx
  -force            reinstall even if up to date
  -nolaunch         install/update only
  -quiet            fewer prompts
",
        name = app::PROJECT_NAME,
        ver = app::VERSION,
        exe = "ruststrap"
    );
}

fn main() {
    std::process::exit(real_main());
}

fn real_main() -> i32 {
    console::attach_parent();
    let args: Vec<String> = std::env::args().skip(1).collect();

    paths::init(paths::default_base());
    let is_watcher = args.iter().any(|a| a == "-watcher");
    logger::initialize(&paths::get().logs, if is_watcher { "Watcher" } else { "Ruststrap" });
    crate::log!("App::Main", "Ruststrap {} starting", app::VERSION);
    app::load_all();

    // plain-word subcommands
    match args.first().map(String::as_str) {
        Some("flags") => return fast_flags::cli(&args[1..]),
        Some("gpu") => return gpu::cli(&args[1..]),
        Some("install") => {
            let code = installer::install(args.iter().any(|a| a == "-quiet"));
            if code == app::error_code::SUCCESS {
                if args.iter().any(|a| a == "-desktop") {
                    let _ = shortcuts::set(shortcuts::Kind::Desktop, true);
                }
                if args.iter().any(|a| a == "-startmenu") {
                    let _ = shortcuts::set(shortcuts::Kind::StartMenu, true);
                }
            }
            return code;
        }
        Some("uninstall") => {
            return installer::uninstall(args.iter().any(|a| a == "-quiet"), args.iter().any(|a| a == "-keepdata"));
        }
        _ => {}
    }

    // double-click: the setup wizard until Ruststrap is installed, then the settings window
    if args.is_empty() {
        if !paths::running_installed_copy() {
            return ui::setup::run();
        }
        return ui::home::run();
    }

    app::set_launch(LaunchSettings::parse(args));
    let launch = app::launch();

    if launch.active("help") || launch.args.is_empty() {
        print_help();
        return 0;
    }
    if launch.active("uninstall") {
        return installer::uninstall(launch.active("quiet"), false);
    }
    if launch.active("install") {
        return installer::install(launch.active("quiet"));
    }
    if launch.active("watcher") {
        return watcher::run();
    }
    if launch.active("menu") {
        return ui::settings::run();
    }

    let mode = launch.roblox_launch_mode;
    if mode == LaunchMode::None {
        print_help();
        return 0;
    }
    ui::launch::run(mode)
}
