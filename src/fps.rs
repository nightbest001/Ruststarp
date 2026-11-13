//! FPS meter for the performance overlay, the way PresentMon does it: an ETW (Event Tracing for Windows)
//! session listens to the DXGI provider and counts `Present` calls of the Roblox process per second.
//!
//! Starting an ETW session needs administrator rights, so the overlay starts `ruststrap -fpsmeter` through
//! the UAC prompt (`spawn_elevated`). That small elevated process writes the number to `fps.txt` once a
//! second; the overlay (which runs as the normal user) only reads that file.
//!
//! Covers Direct3D 10/11/12 games (Roblox's default renderer). Vulkan is not counted.

use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows_sys::core::GUID;
use windows_sys::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, EnableTraceEx2, OpenTraceW, ProcessTrace, StartTraceW, EVENT_RECORD, EVENT_TRACE_LOGFILEW,
    EVENT_TRACE_PROPERTIES,
};

use crate::{paths, singleton};

const IDENT: &str = "FpsMeter";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// values from evntrace.h / evntcons.h
const ERROR_ALREADY_EXISTS: u32 = 183;
const WNODE_FLAG_TRACED_GUID: u32 = 0x0002_0000;
const EVENT_TRACE_REAL_TIME_MODE: u32 = 0x0000_0100;
const PROCESS_TRACE_MODE_REAL_TIME: u32 = 0x0000_0100;
const PROCESS_TRACE_MODE_EVENT_RECORD: u32 = 0x1000_0000;
const EVENT_CONTROL_CODE_ENABLE_PROVIDER: u32 = 1;
const EVENT_TRACE_CONTROL_STOP: u32 = 1;
const TRACE_LEVEL_VERBOSE: u8 = 5;

/// Microsoft-Windows-DXGI. Event 42 = Present start, 45 = PresentMultiplaneOverlay start.
const DXGI_PROVIDER: GUID = GUID { data1: 0xCA11C036, data2: 0x0102, data3: 0x4A2D, data4: [0xA6, 0xAD, 0xF0, 0x3C, 0xFE, 0xD5, 0xD3, 0xC9] };
const SESSION_NAME: &str = "RuststrapFpsMeter";
const OWNER_MUTEX: &str = "Local\\Ruststrap_FpsMeterOwner";

/// Process ids of Roblox windows, refreshed every couple of seconds.
static ROBLOX_PIDS: Mutex<Vec<u32>> = Mutex::new(Vec::new());
/// Present calls seen since the last second tick, per Roblox process id.
static COUNTS: Mutex<Vec<(u32, u32)>> = Mutex::new(Vec::new());

#[link(name = "shell32")]
extern "system" {
    fn ShellExecuteW(hwnd: isize, operation: *const u16, file: *const u16, params: *const u16, dir: *const u16, show: i32) -> isize;
}

fn fps_file() -> std::path::PathBuf {
    paths::get().base.join("fps.txt")
}

// ------------------------------------------------------------------ overlay side

/// Starts the meter with a UAC prompt. False when the user said no (or Windows refused).
pub fn spawn_elevated() -> bool {
    let exe = singleton::wide(&paths::process().to_string_lossy());
    let verb = singleton::wide("runas");
    let args = singleton::wide("-fpsmeter");
    // SAFETY: all strings are NUL-terminated and outlive the call; SW_HIDE = 0.
    let r = unsafe { ShellExecuteW(0, verb.as_ptr(), exe.as_ptr(), args.as_ptr(), std::ptr::null(), 0) };
    crate::log!(IDENT, "ShellExecute runas returned {r}");
    r > 32
}

/// Latest FPS written by the meter; `None` when there is no fresh number.
pub fn read() -> Option<f64> {
    let text = std::fs::read_to_string(fps_file()).ok()?;
    let mut it = text.split_whitespace();
    let fps: f64 = it.next()?.parse().ok()?;
    let at: u128 = it.next()?.parse().ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_millis();
    (now.saturating_sub(at) < 3500).then_some(fps)
}

// ------------------------------------------------------------------ meter side (elevated)

fn roblox_pids() -> Vec<u32> {
    let out = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq RobloxPlayerBeta.exe", "/NH", "/FO", "CSV"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(o) = out else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter(|l| l.contains("RobloxPlayerBeta.exe"))
        .filter_map(|l| l.trim_matches('"').split("\",\"").nth(1).and_then(|p| p.parse().ok()))
        .collect()
}

unsafe extern "system" fn on_event(rec: *mut EVENT_RECORD) {
    if rec.is_null() {
        return;
    }
    let h = &(*rec).EventHeader;
    let id = h.EventDescriptor.Id;
    if id != 42 && id != 45 {
        return;
    }
    let pid = h.ProcessId;
    if !ROBLOX_PIDS.lock().map(|p| p.contains(&pid)).unwrap_or(false) {
        return;
    }
    if let Ok(mut c) = COUNTS.lock() {
        match c.iter_mut().find(|e| e.0 == pid) {
            Some(e) => e.1 += 1,
            None => c.push((pid, 1)),
        }
    }
}

