//! GPU detection plus the Windows per-app GPU preference
//! (Settings > System > Display > Graphics). On laptops with two GPUs this is what makes
//! Roblox run on the fast one instead of the integrated one.

use std::io;
use std::path::{Path, PathBuf};

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};
use winreg::RegKey;

use crate::{app, paths};

const CLASS_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
const PREF_KEY: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";
const EXES: [&str; 2] = ["RobloxPlayerBeta.exe", "RobloxStudioBeta.exe"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Discrete,
    Integrated,
    Other,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Discrete => "discrete",
            Kind::Integrated => "integrated",
            Kind::Other => "unknown",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Adapter {
    pub name: String,
    pub vendor: &'static str,
    pub kind: Kind,
    pub vram_mb: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pref {
    Default,
    PowerSaving,
    High,
}

impl Pref {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "default" | "auto" | "off" | "" => Some(Pref::Default),
            "saving" | "low" | "power-saving" | "powersaving" => Some(Pref::PowerSaving),
            "high" | "gpu" | "performance" | "on" => Some(Pref::High),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Pref::Default => "default",
            Pref::PowerSaving => "saving",
            Pref::High => "high",
        }
    }

    fn reg_value(self) -> Option<&'static str> {
        match self {
            Pref::Default => None,
            Pref::PowerSaving => Some("GpuPreference=1;"),
            Pref::High => Some("GpuPreference=2;"),
        }
    }
}

// ------------------------------------------------------------------ detection

fn vendor_of(id: &str, lname: &str) -> &'static str {
    if id.contains("ven_10de") || lname.contains("nvidia") || lname.contains("geforce") {
        "NVIDIA"
    } else if id.contains("ven_1002") || lname.contains("radeon") || lname.contains("amd") {
        "AMD"
    } else if id.contains("ven_8086") || lname.contains("intel") {
        "Intel"
    } else {
        "Other"
    }
}

/// Best-effort guess. Windows does not say "discrete" anywhere in the registry.
fn classify(vendor: &str, lname: &str, vram_mb: Option<u64>) -> Kind {
    match vendor {
        "NVIDIA" => Kind::Discrete,
        "AMD" => {
            let apu = lname.contains("radeon(tm) graphics")
                || lname.contains("radeon graphics")
                || (lname.contains("vega") && !lname.contains("rx") && !lname.contains("frontier"));
            if apu { Kind::Integrated } else { Kind::Discrete }
        }
        "Intel" => {
            if lname.contains("arc") && vram_mb.unwrap_or(0) >= 2048 { Kind::Discrete } else { Kind::Integrated }
        }
        _ => Kind::Other,
    }
}

fn read_vram_mb(k: &RegKey) -> Option<u64> {
    for name in ["HardwareInformation.qwMemorySize", "HardwareInformation.MemorySize"] {
        if let Ok(raw) = k.get_raw_value(name) {
            let b: &[u8] = &raw.bytes;
            if b.is_empty() || b.len() > 8 {
                continue;
            }
            let mut buf = [0u8; 8];
            buf[..b.len()].copy_from_slice(b);
            let bytes = u64::from_le_bytes(buf);
            if bytes > 0 {
                return Some(bytes / (1024 * 1024));
            }
        }
    }
    None
}

pub fn detect() -> Vec<Adapter> {
    let Ok(class) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(CLASS_KEY) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for sub in class.enum_keys().flatten() {
        if sub.len() != 4 || !sub.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(k) = class.open_subkey(&sub) else { continue };
        let Ok(name) = k.get_value::<String, _>("DriverDesc") else { continue };
        let id = k.get_value::<String, _>("MatchingDeviceId").unwrap_or_default().to_lowercase();
        if !id.contains("ven_") {
            continue; // basic / remote / virtual display adapters have no PCI id
        }
        let lname = name.to_lowercase();
        let vendor = vendor_of(&id, &lname);
        let vram_mb = read_vram_mb(&k);
        let kind = classify(vendor, &lname, vram_mb);
        out.push(Adapter { name, vendor, kind, vram_mb });
    }
    out
}

// ------------------------------------------------------------------ preference

