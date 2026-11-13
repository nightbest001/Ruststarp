//! Port of `Installer.cs` (install = copy the exe + register protocols; uninstall = reverse it).

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::os::windows::process::CommandExt;
use std::process::Command;

use crate::app::{self, error_code, PROJECT_NAME};
use crate::{paths, registry, shortcuts};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// True when a terminal is attached (`ruststrap uninstall` typed in cmd). Apps & Features starts the exe without one.
fn has_console() -> bool {
    io::stdin().is_terminal()
}

fn confirm(question: &str) -> bool {
    if has_console() {
        print!("{question} [y/N] ");
        let _ = io::stdout().flush();
        let mut s = String::new();
        io::stdin().read_line(&mut s).is_ok() && matches!(s.trim().to_lowercase().as_str(), "y" | "yes")
    } else {
        rfd::MessageDialog::new()
            .set_title(PROJECT_NAME)
            .set_description(question)
            .set_level(rfd::MessageLevel::Warning)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            == rfd::MessageDialogResult::Yes
    }
}

fn notify(msg: &str) {
    if has_console() {
        println!("{msg}");
    } else {
        let _ = rfd::MessageDialog::new()
            .set_title(PROJECT_NAME)
            .set_description(msg)
            .set_level(rfd::MessageLevel::Info)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

pub fn install(quiet: bool) -> i32 {
    const IDENT: &str = "Installer::DoInstall";
    let p = paths::get();
    if let Err(e) = fs::create_dir_all(&p.base) {
        eprintln!("cannot create {}: {e}", p.base.display());
        return error_code::INSTALL_FAILURE;
    }

    let me = paths::process();
    if me != p.application {
        crate::log!(IDENT, "Copying {} to {}", me.display(), p.application.display());
        if let Err(e) = fs::copy(&me, &p.application) {
            eprintln!("cannot copy the executable: {e}");
            return error_code::INSTALL_FAILURE;
        }
    }

    let app_path = p.application.to_string_lossy().to_string();
    let base = p.base.to_string_lossy().to_string();
    if let Err(e) = registry::write_uninstall_entry(&app_path, &base).and_then(|_| registry::register_all(&app_path)) {
        eprintln!("{PROJECT_NAME} was unable to write to the registry: {e}");
        return error_code::INSTALL_FAILURE;
    }

    app::settings().save();
    app::state().save();
    if !quiet {
        println!("{PROJECT_NAME} installed to {base}");
        println!("roblox:// and roblox-player:// links now open through {PROJECT_NAME}.");
    }
    error_code::SUCCESS
}

pub fn uninstall(quiet: bool, keep_data: bool) -> i32 {
    const IDENT: &str = "Installer::DoUninstall";
    let p = paths::get();

    if !quiet && !confirm(&format!("Uninstall {PROJECT_NAME}? Roblox will be removed too.")) {
        return error_code::INSTALL_USER_EXIT;
    }

    for exe in ["RobloxPlayerBeta.exe", "RobloxStudioBeta.exe"] {
        let _ = Command::new("taskkill").args(["/F", "/IM", exe]).creation_flags(CREATE_NO_WINDOW).output();
    }

    let _ = shortcuts::apply(&[false; 5]);

    let (player_left, studio_left) = registry::restore_official_handlers();
    crate::log!(IDENT, "Official player left: {player_left}, studio left: {studio_left}");

    let mut targets = vec![p.versions.clone(), p.downloads.clone()];
    let mut files = vec![p.base.join("State.json"), p.base.join("PlayerState.json"), p.base.join("StudioState.json")];
    if !keep_data {
        targets.push(p.modifications.clone());
        targets.push(p.logs.clone());
        files.push(p.base.join("Settings.json"));
    }
    for d in &targets {
        if let Err(e) = fs::remove_dir_all(d) {
            crate::log!(IDENT, "Failed to remove {}: {e}", d.display());
        }
    }
    for f in &files {
        let _ = fs::remove_file(f);
    }
    registry::delete_uninstall_entry();
    registry::clear_install_location();

    // the running exe cannot delete itself: let cmd.exe do it after we exit
    let robloxless = !player_left && !studio_left;
    let base = p.base.to_string_lossy().to_string();
    // WebView2 keeps files in the folder for a moment after exit, so try twice
    let mut tail = format!("rmdir /s /q \"{base}\" & ping 127.0.0.1 -n 4 >nul & rmdir /s /q \"{base}\"");
    if robloxless {
        tail.push_str(&format!(" & rmdir /s /q \"{}\"", paths::local_app_data().join("Roblox").display()));
    }
    let _ = Command::new("cmd")
        .raw_arg(format!("/c ping 127.0.0.1 -n 3 >nul & {tail}"))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();

    if !quiet {
        notify(&format!("{PROJECT_NAME} has been uninstalled."));
    }
    error_code::SUCCESS
}

/// After a self-update (or a manual exe swap) the registry still names the old version: bring it up to date.
pub fn refresh_registration() {
    if !paths::running_installed_copy() || registry::installed_version().as_deref() == Some(app::VERSION) {
        return;
    }
    let p = paths::get();
    let app_path = p.application.to_string_lossy().to_string();
    let base = p.base.to_string_lossy().to_string();
    if let Err(e) = registry::write_uninstall_entry(&app_path, &base).and_then(|_| registry::register_all(&app_path)) {
        crate::log!("Installer::Refresh", "Could not refresh the registry: {e}");
    }
}
