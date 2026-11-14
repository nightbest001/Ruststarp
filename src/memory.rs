//! RAM cleaner, the same idea as Mem Reduct (https://github.com/henrypp/memreduct).
//!
//! It asks Windows to drop memory it only keeps as cache (working sets, file cache, standby pages ...).
//! This is an independent Rust version of the same Windows calls; no Mem Reduct source is copied
//! (Mem Reduct is GPL-3.0, so do not paste its code into this project without checking the license).
//!
//! The calls need administrator rights. The settings window therefore starts `ruststrap -memclean <mask>`
//! through the UAC prompt (like the FPS meter) and reads the result from `memclean.json`. If Ruststrap is
//! already elevated it cleans in-process.

use std::ffi::c_void;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

use crate::{lang, paths, singleton};

const IDENT: &str = "Memory";

// same bit layout as Mem Reduct
pub const WORKING_SET: u32 = 0x01;
pub const SYSTEM_FILE_CACHE: u32 = 0x02;
pub const STANDBY_PRIORITY0: u32 = 0x04;
pub const STANDBY_LIST: u32 = 0x08; // can freeze Windows for a moment
pub const MODIFIED_LIST: u32 = 0x10; // can freeze Windows for a moment
pub const COMBINE_LISTS: u32 = 0x20;
pub const REGISTRY_CACHE: u32 = 0x40;
pub const MODIFIED_FILE_CACHE: u32 = 0x80;
const ALL_MASK: u32 = 0xFF;
pub const DEFAULT_MASK: u32 =
    WORKING_SET | SYSTEM_FILE_CACHE | STANDBY_PRIORITY0 | COMBINE_LISTS | REGISTRY_CACHE | MODIFIED_FILE_CACHE;

// SYSTEM_INFORMATION_CLASS values (ntdll, undocumented but stable since Windows 7 / 8.1 / 10)
const SYSTEM_MEMORY_LIST_INFORMATION: i32 = 80;
const SYSTEM_FILE_CACHE_INFORMATION_EX: i32 = 81;
const SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION: i32 = 130;
const SYSTEM_REGISTRY_RECONCILIATION_INFORMATION: i32 = 155;

// SYSTEM_MEMORY_LIST_COMMAND
const MEMORY_EMPTY_WORKING_SETS: u32 = 2;
const MEMORY_FLUSH_MODIFIED_LIST: u32 = 3;
const MEMORY_PURGE_STANDBY_LIST: u32 = 4;
const MEMORY_PURGE_LOW_PRIORITY_STANDBY_LIST: u32 = 5;

const SE_INCREASE_QUOTA_PRIVILEGE: u32 = 5;
const SE_PROF_SINGLE_PROCESS_PRIVILEGE: u32 = 13;

#[repr(C)]
struct FileCacheInfo {
    current_size: usize,
    peak_size: usize,
    page_fault_count: u32,
    minimum_working_set: usize,
    maximum_working_set: usize,
    current_size_including_transition: usize,
    peak_size_including_transition: usize,
    transition_repurpose_count: u32,
    flags: u32,
}

#[repr(C)]
struct CombineInfoEx {
    handle: usize,
    pages_combined: usize,
    flags: u32,
}

#[link(name = "ntdll")]
extern "system" {
    fn NtSetSystemInformation(class: i32, info: *mut c_void, len: u32) -> i32;
    fn RtlAdjustPrivilege(privilege: u32, enable: u8, current_thread: u8, was_enabled: *mut u8) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn IsUserAnAdmin() -> i32;
    fn ShellExecuteW(hwnd: isize, operation: *const u16, file: *const u16, params: *const u16, dir: *const u16, show: i32) -> isize;
}

// ------------------------------------------------------------------ numbers for the page

pub struct Info {
    pub total: u64,
    pub avail: u64,
    pub load: u32,
}