pub fn set_for_exe(exe: &Path, pref: Pref) -> io::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(PREF_KEY)?;
    let name = exe.to_string_lossy().to_string();
    match pref.reg_value() {
        Some(v) => key.set_value(&name, &v),
        None => match key.delete_value(&name) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

fn installed_exes() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&paths::get().versions) {
        for e in rd.flatten() {
            for exe in EXES {
                let p = e.path().join(exe);
                if p.is_file() {
                    v.push(p);
                }
            }
        }
    }
    v
}

/// Writes `pref` for every Roblox exe that is installed now (used by the settings window).
/// Returns one line per exe that could not be written; empty means everything worked.
pub fn apply_installed(pref: Pref) -> Vec<String> {
    let mut failed = Vec::new();
    for exe in installed_exes() {
        if let Err(e) = set_for_exe(&exe, pref) {
            failed.push(format!("{}: {e}", exe.display()));
        }
    }
    failed
}

/// Drop preference entries for old Roblox version folders that no longer exist.
fn clean_stale() {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(PREF_KEY, KEY_READ | KEY_WRITE) else {
        return;
    };
    let versions = paths::get().versions.to_string_lossy().to_lowercase();
    let stale: Vec<String> = key
        .enum_values()
        .flatten()
        .map(|(n, _)| n)
        .filter(|n| n.to_lowercase().starts_with(&versions) && !Path::new(n).exists())
        .collect();
    for n in stale {
        let _ = key.delete_value(&n);
    }
}

/// Called right before Roblox starts. Every Roblox update lives in a new folder, and Windows keys
/// the GPU preference by full exe path, so it has to be written again each time.
pub fn apply_launch(exe: &Path) {
    const IDENT: &str = "Gpu::ApplyLaunch";
    let adapters = detect();
    for a in &adapters {
        crate::log!(
            IDENT,
            "Found GPU: {} ({}, {}, {})",
            a.name,
            a.vendor,
            a.kind.label(),
            a.vram_mb.map(|m| format!("{m} MB")).unwrap_or_else(|| "VRAM unknown".into())
        );
    }

    let pref = Pref::parse(&app::settings().prop.gpu_preference).unwrap_or(Pref::Default);
    if pref == Pref::Default {
        return; // leave whatever the user set in Windows alone
    }
    if pref == Pref::High && !adapters.iter().any(|a| a.kind == Kind::Discrete) {
        crate::log!(IDENT, "No discrete GPU found; the preference may do nothing on this PC");
    }
    match set_for_exe(exe, pref) {
        Ok(()) => crate::log!(IDENT, "GPU preference '{}' set for {}", pref.as_str(), exe.display()),
        Err(e) => crate::log!(IDENT, "Failed to set GPU preference: {e}"),
    }
    clean_stale();
}

// ------------------------------------------------------------------ cli

fn print_adapters() {
    let adapters = detect();
    if adapters.is_empty() {
        println!("No GPU found in the registry.");
    }
    for (i, a) in adapters.iter().enumerate() {
        let vram = a.vram_mb.map(|m| format!("{m} MB")).unwrap_or_else(|| "VRAM unknown".into());
        println!("[{}] {}  |  {}  |  {}  |  {}", i + 1, a.name, a.vendor, a.kind.label(), vram);
    }
    let pref = Pref::parse(&app::settings().prop.gpu_preference).unwrap_or(Pref::Default);
    println!("Roblox GPU preference: {}", pref.as_str());
}

/// `ruststrap gpu [list | set <high|saving|default>]`
pub fn cli(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("list") | None => {
            print_adapters();
            0
        }
        Some("set") if args.len() >= 2 => {
            let Some(pref) = Pref::parse(&args[1]) else {
                eprintln!("value must be: high | saving | default");
                return 1;
            };
            {
                let mut s = app::settings();
                s.prop.gpu_preference = pref.as_str().to_string();
                s.save();
            }
            for exe in installed_exes() {
                if let Err(e) = set_for_exe(&exe, pref) {
                    eprintln!("failed for {}: {e}", exe.display());
                }
            }
            println!("ok. Roblox GPU preference: {}", pref.as_str());
            0
        }
        _ => {
            eprintln!("usage: ruststrap gpu [list | set <high|saving|default>]");
            1
        }
    }
}
