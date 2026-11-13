//! Desktop, Start Menu and function shortcuts (the Shortcuts page of Bloxstrap).
//! Whether a shortcut is "on" is simply whether its .lnk file exists.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
use winreg::RegKey;

use crate::app::PROJECT_NAME;
use crate::paths;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SHELL_FOLDERS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Desktop,
    StartMenu,
    Player,
    Studio,
    Settings,
}

/// Same order as `SettingsApp::shortcuts`.
pub const ALL: [Kind; 5] = [Kind::Desktop, Kind::StartMenu, Kind::Player, Kind::Studio, Kind::Settings];

fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(a) = rest.find('%') {
        out.push_str(&rest[..a]);
        let tail = &rest[a + 1..];
        match tail.find('%') {
            Some(b) => {
                match std::env::var(&tail[..b]) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => out.push_str(&rest[a..a + b + 2]),
                }
                rest = &tail[b + 1..];
            }
            None => {
                out.push_str(&rest[a..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn shell_folder(name: &str, fallback: PathBuf) -> PathBuf {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(SHELL_FOLDERS, KEY_READ)
        .and_then(|k| k.get_value::<String, _>(name))
        .map(|v| PathBuf::from(expand_env(&v)))
        .unwrap_or(fallback)
}

fn desktop_dir() -> PathBuf {
    shell_folder("Desktop", paths::user_profile().join("Desktop"))
}

fn programs_dir() -> PathBuf {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    shell_folder("Programs", appdata.join(r"Microsoft\Windows\Start Menu\Programs"))
}

fn link_path(kind: Kind) -> PathBuf {
    match kind {
        Kind::Desktop => desktop_dir().join(format!("{PROJECT_NAME}.lnk")),
        Kind::StartMenu => programs_dir().join(format!("{PROJECT_NAME}.lnk")),
        Kind::Player => desktop_dir().join(format!("Play Roblox ({PROJECT_NAME}).lnk")),
        Kind::Studio => desktop_dir().join(format!("Roblox Studio ({PROJECT_NAME}).lnk")),
        Kind::Settings => desktop_dir().join(format!("{PROJECT_NAME} Settings.lnk")),
    }
}

fn link_args(kind: Kind) -> &'static str {
    match kind {
        Kind::Desktop | Kind::StartMenu => "",
        Kind::Player => "-player",
        Kind::Studio => "-studio",
        Kind::Settings => "-menu",
    }
}

pub fn exists(kind: Kind) -> bool {
    link_path(kind).is_file()
}

/// Current state of every shortcut, in `ALL` order.
pub fn current() -> [bool; 5] {
    let mut out = [false; 5];
    for (i, k) in ALL.iter().enumerate() {
        out[i] = exists(*k);
    }
    out
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub fn set(kind: Kind, on: bool) -> io::Result<()> {
    let lnk = link_path(kind);
    if !on {
        return match std::fs::remove_file(&lnk) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = lnk.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let installed = paths::get().application.clone();
    let target = if installed.is_file() { installed } else { paths::process() };
    let work = target.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut({lnk}); $s.TargetPath={t}; $s.Arguments={a}; \
         $s.WorkingDirectory={w}; $s.IconLocation={i}; $s.Save()",
        lnk = ps_quote(&lnk.to_string_lossy()),
        t = ps_quote(&target.to_string_lossy()),
        a = ps_quote(link_args(kind)),
        w = ps_quote(&work.to_string_lossy()),
        i = ps_quote(&format!("{},0", target.display())),
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, String::from_utf8_lossy(&out.stderr).trim().to_string()))
    }
}

/// Makes every shortcut match `want`. Returns one line per failure.
pub fn apply(want: &[bool; 5]) -> Vec<String> {
    let mut failed = Vec::new();
    for (i, k) in ALL.iter().enumerate() {
        if exists(*k) != want[i] {
            if let Err(e) = set(*k, want[i]) {
                failed.push(format!("{k:?}: {e}"));
            }
        }
    }
    failed
}
