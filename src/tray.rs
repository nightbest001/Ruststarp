//! The system tray icon. It lives in its own hidden process (`ruststrap -tray`), so it stays after the
//! launch window closes. The same process holds the Roblox mutex, which is what lets several Roblox
//! windows run at once (Multi Account).
//!
//! Plain Win32 (a hidden window plus `Shell_NotifyIconW`), so no extra crate and no manifest is needed.

use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::lang::pick;
use crate::{app, paths, singleton};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const WM_CLOSE: u32 = 0x0010;
const WM_DESTROY: u32 = 0x0002;
const WM_NULL: u32 = 0x0000;
const WM_USER: u32 = 0x0400;
const WM_TRAY: u32 = WM_USER + 1;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_RBUTTONUP: u32 = 0x0205;

const NIM_ADD: u32 = 0;
const NIM_DELETE: u32 = 2;
const NIF_MESSAGE: u32 = 0x1;
const NIF_ICON: u32 = 0x2;
const NIF_TIP: u32 = 0x4;

const MF_STRING: u32 = 0x0;
const MF_SEPARATOR: u32 = 0x800;
const TPM_RIGHTBUTTON: u32 = 0x2;
const TPM_RETURNCMD: u32 = 0x100;

const IDI_APPLICATION: usize = 32512;
const CLASS_NAME: &str = "RuststrapTrayWindow";

const CMD_OPEN: usize = 1;
const CMD_ACCOUNTS: usize = 2;
const CMD_CLOSE_ROBLOX: usize = 3;
const CMD_EXIT: usize = 4;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: Point,
    private: u32,
}

#[repr(C)]
struct WndClassW {
    style: u32,
    wnd_proc: Option<unsafe extern "system" fn(isize, u32, usize, isize) -> isize>,
    cls_extra: i32,
    wnd_extra: i32,
    instance: isize,
    icon: isize,
    cursor: isize,
    background: isize,
    menu_name: *const u16,
    class_name: *const u16,
}

#[repr(C)]
struct NotifyIconData {
    cb_size: u32,
    hwnd: isize,
    id: u32,
    flags: u32,
    callback_message: u32,
    icon: isize,
    tip: [u16; 128],
    state: u32,
    state_mask: u32,
    info: [u16; 256],
    version: u32,
    info_title: [u16; 64],
    info_flags: u32,
    guid: [u32; 4],
    balloon_icon: isize,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(class: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: isize,
        menu: isize,
        instance: isize,
        param: *mut c_void,
    ) -> isize;
    fn DefWindowProcW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn PostQuitMessage(code: i32);
    fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn DestroyWindow(hwnd: isize) -> i32;
    fn FindWindowW(class_name: *const u16, window_name: *const u16) -> isize;
    fn LoadIconW(instance: isize, name: *const u16) -> isize;
    fn CreatePopupMenu() -> isize;
    fn AppendMenuW(menu: isize, flags: u32, id: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(menu: isize, flags: u32, x: i32, y: i32, reserved: i32, hwnd: isize, rect: *const c_void) -> i32;
    fn DestroyMenu(menu: isize) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn RegisterWindowMessageW(name: *const u16) -> u32;
}

#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(message: u32, data: *const NotifyIconData) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

fn wide(s: &str) -> Vec<u16> {
    singleton::wide(s)
}

fn load_icon() -> isize {
    // SAFETY: resource id 1 is the icon `build.rs` embeds; the fallback is a stock icon.
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let h = LoadIconW(inst, 1usize as *const u16);
        if h != 0 {
            h
        } else {
            LoadIconW(0, IDI_APPLICATION as *const u16)
        }
    }
}

fn icon_data(hwnd: isize) -> NotifyIconData {
    let mut tip = [0u16; 128];
    for (i, c) in "Ruststrap".encode_utf16().take(127).enumerate() {
        tip[i] = c;
    }
    NotifyIconData {
        cb_size: std::mem::size_of::<NotifyIconData>() as u32,
        hwnd,
        id: 1,
        flags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        callback_message: WM_TRAY,
        icon: load_icon(),
        tip,
        state: 0,
        state_mask: 0,
        info: [0; 256],
        version: 0,
        info_title: [0; 64],
        info_flags: 0,
        guid: [0; 4],
        balloon_icon: 0,
    }
}

fn add_icon(hwnd: isize) {
    let d = icon_data(hwnd);
    // SAFETY: `d` is a fully initialised NOTIFYICONDATAW.
    unsafe {
        Shell_NotifyIconW(NIM_ADD, &d);
    }
}

fn remove_icon(hwnd: isize) {
    let d = icon_data(hwnd);
    // SAFETY: as above.
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &d);
    }
}

fn open_ruststrap(args: &[&str]) {
    let _ = Command::new(paths::process()).args(args).spawn();
}

fn close_roblox() {
    let _ = Command::new("taskkill").args(["/F", "/IM", "RobloxPlayerBeta.exe"]).creation_flags(CREATE_NO_WINDOW).output();
}