/// Zeroed EVENT_TRACE_PROPERTIES followed by room for the session name, as ETW wants it.
fn make_props(name_bytes: usize) -> Vec<u64> {
    let head = std::mem::size_of::<EVENT_TRACE_PROPERTIES>();
    let total = head + name_bytes;
    let mut buf = vec![0u64; (total + 7) / 8];
    let p = buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
    // SAFETY: `buf` is zeroed, 8-byte aligned and at least `total` bytes long.
    unsafe {
        (*p).Wnode.BufferSize = total as u32;
        (*p).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        (*p).Wnode.ClientContext = 1;
        (*p).BufferSize = 64; // KB
        (*p).MinimumBuffers = 4;
        (*p).LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
        (*p).FlushTimer = 1;
        (*p).LoggerNameOffset = head as u32;
    }
    buf
}

fn stop_session(name: &[u16]) {
    let mut buf = make_props(name.len() * 2);
    // SAFETY: valid properties buffer and NUL-terminated session name.
    unsafe {
        ControlTraceW(0, name.as_ptr(), buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES, EVENT_TRACE_CONTROL_STOP);
    }
}

/// Entry point of `ruststrap -fpsmeter` (runs elevated).
pub fn run() -> i32 {
    if !singleton::take_named(OWNER_MUTEX) {
        crate::log!(IDENT, "Another FPS meter is already running");
        return 0;
    }
    let name = singleton::wide(SESSION_NAME);

    let mut props = make_props(name.len() * 2);
    let mut session: u64 = 0;
    // SAFETY: valid out pointer, name and properties buffer.
    let mut status = unsafe { StartTraceW(&mut session, name.as_ptr(), props.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES) };
    if status == ERROR_ALREADY_EXISTS {
        // a session left behind by a crash
        stop_session(&name);
        props = make_props(name.len() * 2);
        // SAFETY: as above.
        status = unsafe { StartTraceW(&mut session, name.as_ptr(), props.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES) };
    }
    if status != 0 {
        crate::log!(IDENT, "StartTrace failed: {status} (are we elevated?)");
        return 1;
    }

    // SAFETY: session handle from StartTrace; provider GUID is a constant.
    let status = unsafe { EnableTraceEx2(session, &DXGI_PROVIDER, EVENT_CONTROL_CODE_ENABLE_PROVIDER, TRACE_LEVEL_VERBOSE, 0, 0, 0, std::ptr::null()) };
    if status != 0 {
        crate::log!(IDENT, "EnableTraceEx2 failed: {status}");
        stop_session(&name);
        return 1;
    }

    // SAFETY: an all-zero EVENT_TRACE_LOGFILEW is valid; the fields set below are the ones ETW needs.
    let mut logfile: EVENT_TRACE_LOGFILEW = unsafe { std::mem::zeroed() };
    logfile.LoggerName = name.as_ptr() as *mut u16;
    logfile.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
    logfile.Anonymous2.EventRecordCallback = Some(on_event);
    // SAFETY: `logfile` and `name` stay alive until after ProcessTrace returns.
    let trace = unsafe { OpenTraceW(&mut logfile) };
    if trace == u64::MAX {
        crate::log!(IDENT, "OpenTrace failed");
        stop_session(&name);
        return 1;
    }
    crate::log!(IDENT, "ETW session is up");

    // ProcessTrace blocks until the session is stopped
    let consumer = thread::spawn(move || {
        let h = trace;
        // SAFETY: `h` came from OpenTraceW.
        let r = unsafe { ProcessTrace(&h, 1, std::ptr::null(), std::ptr::null()) };
        crate::log!(IDENT, "ProcessTrace ended: {r}");
    });

    let mut last = Instant::now();
    let mut last_pid_check = Instant::now() - Duration::from_secs(10);
    let mut misses = 0u32;
    loop {
        thread::sleep(Duration::from_millis(1000));
        if last_pid_check.elapsed() >= Duration::from_secs(2) {
            last_pid_check = Instant::now();
            let pids = roblox_pids();
            if pids.is_empty() {
                misses += 1;
                if misses >= 3 {
                    break;
                }
            } else {
                misses = 0;
            }
            if let Ok(mut p) = ROBLOX_PIDS.lock() {
                *p = pids;
            }
        }
        let secs = last.elapsed().as_secs_f64().max(0.001);
        last = Instant::now();
        // several Roblox windows: the busiest one is the one you are playing
        let best = COUNTS.lock().map(|mut c| {
            let m = c.iter().map(|e| e.1).max().unwrap_or(0);
            c.clear();
            m
        });
        let fps = best.unwrap_or(0) as f64 / secs;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let _ = std::fs::write(fps_file(), format!("{fps:.1} {now}"));
    }

    stop_session(&name);
    // SAFETY: closes the handle from OpenTraceW once.
    unsafe {
        CloseTrace(trace);
    }
    let _ = consumer.join();
    let _ = std::fs::remove_file(fps_file());
    crate::log!(IDENT, "FPS meter closed");
    0
}

#[allow(unused)]
fn _keep(_: *const c_void) {}
