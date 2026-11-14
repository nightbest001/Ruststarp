//! "Max alts": keeps the Roblox windows you are NOT playing light on the PC.
//!
//! The Roblox window that has the keyboard focus stays at full speed (it gets the priority chosen in
//! "Process priority"). Every other Roblox window gets Idle CPU priority plus Windows' own efficiency mode
//! (EcoQoS) and, if the user wants it, a memory trim every 20 seconds. Only the standard Windows process
//! controls that Task Manager also offers are used; nothing inside Roblox is touched, and none of it needs
//! admin rights. If Roblox protects a process, that process is simply left alone.
//!
//! It runs as a thread of the tray process (`start`), so it works while Ruststrap sits in the System Tray.
//! Like the window layout, it reads `Settings.json` again on every check, so a change made in the settings
//! window applies within a couple of seconds. When no Roblox window is in front (you are in a browser, say),
//! every Roblox window counts as a background one.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::boost::Priority;
use crate::layout::disk_settings;
use crate::models::Settings;

#[repr(C)]
struct ProcessEntry {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    base_priority: i32,
    flags: u32,
    exe: [u16; 260],
}

#[repr(C)]
struct PowerThrottlingState {
    version: u32,
    control_mask: u32,
    state_mask: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> isize;
    fn Process32FirstW(snapshot: isize, entry: *mut ProcessEntry) -> i32;
    fn Process32NextW(snapshot: isize, entry: *mut ProcessEntry) -> i32;
    fn CloseHandle(handle: isize) -> i32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn GetPriorityClass(process: isize) -> u32;
    fn SetPriorityClass(process: isize, class: u32) -> i32;
    fn SetProcessInformation(process: isize, class: i32, info: *const c_void, size: u32) -> i32;
    fn K32EmptyWorkingSet(process: isize) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
}

const TH32CS_SNAPPROCESS: u32 = 0x2;
const PROCESS_SET_QUOTA: u32 = 0x0100;
const PROCESS_SET_INFORMATION: u32 = 0x0200;
const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;
const NORMAL_PRIORITY_CLASS: u32 = 0x0000_0020;
/// `PROCESS_INFORMATION_CLASS::ProcessPowerThrottling`
const PROCESS_POWER_THROTTLING: i32 = 4;
const POWER_THROTTLING_CURRENT_VERSION: u32 = 1;
const POWER_THROTTLING_EXECUTION_SPEED: u32 = 0x1;
const TRIM_EVERY: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_millis(2000);

/// True while we may have changed some Roblox process (so it has to be put back).
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Process ids of every running `RobloxPlayerBeta.exe`.
fn roblox_pids() -> Vec<u32> {
    let mut out = Vec::new();
    // SAFETY: the snapshot handle is closed before returning; `entry` has its size set and outlives the calls.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == -1 || snap == 0 {
            return out;
        }
        let mut entry: ProcessEntry = std::mem::zeroed();
        entry.size = std::mem::size_of::<ProcessEntry>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            let len = entry.exe.iter().position(|&c| c == 0).unwrap_or(entry.exe.len());
            if String::from_utf16_lossy(&entry.exe[..len]).eq_ignore_ascii_case("RobloxPlayerBeta.exe") {
                out.push(entry.pid);
            }
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    out
}

/// The process that owns the window with the keyboard focus.
fn foreground_pid() -> Option<u32> {
    // SAFETY: plain calls with a valid out-pointer.
    unsafe {
        let fg = GetForegroundWindow();
        if fg == 0 {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, &mut pid);
        (pid != 0).then_some(pid)
    }
}

/// The priority the window in front gets: what "Process priority" says (Normal when it says default).
fn front_class(s: &Settings) -> u32 {
    Priority::parse(&s.process_priority).unwrap_or(Priority::Default).class().unwrap_or(NORMAL_PRIORITY_CLASS)
}

fn set_priority(pid: u32, class: u32) {
    // SAFETY: the handle is opened and closed here. Roblox may refuse the access; then nothing changes.
    unsafe {
        let h = OpenProcess(PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            return;
        }
        if GetPriorityClass(h) != class {
            SetPriorityClass(h, class);
        }
        CloseHandle(h);
    }
}

/// Windows' efficiency mode (what Task Manager shows as "Efficiency mode").
fn set_efficiency(pid: u32, on: bool) {
    let state = PowerThrottlingState {
        version: POWER_THROTTLING_CURRENT_VERSION,
        control_mask: POWER_THROTTLING_EXECUTION_SPEED,
        state_mask: if on { POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
    };
    // SAFETY: `state` outlives the call and the size matches the struct.
    unsafe {
        let h = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if h == 0 {
            return;
        }
        SetProcessInformation(
            h,
            PROCESS_POWER_THROTTLING,
            &state as *const PowerThrottlingState as *const c_void,
            std::mem::size_of::<PowerThrottlingState>() as u32,
        );
        CloseHandle(h);
    }
}

/// Asks Windows to take the memory this process is not using right now.
fn trim(pid: u32) {
    // SAFETY: the handle is opened and closed here.
    unsafe {
        let h = OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION, 0, pid);
        if h == 0 {
            return;
        }
        K32EmptyWorkingSet(h);
        CloseHandle(h);
    }
}

struct State {
    /// pid -> is it a background window (what we last told Windows about it)
    roles: HashMap<u32, bool>,
    last_trim: Option<Instant>,
}

/// Puts every Roblox process back to normal.
fn reset(front: u32, st: &mut State) {
    for pid in roblox_pids() {
        set_priority(pid, front);
        set_efficiency(pid, false);
    }
    st.roles.clear();
}

fn tick(st: &mut State) {
    let Some(cfg) = disk_settings() else { return };
    if !cfg.max_alts {
        if ACTIVE.swap(false, Ordering::SeqCst) {
            reset(front_class(&cfg), st);
        }
        return;
    }
    ACTIVE.store(true, Ordering::SeqCst);

    let pids = roblox_pids();
    st.roles.retain(|p, _| pids.contains(p));
    let active = foreground_pid().filter(|p| pids.contains(p));
    let front = front_class(&cfg);
    let trim_now = cfg.max_alts_trim && st.last_trim.map_or(true, |t| t.elapsed() >= TRIM_EVERY);
    let mut trimmed = false;

    for &pid in &pids {
        let background = Some(pid) != active;
        // checked every time: a new process, or something that changed its priority, is corrected
        set_priority(pid, if background { IDLE_PRIORITY_CLASS } else { front });
        if st.roles.get(&pid) != Some(&background) {
            set_efficiency(pid, background);
            st.roles.insert(pid, background);
        }
        if background && trim_now {
            trim(pid);
            trimmed = true;
        }
    }
    if trimmed {
        st.last_trim = Some(Instant::now());
    }
}

/// Starts the thread that does the work (called by the tray process).
pub fn start() {
    thread::spawn(|| {
        let mut st = State { roles: HashMap::new(), last_trim: None };
        loop {
            tick(&mut st);
            thread::sleep(CHECK_EVERY);
        }
    });
}

/// Called when the tray process ends: Roblox windows that are still running go back to normal.
pub fn reset_now() {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let front = disk_settings().map(|s| front_class(&s)).unwrap_or(NORMAL_PRIORITY_CLASS);
    let mut st = State { roles: HashMap::new(), last_trim: None };
    reset(front, &mut st);
}