pub fn info() -> Option<Info> {
    // SAFETY: zeroed MEMORYSTATUSEX with dwLength set, as the API requires.
    unsafe {
        let mut m: MEMORYSTATUSEX = std::mem::zeroed();
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut m) == 0 {
            return None;
        }
        Some(Info { total: m.ullTotalPhys, avail: m.ullAvailPhys, load: m.dwMemoryLoad })
    }
}

pub fn is_admin() -> bool {
    // SAFETY: no arguments.
    unsafe { IsUserAnAdmin() != 0 }
}

pub fn info_json() -> Value {
    match info() {
        Some(i) => json!({
            "total": i.total,
            "avail": i.avail,
            "used": i.total.saturating_sub(i.avail),
            "load": i.load,
            "admin": is_admin(),
        }),
        None => json!({"total": 0, "avail": 0, "used": 0, "load": 0, "admin": is_admin()}),
    }
}

fn used() -> u64 {
    info().map(|i| i.total.saturating_sub(i.avail)).unwrap_or(0)
}

// ------------------------------------------------------------------ cleaning (needs admin)

pub struct Report {
    pub before: u64,
    pub after: u64,
    pub errors: Vec<String>,
}

impl Report {
    fn to_json(&self) -> Value {
        json!({
            "before": self.before,
            "after": self.after,
            "freed": self.before.saturating_sub(self.after),
            "errors": self.errors,
        })
    }
}

fn check(errors: &mut Vec<String>, name: &str, status: i32) {
    if status < 0 {
        crate::log!(IDENT, "{name} failed: NTSTATUS 0x{:08X}", status as u32);
        errors.push(format!("{name} (0x{:08X})", status as u32));
    } else {
        crate::log!(IDENT, "{name} ok");
    }
}

fn memory_list(command: u32) -> i32 {
    let mut cmd = command;
    // SAFETY: the command is a 4-byte value, which is what SystemMemoryListInformation expects.
    unsafe { NtSetSystemInformation(SYSTEM_MEMORY_LIST_INFORMATION, &mut cmd as *mut u32 as *mut c_void, 4) }
}

fn file_cache() -> i32 {
    let mut info = FileCacheInfo {
        current_size: 0,
        peak_size: 0,
        page_fault_count: 0,
        minimum_working_set: usize::MAX,
        maximum_working_set: usize::MAX,
        current_size_including_transition: 0,
        peak_size_including_transition: 0,
        transition_repurpose_count: 0,
        flags: 0,
    };
    // SAFETY: valid pointer and the size of the struct.
    unsafe {
        NtSetSystemInformation(
            SYSTEM_FILE_CACHE_INFORMATION_EX,
            &mut info as *mut FileCacheInfo as *mut c_void,
            std::mem::size_of::<FileCacheInfo>() as u32,
        )
    }
}

fn combine_lists() -> i32 {
    let mut info = CombineInfoEx { handle: 0, pages_combined: 0, flags: 0 };
    // SAFETY: valid pointer and the size of the struct.
    unsafe {
        NtSetSystemInformation(
            SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION,
            &mut info as *mut CombineInfoEx as *mut c_void,
            std::mem::size_of::<CombineInfoEx>() as u32,
        )
    }
}

fn registry_cache() -> i32 {
    // SAFETY: this class takes no data.
    unsafe { NtSetSystemInformation(SYSTEM_REGISTRY_RECONCILIATION_INFORMATION, std::ptr::null_mut(), 0) }
}

/// Writes the modified file cache of every drive to disk (FlushFileBuffers on `\\.\X:`).
fn flush_volumes() -> i32 {
    for letter in b'A'..=b'Z' {
        let c = letter as char;
        if !Path::new(&format!("{c}:\\")).exists() {
            continue;
        }
        if let Ok(f) = OpenOptions::new().write(true).open(format!(r"\\.\{c}:")) {
            let _ = f.sync_all();
        }
    }
    0
}

