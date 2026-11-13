//! Ruststrap is a GUI-subsystem program, so it has no console of its own. When it is started
//! from cmd/PowerShell with a command (`ruststrap flags list`), borrow that console so the
//! text output still shows up.

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::os::windows::io::AsRawHandle;

const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
const STD_INPUT_HANDLE: u32 = -10i32 as u32;
const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
const STD_ERROR_HANDLE: u32 = -12i32 as u32;

extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
    fn SetStdHandle(std_handle: u32, handle: *mut c_void) -> i32;
}

pub fn attach_parent() {
    // SAFETY: plain Win32 calls; the opened files are leaked on purpose so the handles stay valid.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return; // no parent console (started by double-click): nothing to print to
        }
        if let Ok(out) = OpenOptions::new().write(true).open("CONOUT$") {
            let h = out.as_raw_handle();
            SetStdHandle(STD_OUTPUT_HANDLE, h as *mut c_void);
            SetStdHandle(STD_ERROR_HANDLE, h as *mut c_void);
            std::mem::forget(out);
        }
        if let Ok(inp) = OpenOptions::new().read(true).write(true).open("CONIN$") {
            SetStdHandle(STD_INPUT_HANDLE, inp.as_raw_handle() as *mut c_void);
            std::mem::forget(inp);
        }
    }
}
