//! Lines up every open Roblox window in a neat grid ("Multi Account" > Roblox windows).
//!
//! Written from scratch for Ruststrap. Roblox windows are found by the program that owns them
//! (`RobloxPlayerBeta.exe`), oldest first, so window 1 is always the first account that was started.
//! The grid fills the work area (the screen minus the taskbar) of the monitor the first window is on.
//!
//! Two ways to run it:
//! * the "Arrange now" button in the settings window, or "Arrange Roblox windows" in the tray menu;
//! * automatically: a small thread in the tray process (`start_auto_watch`) arranges again whenever the
//!   set of Roblox windows changes, if "Arrange automatically" is on.
//!
//! Roblox must be in windowed mode. A window in exclusive fullscreen (F11) can fight the move.

use std::collections::HashMap;
use std::ffi::c_void;
use std::fs;
use std::thread;
use std::time::Duration;

use crate::lang::pick;
use crate::models::Settings;
use crate::paths;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// What the user chose in the settings page.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// 0 = automatic (a grid close to a square).
    pub columns: u32,
    /// Pixels between windows.
    pub gap: i32,
    /// Hide the title bar and the frame.
    pub borderless: bool,
    /// Only read by the tray's watcher.
    pub auto: bool,
}

impl Config {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            columns: s.layout_columns.min(12),
            gap: s.layout_gap.min(64) as i32,
            borderless: s.layout_borderless,
            auto: s.layout_auto,
        }
    }
}

// ------------------------------------------------------------------ grid maths (no Windows calls)

/// (columns, rows) for `n` windows. `columns == 0` picks a grid close to a square.
pub fn grid_dims(n: usize, columns: u32) -> (usize, usize) {
    if n == 0 {
        return (0, 0);
    }
    let cols = if columns > 0 {
        (columns as usize).min(n)
    } else {
        let mut c = 1;
        while c * c < n {
            c += 1;
        }
        c
    };
    (cols, (n + cols - 1) / cols)
}

/// One rectangle per window, left to right then top to bottom, filling `area` exactly.
pub fn cells(n: usize, columns: u32, area: Rect, gap: i32) -> Vec<Rect> {
    let (cols, rows) = grid_dims(n, columns);
    if cols == 0 {
        return Vec::new();
    }
    let w = (area.right - area.left).max(1) as i64;
    let h = (area.bottom - area.top).max(1) as i64;
    let gap = gap.max(0) as i64;
    let (cols_i, rows_i) = (cols as i64, rows as i64);
    // start(i) and end(i) split the area so that the last cell ends exactly on the edge
    let start = |i: i64, len: i64, parts: i64| (i * (len + gap)) / parts;
    let end = |i: i64, len: i64, parts: i64| ((i + 1) * (len + gap)) / parts - gap;
    (0..n as i64)
        .map(|k| {
            let (c, r) = (k % cols_i, k / cols_i);
            Rect {
                left: area.left + start(c, w, cols_i) as i32,
                top: area.top + start(r, h, rows_i) as i32,
                right: area.left + end(c, w, cols_i) as i32,
                bottom: area.top + end(r, h, rows_i) as i32,
            }
        })
        .collect()
}

// ------------------------------------------------------------------ Win32

