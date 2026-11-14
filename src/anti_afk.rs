//! Anti-AFK: keeps Roblox accounts from being kicked for idling (Roblox kicks after about 20 minutes
//! without input).
//!
//! Every few minutes each Roblox window that is not the one you are using is brought to the front for a
//! fraction of a second, gets one small input (like your own mouse or keyboard), and the window you were
//! using is put back. Minimized windows are minimized again afterwards. Three actions:
//! * `nudge`: moves the mouse a few pixels over the window, then puts your cursor back. No effect on the
//!   character, so auto-farm and the like keep running (default);
//! * `space`: presses Space (a jump);
//! * `zoom`: presses I then O (camera in, camera out).
//!
//! It runs as a thread of the tray process (`start`) and reads `Settings.json` again on every check. A pulse
//! waits while you are typing or moving the mouse (up to 3 minutes past its time), so it does not steal the
//! focus in the middle of your work.

use std::thread;
use std::time::{Duration, Instant};

use crate::layout::{self, disk_settings, DpiGuard, Rect};

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct LastInputInfo {
    size: u32,
    time: u32,
}

#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn AttachThreadInput(attach: u32, attach_to: u32, on: i32) -> i32;
    fn BringWindowToTop(hwnd: isize) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn mouse_event(flags: u32, dx: i32, dy: i32, data: u32, extra: usize);
    fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
    fn GetLastInputInfo(info: *mut LastInputInfo) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn GetTickCount() -> u32;
}

const SW_MINIMIZE: i32 = 6;
const SW_RESTORE: i32 = 9;
const MOUSEEVENTF_MOVE: u32 = 0x0001;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const CHECK_EVERY: Duration = Duration::from_secs(2);
/// The user counts as busy when there was input in the last 1.5 seconds.
const BUSY_MS: u32 = 1500;
/// A pulse is put off while the user is busy, but never for longer than this.
const MAX_WAIT: Duration = Duration::from_secs(180);

fn sleep_ms(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

/// Milliseconds since the last key press or mouse move by the user.
fn idle_ms() -> u32 {
    let mut info = LastInputInfo { size: std::mem::size_of::<LastInputInfo>() as u32, time: 0 };
    // SAFETY: `info` has its size set and outlives the call.
    unsafe {
        if GetLastInputInfo(&mut info) == 0 {
            return u32::MAX;
        }
        GetTickCount().wrapping_sub(info.time)
    }
}

/// SetForegroundWindow is refused while another program has the focus; borrowing the input queue of the
/// current foreground window makes Windows accept it.
fn force_foreground(hwnd: isize) -> bool {
    // SAFETY: plain window calls; a handle that went away just makes them fail.
    unsafe {
        let fg = GetForegroundWindow();
        if fg == hwnd {
            return true;
        }
        let mut pid = 0u32;
        let fg_thread = if fg == 0 { 0 } else { GetWindowThreadProcessId(fg, &mut pid) };
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if attached {
            AttachThreadInput(me, fg_thread, 0);
        }
        GetForegroundWindow() == hwnd
    }
}

fn key(vk: u8, scan: u8) {
    // SAFETY: plain input calls.
    unsafe {
        keybd_event(vk, scan, 0, 0);
    }
    sleep_ms(70);
    unsafe {
        keybd_event(vk, scan, KEYEVENTF_KEYUP, 0);
    }
}

/// One small input. The window has the focus already.
fn do_action(hwnd: isize, action: &str) {
    match action {
        "space" => key(0x20, 0x39),
        "zoom" => {
            key(0x49, 0x17);
            sleep_ms(60);
            key(0x4F, 0x18);
        }
        _ => {
            // Roblox only sees mouse movement that happens over its own window: move the cursor to the middle
            // of it, nudge a few pixels there and back, then put the cursor back where the user had it.
            let mut orig = Point { x: 0, y: 0 };
            let mut r = Rect { left: 0, top: 0, right: 0, bottom: 0 };
            // SAFETY: valid out-pointers; the calls only read or move the cursor.
            unsafe {
                let have_orig = GetCursorPos(&mut orig) != 0;
                if GetWindowRect(hwnd, &mut r) != 0 && r.right - r.left > 20 && r.bottom - r.top > 20 {
                    SetCursorPos((r.left + r.right) / 2, (r.top + r.bottom) / 2);
                    sleep_ms(30);
                    mouse_event(MOUSEEVENTF_MOVE, 4, 2, 0, 0);
                    sleep_ms(40);
                    mouse_event(MOUSEEVENTF_MOVE, -4, -2, 0, 0);
                    sleep_ms(20);
                    if have_orig {
                        SetCursorPos(orig.x, orig.y);
                    }
                } else {
                    // no window rectangle: a plain relative nudge
                    mouse_event(MOUSEEVENTF_MOVE, 4, 0, 0, 0);
                    sleep_ms(40);
                    mouse_event(MOUSEEVENTF_MOVE, -4, 0, 0, 0);
                }
            }
        }
    }
}

/// Front for about 0.3 seconds, one small input. Minimized windows are minimized again.
fn poke(hwnd: isize, action: &str) -> bool {
    // SAFETY: plain window calls.
    let minimized = unsafe {
        if IsWindow(hwnd) == 0 {
            return false;
        }
        IsIconic(hwnd) != 0
    };
    if minimized {
        // SAFETY: as above.
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
        }
    }
    let mut ok = force_foreground(hwnd);
    if !ok {
        sleep_ms(100);
        ok = force_foreground(hwnd);
    }
    if ok {
        sleep_ms(120); // let Roblox notice it has the focus
        do_action(hwnd, action);
        sleep_ms(80);
    }
    if minimized {
        // SAFETY: as above.
        unsafe {
            ShowWindow(hwnd, SW_MINIMIZE);
        }
    }
    ok
}

