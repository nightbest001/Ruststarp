//! Port of `Utility/WindowsRegistry.cs` plus the registry bits of the installer.

use std::io;

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

use crate::app::UNINSTALL_KEY;

const ROBLOX_PLACE_KEY: &str = "Roblox.Place";
const WEBVIEW2_CLIENT: &str = r"{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

fn hkcu() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

pub fn webview2_installed() -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(format!(r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}"))
        .is_ok();
    let user = hkcu().open_subkey(format!(r"Software\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}")).is_ok();
    hklm || user
}

pub fn set_estimated_size(kb: i64) {
    if let Ok((k, _)) = hkcu().create_subkey(UNINSTALL_KEY) {
        let _ = k.set_value("EstimatedSize", &(kb.max(0) as u32));
    }
}

pub fn register_protocol(key: &str, name: &str, handler: &str, param: &str) -> io::Result<()> {
    let args = format!("\"{handler}\" {param}");
    let (uri, _) = hkcu().create_subkey(format!(r"Software\Classes\{key}"))?;
    let (icon, _) = uri.create_subkey("DefaultIcon")?;
    let (cmd, _) = uri.create_subkey(r"shell\open\command")?;

    if uri.get_value::<String, _>("").is_err() {
        uri.set_value("", &format!("URL: {name} Protocol"))?;
        uri.set_value("URL Protocol", &"")?;
    }
    if cmd.get_value::<String, _>("").ok().as_deref() != Some(args.as_str()) {
        icon.set_value("", &handler)?;
        cmd.set_value("", &args)?;
    }
    Ok(())
}

pub fn register_player(handler: &str, param: &str) -> io::Result<()> {
    register_protocol("roblox", "Roblox", handler, param)?;
    register_protocol("roblox-player", "Roblox", handler, param)
}

pub fn register_studio_protocol(handler: &str, param: &str) -> io::Result<()> {
    register_protocol("roblox-studio", "Roblox", handler, param)?;
    register_protocol("roblox-studio-auth", "Roblox", handler, param)
}

pub fn register_studio_file_class(handler: &str, param: &str) -> io::Result<()> {
    let value = "Roblox Place";
    let args = format!("\"{handler}\" {param}");
    let (key, _) = hkcu().create_subkey(format!(r"Software\Classes\{ROBLOX_PLACE_KEY}"))?;
    let (icon, _) = key.create_subkey("DefaultIcon")?;
    let (open, _) = key.create_subkey(r"shell\Open")?;
    let (cmd, _) = open.create_subkey("command")?;
    key.set_value("", &value)?;
    cmd.set_value("", &args)?;
    open.set_value("", &"Open")?;
    icon.set_value("", &format!("{handler},0"))?;
    Ok(())
}

pub fn register_studio_file_types() -> io::Result<()> {
    for ext in [".rbxl", ".rbxlx"] {
        let (key, _) = hkcu().create_subkey(format!(r"Software\Classes\{ext}"))?;
        key.create_subkey(format!(r"{ROBLOX_PLACE_KEY}\ShellNew"))?;
        key.set_value("", &ROBLOX_PLACE_KEY)?;
    }
    Ok(())
}

pub fn register_all(app_path: &str) -> io::Result<()> {
    register_player(app_path, "-player \"%1\"")?;
    register_studio_protocol(app_path, "-studio \"%1\"")?;
    register_studio_file_class(app_path, "-studio \"%1\"")?;
    register_studio_file_types()
}

pub fn unregister(key: &str) {
    if let Err(e) = hkcu().delete_subkey_all(format!(r"Software\Classes\{key}")) {
        crate::log!("Protocol::Unregister", "Failed to unregister {key}: {e}");
    }
}

/// After uninstalling: give the protocols back to the official Roblox install, or remove them.
pub fn restore_official_handlers() -> (bool, bool) {
    let read_loc = |name: &str| -> Option<String> {
        hkcu()
            .open_subkey(format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{name}"))
            .ok()?
            .get_value::<String, _>("InstallLocation")
            .ok()
    };

    let player = read_loc("roblox-player");
    match &player {
        Some(dir) => {
            let exe = std::path::Path::new(dir).join("RobloxPlayerBeta.exe");
            let _ = register_player(&exe.to_string_lossy(), "%1");
        }
        None => {
            unregister("roblox");
            unregister("roblox-player");
        }
    }

    let studio = read_loc("roblox-studio");
    match &studio {
        Some(dir) => {
            let exe = std::path::Path::new(dir).join("RobloxStudioBeta.exe");
            let exe = exe.to_string_lossy().to_string();
            let _ = register_studio_protocol(&exe, "%1");
            let _ = register_studio_file_class(&exe, "-ide \"%1\"");
        }
        None => {
            for k in ["roblox-studio", "roblox-studio-auth", ROBLOX_PLACE_KEY, ".rbxl", ".rbxlx"] {
                unregister(k);
            }
        }
    }
    (player.is_some(), studio.is_some())
}

pub fn write_uninstall_entry(app_path: &str, base: &str) -> io::Result<()> {
    use crate::app::*;
    let (k, _) = hkcu().create_subkey(UNINSTALL_KEY)?;
    k.set_value("DisplayIcon", &format!("{app_path},0"))?;
    k.set_value("DisplayName", &PROJECT_NAME)?;
    k.set_value("DisplayVersion", &VERSION)?;
    if k.get_value::<String, _>("InstallDate").is_err() {
        k.set_value("InstallDate", &chrono::Local::now().format("%Y%m%d").to_string())?;
    }
    k.set_value("InstallLocation", &base)?;
    k.set_value("NoRepair", &1u32)?;
    k.set_value("Publisher", &PROJECT_NAME)?;
    k.set_value("ModifyPath", &format!("\"{app_path}\" -menu"))?;
    k.set_value("QuietUninstallString", &format!("\"{app_path}\" -uninstall -quiet"))?;
    k.set_value("UninstallString", &format!("\"{app_path}\" -uninstall"))?;
    k.set_value("HelpLink", &PROJECT_HELP_LINK)?;
    k.set_value("URLInfoAbout", &PROJECT_SUPPORT_LINK)?;
    k.set_value("URLUpdateInfo", &PROJECT_DOWNLOAD_LINK)?;
    Ok(())
}

pub fn delete_uninstall_entry() {
    let _ = hkcu().delete_subkey_all(UNINSTALL_KEY);
}
