//! Ruststrap: a Rust port of Bloxstrap (https://github.com/bloxstraplabs/bloxstrap).
//!
//! Differences from the original: no WPF/WinForms UI (everything is CLI and log based), no Discord
//! Rich Presence, no custom bootstrapper themes, no in-app updater. The launch/update/modification
//! pipeline, FastFlags, activity tracking and the roblox:// protocol handlers are ported.

#[cfg(not(windows))]
compile_error!("Ruststrap only supports Windows.");

mod activity_watcher;
mod app;
mod bootstrapper;
mod deployment;
mod fast_flags;
mod installer;
mod json_manager;
mod launch_settings;
mod logger;
mod models;
mod paths;
mod registry;
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
  {exe} -menu                       print where settings and logs live

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
    let args: Vec<String> = std::env::args().skip(1).collect();

    paths::init(paths::default_base());
    let is_watcher = args.iter().any(|a| a == "-watcher");
    logger::initialize(&paths::get().logs, if is_watcher { "Watcher" } else { "Ruststrap" });
    crate::log!("App::Main", "Ruststrap {} starting", app::VERSION);
    app::load_all();

    // plain-word subcommands
    match args.first().map(String::as_str) {
        Some("flags") => return fast_flags::cli(&args[1..]),
        Some("install") => return installer::install(args.iter().any(|a| a == "-quiet")),
        Some("uninstall") => {
            return installer::uninstall(args.iter().any(|a| a == "-quiet"), args.iter().any(|a| a == "-keepdata"));
        }
        _ => {}
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
        let p = paths::get();
        println!("Settings : {}", p.base.join("Settings.json").display());
        println!("FastFlags: {}", p.modifications.join("ClientSettings").join("ClientAppSettings.json").display());
        println!("Mods     : {}", p.modifications.display());
        println!("Logs     : {}", p.logs.display());
        return 0;
    }

    let mode = launch.roblox_launch_mode;
    if mode == LaunchMode::None {
        print_help();
        return 0;
    }
    bootstrapper::Bootstrapper::new(mode).run()
}