#[repr(C)]
struct MonitorInfo {
    cb_size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn IsZoomed(hwnd: isize) -> i32;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;
    fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
    fn GetPropW(hwnd: isize, name: *const u16) -> isize;
    fn SetPropW(hwnd: isize, name: *const u16, data: isize) -> i32;
    fn RemovePropW(hwnd: isize, name: *const u16) -> isize;
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn CloseHandle(handle: isize) -> i32;
    fn QueryFullProcessImageNameW(process: isize, flags: u32, name: *mut u16, size: *mut u32) -> i32;
    fn GetProcessTimes(process: isize, created: *mut FileTime, exited: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
    fn GetModuleHandleW(name: *const u16) -> isize;
    fn GetProcAddress(module: isize, name: *const u8) -> *const c_void;
}

const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const GWL_STYLE: i32 = -16;
const WS_CAPTION: usize = 0x00C0_0000;
const WS_THICKFRAME: usize = 0x0004_0000;
const FRAME: usize = WS_CAPTION | WS_THICKFRAME;
const SW_RESTORE: i32 = 9;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;
const SWP_SHOWWINDOW: u32 = 0x0040;
const SWP_ASYNCWINDOWPOS: u32 = 0x4000;
const MONITOR_DEFAULTTONEAREST: u32 = 2;
/// `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2`
const DPI_PER_MONITOR_V2: isize = -4;
/// Window property that remembers "we removed the frame of this window", so turning the option off
/// puts it back (the property lives on the window, so any Ruststrap process can see it).
const BORDERLESS_PROP: &str = "Ruststrap.Borderless";
/// Ignore tiny windows (splash screens and the like).
const MIN_WIDTH: i32 = 200;
const MIN_HEIGHT: i32 = 150;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Makes this thread's coordinates real pixels on scaled screens, then puts the old mode back.
pub(crate) struct DpiGuard {
    setter: Option<unsafe extern "system" fn(isize) -> isize>,
    previous: isize,
}

impl DpiGuard {
    pub(crate) fn new() -> Self {
        // looked up at run time: the function only exists on Windows 10 1607 and newer
        // SAFETY: plain module and symbol lookups with valid NUL-terminated names; the pointer is only
        // transmuted to the documented signature of SetThreadDpiAwarenessContext.
        unsafe {
            let module = GetModuleHandleW(wide("user32.dll").as_ptr());
            if module == 0 {
                return Self { setter: None, previous: 0 };
            }
            let p = GetProcAddress(module, b"SetThreadDpiAwarenessContext\0".as_ptr());
            if p.is_null() {
                return Self { setter: None, previous: 0 };
            }
            let f: unsafe extern "system" fn(isize) -> isize = std::mem::transmute(p);
            let previous = f(DPI_PER_MONITOR_V2);
            Self { setter: Some(f), previous }
        }
    }
}

impl Drop for DpiGuard {
    fn drop(&mut self) {
        if let Some(f) = self.setter {
            if self.previous != 0 {
                // SAFETY: restores the value the same call returned.
                unsafe {
                    f(self.previous);
                }
            }
        }
    }
}

/// (is this process the Roblox player, when it started as a Windows FILETIME)
fn process_info(pid: u32) -> Option<(bool, u64)> {
    // SAFETY: the handle is closed before returning; the buffers outlive the calls that fill them.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            return None;
        }
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let mut is_roblox = false;
        if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0 {
            let path = String::from_utf16_lossy(&buf[..len as usize]).to_ascii_lowercase();
            is_roblox = path.ends_with("\\robloxplayerbeta.exe");
        }
        let (mut c, mut e, mut k, mut u) = (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
        let created = if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) != 0 {
            ((c.high as u64) << 32) | c.low as u64
        } else {
            0
        };
        CloseHandle(h);
        Some((is_roblox, created))
    }
}

unsafe extern "system" fn collect(hwnd: isize, lparam: isize) -> i32 {
    // SAFETY: `lparam` is the address of the Vec that `top_level_windows` keeps alive during EnumWindows.
    let list = &mut *(lparam as *mut Vec<(isize, u32)>);
    if IsWindowVisible(hwnd) != 0 {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != 0 {
            list.push((hwnd, pid));
        }
    }
    1
}

fn top_level_windows() -> Vec<(isize, u32)> {
    let mut list: Vec<(isize, u32)> = Vec::new();
    // SAFETY: the callback only touches `list`, which lives until EnumWindows returns.
    unsafe {
        EnumWindows(collect, &mut list as *mut _ as isize);
    }
    list
}

/// Visible Roblox windows, the one that started first at the front. One window per Roblox process.
pub fn find_windows() -> Vec<isize> {
    let mut info: HashMap<u32, Option<(bool, u64)>> = HashMap::new();
    // pid -> (window, area, started)
    let mut best: HashMap<u32, (isize, i64, u64)> = HashMap::new();
    for (hwnd, pid) in top_level_windows() {
        let entry = *info.entry(pid).or_insert_with(|| process_info(pid));
        let Some((true, started)) = entry else { continue };
        let mut r = Rect { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: `r` is a valid out-pointer.
        if unsafe { GetWindowRect(hwnd, &mut r) } == 0 {
            continue;
        }
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        // a minimized window is tiny on purpose: it still counts (and is brought back when arranged)
        // SAFETY: plain query on a window handle.
        let iconic = unsafe { IsIconic(hwnd) } != 0;
        if !iconic && (w < MIN_WIDTH || h < MIN_HEIGHT) {
            continue;
        }
        let area = if iconic { 1 } else { w as i64 * h as i64 };
        let e = best.entry(pid).or_insert((hwnd, 0, started));
        if area > e.1 {
            *e = (hwnd, area, started);
        }
    }
    let mut all: Vec<(u64, u32, isize)> = best.into_iter().map(|(pid, (hwnd, _, started))| (started, pid, hwnd)).collect();
    all.sort();
    all.into_iter().map(|(_, _, hwnd)| hwnd).collect()
}

pub fn count() -> usize {
    find_windows().len()
}

fn work_area(hwnd: isize) -> Rect {
    let mut mi = MonitorInfo {
        cb_size: std::mem::size_of::<MonitorInfo>() as u32,
        monitor: Rect { left: 0, top: 0, right: 1280, bottom: 720 },
        work: Rect { left: 0, top: 0, right: 1280, bottom: 720 },
        flags: 0,
    };
    // SAFETY: `mi` has its size set and outlives the call.
    unsafe {
        let m = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if m != 0 && GetMonitorInfoW(m, &mut mi) != 0 {
            return mi.work;
        }
    }
    mi.work
}

fn set_frame(hwnd: isize, borderless: bool) {
    let prop = wide(BORDERLESS_PROP);
    // SAFETY: plain window-style calls on a handle that may have closed meanwhile (then they just fail).
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as usize;
        if borderless {
            if style & FRAME != 0 {
                SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !FRAME) as isize);
                SetPropW(hwnd, prop.as_ptr(), 1);
            }
        } else if GetPropW(hwnd, prop.as_ptr()) != 0 {
            SetWindowLongPtrW(hwnd, GWL_STYLE, (style | FRAME) as isize);
            RemovePropW(hwnd, prop.as_ptr());
        }
    }
}

