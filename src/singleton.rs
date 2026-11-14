//! Named Windows mutexes.
//!
//! * `ROBLOX_singletonMutex` and `ROBLOX_singletonEvent`: Roblox checks both names when it starts. A
//!   client that finds them free takes them, and the next client then closes it (that is why starting
//!   account 2 closed account 1). Ruststrap takes both names, and OWNS them on one thread that never
//!   ends, before the first client starts. Every client then believes it may run.
//!   Both are taken as plain mutexes: Roblox asks for an event with that name, gets a "wrong object
//!   type" answer instead of "already exists", and carries on. Nothing is injected into Roblox.
//! * `Local\Ruststrap_TrayOwner`: only one Ruststrap process at a time lives in the system tray.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attributes: *mut c_void, initial_owner: i32, name: *const u16) -> isize;
    fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> isize;
    fn CloseHandle(handle: isize) -> i32;
    fn GetLastError() -> u32;
    fn WaitForSingleObject(handle: isize, milliseconds: u32) -> u32;
}

const WAIT_OBJECT_0: u32 = 0;
const WAIT_ABANDONED: u32 = 0x80;
/// The two names Roblox checks. Both must be held.
const ROBLOX_NAMES: [&str; 2] = ["ROBLOX_singletonMutex", "ROBLOX_singletonEvent"];

const ERROR_ALREADY_EXISTS: u32 = 183;
const SYNCHRONIZE: u32 = 0x0010_0000;
const TRAY_MUTEX_NAME: &str = "Local\\Ruststrap_TrayOwner";

static TRAY_MUTEX: AtomicIsize = AtomicIsize::new(0);
static OVERLAY_MUTEX: AtomicIsize = AtomicIsize::new(0);
static NAMED_MUTEX: AtomicIsize = AtomicIsize::new(0);

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

struct Slot {
    name: &'static str,
    handle: isize,
    owned: bool,
}

impl Slot {
    /// Creates the named mutex (asking for ownership at once) or, when it exists, tries to take it over.
    /// Returns true the moment this slot becomes owned by the calling thread.
    fn step(&mut self) -> bool {
        if self.owned {
            return false;
        }
        let w = wide(self.name);
        // SAFETY: `w` is a valid NUL-terminated UTF-16 string; handles stay open for the whole process.
        unsafe {
            if self.handle == 0 {
                let h = CreateMutexW(std::ptr::null_mut(), 1, w.as_ptr());
                if h == 0 {
                    return false; // the name belongs to an object of another type (a running client's event)
                }
                self.handle = h;
                if GetLastError() != ERROR_ALREADY_EXISTS {
                    self.owned = true; // created with us as the owner
                    return true;
                }
            }
            // exists already: ours only if it is free, or its owner (another Ruststrap process) has ended
            let r = WaitForSingleObject(self.handle, 0);
            if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
                self.owned = true;
                return true;
            }
        }
        false
    }
}

/// Runs for the rest of the process on its own thread: a Windows mutex belongs to the thread that took it,
/// so this thread must never end. It also keeps trying, so when another Ruststrap process (the one that
/// started Roblox) ends, this one takes over without a gap.
fn roblox_holder(tx: mpsc::Sender<bool>) {
    const IDENT: &str = "Singleton::Roblox";
    let mut slots: Vec<Slot> = ROBLOX_NAMES.iter().map(|n| Slot { name: *n, handle: 0, owned: false }).collect();
    for s in slots.iter_mut() {
        s.step();
        crate::log!(IDENT, "{} {}", s.name, if s.owned { "owned" } else { "not owned yet (another process holds it)" });
    }
    let _ = tx.send(slots.iter().all(|s| s.owned));
    drop(tx);
    loop {
        std::thread::sleep(Duration::from_millis(50));
        for s in slots.iter_mut() {
            if s.step() {
                crate::log!(IDENT, "{} owned (taken over)", s.name);
            }
        }
    }
}

static ROBLOX_HELD: OnceLock<bool> = OnceLock::new();

/// Allow more than one Roblox client. Must run before the first client starts. True when this process owns
/// both names right now (false when another Ruststrap process owns them, or a Roblox that was already open
/// does; close all Roblox windows and start again from Ruststrap in that case).
pub fn hold_roblox_mutex() -> bool {
    *ROBLOX_HELD.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        match std::thread::Builder::new().name("RobloxMutexHolder".into()).spawn(move || roblox_holder(tx)) {
            Ok(_) => rx.recv_timeout(Duration::from_secs(3)).unwrap_or(false),
            Err(_) => false,
        }
    })
}

/// True when this process is the one that may show the tray icon.
pub fn become_tray_owner() -> bool {
    take(TRAY_MUTEX_NAME, &TRAY_MUTEX)
}

/// True when this process is the one that may show the performance overlay.
pub fn become_overlay_owner() -> bool {
    take("Local\\Ruststrap_OverlayOwner", &OVERLAY_MUTEX)
}

/// True when this process created the named mutex (nobody else held it). Used by the FPS meter.
pub fn take_named(name: &str) -> bool {
    take(name, &NAMED_MUTEX)
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
