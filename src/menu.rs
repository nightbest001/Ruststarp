//! Text menu shown when the exe is opened by double-click (no arguments), so the window
//! stays open instead of printing help and closing.

use std::io::{self, Write};

use crate::app::{self, PROJECT_NAME};
use crate::bootstrapper::Bootstrapper;
use crate::launch_settings::LaunchSettings;
use crate::models::LaunchMode;
use crate::{gpu, installer, paths};

/// `None` when stdin is closed.
fn ask(prompt: &str) -> Option<String> {
    print!("{prompt}");
    let _ = io::stdout().flush();
    let mut s = String::new();
    match io::stdin().read_line(&mut s) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(s.trim().to_string()),
    }
}

fn pause() {
    let _ = ask("\nPress Enter to continue...");
}

fn show_locations() {
    let p = paths::get();
    println!("Settings : {}", p.base.join("Settings.json").display());
    println!("FastFlags: {}", p.modifications.join("ClientSettings").join("ClientAppSettings.json").display());
    println!("Mods     : {}", p.modifications.display());
    println!("Logs     : {}", p.logs.display());
}

pub fn run() -> i32 {
    app::set_launch(LaunchSettings::parse(vec!["-player".to_string()]));
    loop {
        println!("\n=== {PROJECT_NAME} {} ===", app::VERSION);
        if !paths::get().application.exists() {
            println!("(not installed yet, pick 2 first)");
        }
        println!("  1) Play Roblox");
        println!("  2) Install / repair {PROJECT_NAME}");
        println!("  3) GPU: show GPUs and choose one for Roblox");
        println!("  4) Show settings and log folders");
        println!("  5) Uninstall");
        println!("  0) Exit");
        let Some(choice) = ask("> ") else { return 0 };
        match choice.as_str() {
            "1" => {
                let code = Bootstrapper::new(LaunchMode::Player).run();
                if code == 0 {
                    return 0;
                }
                pause();
            }
            "2" => {
                installer::install(false);
                pause();
            }
            "3" => {
                gpu::cli(&[]);
                if let Some(v) = ask("Set GPU for Roblox [high / saving / default, Enter = skip]: ") {
                    if !v.is_empty() {
                        gpu::cli(&["set".to_string(), v]);
                    }
                }
                pause();
            }
            "4" => {
                show_locations();
                pause();
            }
            "5" => {
                if installer::uninstall(false, false) == 0 {
                    return 0;
                }
                pause();
            }
            "0" | "q" | "exit" => return 0,
            _ => println!("Pick a number from the list."),
        }
    }
}