/// Moves `windows` (in this order) into the grid.
fn apply(windows: &[isize], cfg: &Config) {
    if windows.is_empty() {
        return;
    }
    let _dpi = DpiGuard::new();
    let area = work_area(windows[0]);
    let rects = cells(windows.len(), cfg.columns, area, cfg.gap);
    for (&hwnd, r) in windows.iter().zip(rects) {
        // SAFETY: plain window calls; a window that went away makes them fail, which is fine.
        unsafe {
            if IsIconic(hwnd) != 0 || IsZoomed(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
        }
        set_frame(hwnd, cfg.borderless);
        // SAFETY: as above. The async flag keeps us from waiting on a Roblox window that is busy.
        unsafe {
            SetWindowPos(
                hwnd,
                0,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS,
            );
        }
    }
}

/// Arranges every open Roblox window now. Returns how many were moved.
pub fn arrange(cfg: &Config) -> Result<usize, String> {
    let windows = find_windows();
    if windows.is_empty() {
        return Err(pick("No Roblox window is open.", "Tidak ada jendela Roblox yang terbuka."));
    }
    apply(&windows, cfg);
    Ok(windows.len())
}

// ------------------------------------------------------------------ the tray process

/// The tray process lives for a long time, so it reads the settings file again instead of trusting
/// the copy it loaded at start.
pub(crate) fn disk_settings() -> Option<Settings> {
    let text = fs::read_to_string(paths::get().base.join("Settings.json")).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

/// "Arrange Roblox windows" in the tray menu.
pub fn arrange_from_disk() -> Result<usize, String> {
    let cfg = disk_settings().map(|s| Config::from_settings(&s)).unwrap_or(Config { columns: 0, gap: 0, borderless: false, auto: false });
    arrange(&cfg)
}

/// Starts the thread that arranges again whenever a Roblox window opens or closes (if the user turned
/// "Arrange automatically" on). It waits until the set of windows has stayed the same for two checks so
/// a window that is still starting up is not moved half way.
pub fn start_auto_watch() {
    thread::spawn(|| {
        let mut seen: Vec<isize> = Vec::new();
        let mut arranged: Vec<isize> = Vec::new();
        let mut stable = 0u32;
        loop {
            thread::sleep(Duration::from_millis(1000));
            let now = find_windows();
            if now != seen {
                seen = now;
                stable = 0;
                continue;
            }
            stable = stable.saturating_add(1);
            if stable == 2 && seen != arranged {
                arranged = seen.clone();
                if let Some(s) = disk_settings() {
                    let cfg = Config::from_settings(&s);
                    if cfg.auto {
                        apply(&seen, &cfg);
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1040 };

    #[test]
    fn four_windows_make_two_by_two() {
        assert_eq!(grid_dims(4, 0), (2, 2));
        let c = cells(4, 0, SCREEN, 0);
        assert_eq!(c[0], Rect { left: 0, top: 0, right: 960, bottom: 520 });
        assert_eq!(c[3], Rect { left: 960, top: 520, right: 1920, bottom: 1040 });
    }

    #[test]
    fn three_windows_make_two_columns() {
        assert_eq!(grid_dims(3, 0), (2, 2));
    }

    #[test]
    fn chosen_columns_are_capped_by_window_count() {
        assert_eq!(grid_dims(2, 5), (2, 1));
        assert_eq!(grid_dims(5, 2), (2, 3));
    }

    #[test]
    fn gap_goes_between_cells_only() {
        let c = cells(2, 2, SCREEN, 20);
        assert_eq!(c[0].left, 0);
        assert_eq!(c[1].right, 1920);
        assert_eq!(c[1].left - c[0].right, 20);
    }

    #[test]
    fn no_windows_no_cells() {
        assert!(cells(0, 0, SCREEN, 0).is_empty());
    }
}