/// Runs the chosen cleaning steps in this process. Must be elevated.
pub fn clean(mask: u32) -> Report {
    let mask = mask & ALL_MASK;
    let before = used();
    let mut errors = Vec::new();

    for p in [SE_PROF_SINGLE_PROCESS_PRIVILEGE, SE_INCREASE_QUOTA_PRIVILEGE] {
        let mut was = 0u8;
        // SAFETY: valid out pointer.
        let st = unsafe { RtlAdjustPrivilege(p, 1, 0, &mut was) };
        if st < 0 {
            crate::log!(IDENT, "RtlAdjustPrivilege({p}) failed: 0x{:08X}", st as u32);
        }
    }

    if mask & WORKING_SET != 0 {
        check(&mut errors, "Working set", memory_list(MEMORY_EMPTY_WORKING_SETS));
    }
    if mask & SYSTEM_FILE_CACHE != 0 {
        check(&mut errors, "System file cache", file_cache());
    }
    if mask & MODIFIED_FILE_CACHE != 0 {
        check(&mut errors, "Modified file cache", flush_volumes());
    }
    if mask & MODIFIED_LIST != 0 {
        check(&mut errors, "Modified page list", memory_list(MEMORY_FLUSH_MODIFIED_LIST));
    }
    if mask & STANDBY_LIST != 0 {
        check(&mut errors, "Standby list", memory_list(MEMORY_PURGE_STANDBY_LIST));
    }
    if mask & STANDBY_PRIORITY0 != 0 {
        check(&mut errors, "Standby list (priority 0)", memory_list(MEMORY_PURGE_LOW_PRIORITY_STANDBY_LIST));
    }
    if mask & REGISTRY_CACHE != 0 {
        check(&mut errors, "Registry cache", registry_cache());
    }
    if mask & COMBINE_LISTS != 0 {
        check(&mut errors, "Combine memory lists", combine_lists());
    }

    Report { before, after: used(), errors }
}

// ------------------------------------------------------------------ settings window side

fn result_file() -> PathBuf {
    paths::get().base.join("memclean.json")
}

fn spawn_elevated(mask: u32) -> bool {
    let exe = singleton::wide(&paths::process().to_string_lossy());
    let verb = singleton::wide("runas");
    let args = singleton::wide(&format!("-memclean {mask}"));
    // SAFETY: all strings are NUL-terminated and outlive the call; SW_HIDE = 0.
    let r = unsafe { ShellExecuteW(0, verb.as_ptr(), exe.as_ptr(), args.as_ptr(), std::ptr::null(), 0) };
    crate::log!(IDENT, "ShellExecute runas returned {r}");
    r > 32
}

/// Called by the settings page. Blocks until done (the page calls this off the UI thread).
pub fn clean_for_ui(mask: u32) -> Result<Value, String> {
    let mask = mask & ALL_MASK;
    if mask == 0 {
        return Err(lang::pick("Pick at least one memory area.", "Pilih minimal satu area memori."));
    }
    if is_admin() {
        return Ok(clean(mask).to_json());
    }

    let file = result_file();
    let _ = fs::remove_file(&file);
    if !spawn_elevated(mask) {
        return Err(lang::pick(
            "Administrator permission was refused, nothing was cleaned.",
            "Izin administrator ditolak, nggak ada yang dibersihkan.",
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        if let Ok(text) = fs::read_to_string(&file) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                return Ok(v);
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err(lang::pick("The cleaner did not answer in time.", "Pembersih nggak menjawab tepat waktu."))
}

// ------------------------------------------------------------------ elevated side

/// Entry point of `ruststrap -memclean <mask>` (runs elevated).
pub fn run(data: Option<&str>) -> i32 {
    let mask = data.and_then(|d| d.trim().parse::<u32>().ok()).unwrap_or(DEFAULT_MASK);
    crate::log!(IDENT, "Cleaning with mask 0x{mask:02X}");
    let report = clean(mask);
    let file = result_file();
    let tmp = file.with_extension("tmp");
    // write then rename, so the page never reads half a file
    let ok = fs::write(&tmp, report.to_json().to_string()).and_then(|_| fs::rename(&tmp, &file));
    if let Err(e) = ok {
        let e: io::Error = e;
        crate::log!(IDENT, "Could not write the result: {e}");
        return 1;
    }
    0
}
