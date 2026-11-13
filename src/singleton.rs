//! Named Windows mutexes.
//!
//! * `ROBLOX_singletonMutex`: the Roblox client refuses to start a second window when it cannot be the
//!   first to own this mutex. Taking it before the client starts, and keeping it while Ruststrap's
//!   tray process runs, lets several Roblox windows (several accounts) run side by side.
//! * `Local\Ruststrap_TrayOwner`: only one Ruststrap process at a time lives in the system tray.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attributes: *mut c_void, initial_owner: i32, name: *const u16) -> isize;
    fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> isize;
    fn CloseHandle(handle: isize) -> i32;
    fn GetLastError() -> u32;
}

const ERROR_ALREADY_EXISTS: u32 = 183;
const SYNCHRONIZE: u32 = 0x0010_0000;
const TRAY_MUTEX_NAME: &str = "Local\\Ruststrap_TrayOwner";

static ROBLOX_MUTEX: AtomicIsize = AtomicIsize::new(0);
static TRAY_MUTEX: AtomicIsize = AtomicIsize::new(0);
static OVERLAY_MUTEX: AtomicIsize = AtomicIsize::new(0);

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Opens or creates the mutex `name` and keeps the handle in `slot` for the life of the process.
/// True when this process created it (nobody else owned it before).
fn take(name: &str, slot: &AtomicIsize) -> bool {
    if slot.load(Ordering::SeqCst) != 0 {
        return true; // already ours
    }
    let w = wide(name);
    // SAFETY: `w` is a valid NUL-terminated UTF-16 string. The handle is never closed, so the mutex
    // lives as long as this process does.
    unsafe {
        let h = CreateMutexW(std::ptr::null_mut(), 0, w.as_ptr());
        if h == 0 {
            return false;
        }
        let existed = GetLastError() == ERROR_ALREADY_EXISTS;
        slot.store(h, Ordering::SeqCst);
        !existed
    }
}

/// Allow more than one Roblox client. Must run before the first client starts.
pub fn hold_roblox_mutex() -> bool {
    let first = take("ROBLOX_singletonMutex", &ROBLOX_MUTEX);
    crate::log!("Singleton::Roblox", "ROBLOX_singletonMutex held ({})", if first { "created" } else { "shared with another process" });
    first
}

/// True when this process is the one that may show the tray icon.
pub fn become_tray_owner() -> bool {
    take(TRAY_MUTEX_NAME, &TRAY_MUTEX)
}

/// True when this process is the one that may show the performance overlay.
pub fn become_overlay_owner() -> bool {
    take("Local\\Ruststrap_OverlayOwner", &OVERLAY_MUTEX)
}

/// True when some Ruststrap process already owns the tray.
pub fn tray_is_running() -> bool {
    if TRAY_MUTEX.load(Ordering::SeqCst) != 0 {
        return true;
    }
    let w = wide(TRAY_MUTEX_NAME);
    // SAFETY: valid NUL-terminated name; the handle is closed again right away.
    unsafe {
        let h = OpenMutexW(SYNCHRONIZE, 0, w.as_ptr());
        if h == 0 {
            return false;
        }
        CloseHandle(h);
        true
    }
}