fn show_menu(hwnd: isize) {
    let items: [(usize, String); 3] = [
        (CMD_OPEN, pick("Open Ruststrap", "Buka Ruststrap")),
        (CMD_ACCOUNTS, pick("Multi Account", "Multi Akun")),
        (CMD_CLOSE_ROBLOX, pick("Close all Roblox windows", "Tutup semua jendela Roblox")),
    ];
    let exit = pick("Exit", "Keluar");
    // SAFETY: plain Win32 menu calls; every string outlives the call that uses it.
    let cmd = unsafe {
        let menu = CreatePopupMenu();
        if menu == 0 {
            return;
        }
        for (id, text) in &items {
            let w = wide(text);
            AppendMenuW(menu, MF_STRING, *id, w.as_ptr());
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        let w = wide(&exit);
        AppendMenuW(menu, MF_STRING, CMD_EXIT, w.as_ptr());

        let mut pt = Point { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // the menu only closes when you click elsewhere if our window is in front first
        SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, pt.x, pt.y, 0, hwnd, std::ptr::null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        chosen as usize
    };
    match cmd {
        CMD_OPEN => open_ruststrap(&[]),
        CMD_ACCOUNTS => open_ruststrap(&["-menu", "accounts"]),
        CMD_CLOSE_ROBLOX => close_roblox(),
        CMD_EXIT => {
            // SAFETY: `hwnd` is our own window.
            unsafe {
                DestroyWindow(hwnd);
            }
        }
        _ => {}
    }
}

unsafe extern "system" fn wnd_proc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize {
    if msg == WM_TRAY {
        match lparam as u32 {
            WM_RBUTTONUP | WM_LBUTTONUP => show_menu(hwnd),
            WM_LBUTTONDBLCLK => open_ruststrap(&[]),
            _ => {}
        }
        return 0;
    }
    if msg == WM_DESTROY {
        remove_icon(hwnd);
        PostQuitMessage(0);
        return 0;
    }
    let created = TASKBAR_CREATED.load(Ordering::Relaxed);
    if created != 0 && msg == created {
        // Explorer restarted: its tray is empty again
        add_icon(hwnd);
        return 0;
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Entry point of `ruststrap -tray`: shows the icon and runs until "Exit" is chosen.
pub fn run() -> i32 {
    const IDENT: &str = "Tray::Run";
    if !singleton::become_tray_owner() {
        crate::log!(IDENT, "Another tray process is already running");
        return 0;
    }
    if app::settings().prop.multi_instance {
        singleton::hold_roblox_mutex();
    }

    let class = wide(CLASS_NAME);
    let title = wide("Ruststrap");
    // SAFETY: standard window-class registration and message loop on this thread; all pointers stay valid.
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);
        let wc = WndClassW {
            style: 0,
            wnd_proc: Some(wnd_proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance: inst,
            icon: 0,
            cursor: 0,
            background: 0,
            menu_name: std::ptr::null(),
            class_name: class.as_ptr(),
        };
        if RegisterClassW(&wc) == 0 {
            crate::log!(IDENT, "RegisterClassW failed");
            return 1;
        }
        // never shown: it only receives the tray icon's messages
        let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), 0, 0, 0, 0, 0, 0, 0, inst, std::ptr::null_mut());
        if hwnd == 0 {
            crate::log!(IDENT, "CreateWindowExW failed");
            return 1;
        }
        add_icon(hwnd);
        crate::log!(IDENT, "Tray icon is up");

        let mut msg = Msg { hwnd: 0, message: 0, wparam: 0, lparam: 0, time: 0, pt: Point { x: 0, y: 0 }, private: 0 };
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    crate::log!(IDENT, "Tray closed");
    0
}

/// Called before Roblox starts: makes sure the tray process is up (and, for Multi Account, that the
/// Roblox mutex is held) so the process that started Roblox can exit without losing either.
pub fn ensure_running() {
    const IDENT: &str = "Tray::EnsureRunning";
    let (keep, multi) = {
        let s = app::settings();
        (s.prop.keep_in_tray, s.prop.multi_instance)
    };
    if multi {
        // ours until the tray process has taken it over
        singleton::hold_roblox_mutex();
    }
    if !keep && !multi {
        return;
    }
    if singleton::tray_is_running() {
        return;
    }
    if let Err(e) = Command::new(paths::process()).arg("-tray").creation_flags(CREATE_NO_WINDOW).spawn() {
        crate::log!(IDENT, "Failed to start the tray: {e}");
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !singleton::tray_is_running() {
        thread::sleep(Duration::from_millis(100));
    }
}

/// Closes a running tray process (before the exe is replaced or removed) and waits for it to go.
pub fn stop_others() {
    if !singleton::tray_is_running() {
        return;
    }
    let class = wide(CLASS_NAME);
    // SAFETY: FindWindowW and PostMessageW take plain values and a valid NUL-terminated name.
    unsafe {
        let w = FindWindowW(class.as_ptr(), std::ptr::null());
        if w != 0 {
            PostMessageW(w, WM_CLOSE, 0, 0);
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && singleton::tray_is_running() {
        thread::sleep(Duration::from_millis(100));
    }
}