/// Gives every Roblox window except the one in front its small input, then returns the focus.
/// Returns how many windows were reached.
pub fn pulse(action: &str) -> usize {
    let _dpi = DpiGuard::new();
    // SAFETY: plain call.
    let fg = unsafe { GetForegroundWindow() };
    let mut reached = 0;
    for hwnd in layout::find_windows() {
        if hwnd == fg {
            continue; // the window you are using needs nothing
        }
        if poke(hwnd, action) {
            reached += 1;
        }
        sleep_ms(50);
    }
    // SAFETY: plain call.
    if fg != 0 && unsafe { IsWindow(fg) } != 0 {
        force_foreground(fg); // give the user their window back
    }
    reached
}

struct State {
    /// When the next pulse is due (None while Anti-AFK is off).
    due: Option<Instant>,
    /// The interval `due` was worked out from, to notice a change of setting.
    minutes: u32,
}

fn tick(st: &mut State) {
    let Some(cfg) = disk_settings() else { return };
    if !cfg.anti_afk {
        st.due = None;
        return;
    }
    let minutes = cfg.anti_afk_minutes.clamp(1, 19);
    let every = Duration::from_secs(minutes as u64 * 60);
    if st.due.is_none() || st.minutes != minutes {
        st.due = Some(Instant::now() + every);
        st.minutes = minutes;
        return;
    }
    let due = st.due.unwrap_or_else(Instant::now);
    let now = Instant::now();
    if now < due {
        return;
    }
    // due: wait for a quiet moment, unless it has been too long already
    if idle_ms() < BUSY_MS && now < due + MAX_WAIT {
        return;
    }
    pulse(&cfg.anti_afk_action);
    st.due = Some(Instant::now() + every);
}

/// Starts the thread that does the work (called by the tray process).
pub fn start() {
    thread::spawn(|| {
        let mut st = State { due: None, minutes: 0 };
        loop {
            tick(&mut st);
            thread::sleep(CHECK_EVERY);
        }
    });
}
