//! Performance overlay: CPU / GPU / RAM as text on a transparent, click-through window that sits on top of
//! the game. It runs as its own small process (`ruststrap -overlay`), started next to Roblox, and closes
//! itself a few seconds after the last Roblox window is gone. `ruststrap -overlay preview` shows it for 8 s.
//!
//! No extra crate: CPU and RAM come from `GetSystemTimes` / `GlobalMemoryStatusEx`, GPU from the same
//! Windows performance counter Task Manager uses (`\GPU Engine(*)\Utilization Percentage`, Windows 10 1709+).

use std::collections::HashMap;
use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::windows::WindowBuilderExtWindows;
use tao::window::WindowBuilder;
use wry::{WebContext, WebViewBuilder};

use crate::{app, paths, singleton};

const IDENT: &str = "Overlay";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PDH_FMT_DOUBLE: u32 = 0x0000_0200;
const PDH_MORE_DATA: u32 = 0x8000_07D2;
const GB: f64 = 1024.0 * 1024.0 * 1024.0;

const HTML: &str = r#"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;background:transparent!important;overflow:hidden;user-select:none;cursor:default}
body{font:600 13px/20px "Segoe UI",sans-serif;color:#fff;padding:4px 6px;
text-shadow:0 0 3px #000,0 1px 2px #000,1px 0 2px #000,-1px 0 2px #000}
.r{display:flex;gap:8px;white-space:nowrap}.k{color:#ff7a33;width:34px}
</style></head><body><div id="o"></div><script>
const C=window.__CFG||{};const o=document.getElementById('o');
const f=(x,d)=>x==null?'--':x.toFixed(d);
const row=(k,v)=>'<div class="r"><span class="k">'+k+'</span><span>'+v+'</span></div>';
window.__stats=s=>{let h='';
if(C.cpu)h+=row('CPU',f(s.cpu,0)+'%');
if(C.gpu)h+=row('GPU',s.gpu_ok===false?'N/A':f(s.gpu,0)+'%');
if(C.ram)h+=row('RAM',f(s.ram_used,1)+' / '+f(s.ram_total,1)+' GB');
o.innerHTML=h};
window.__stats({});
</script></body></html>"#;

// ------------------------------------------------------------------ Win32

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    lo: u32,
    hi: u32,
}

impl FileTime {
    fn v(self) -> u64 {
        ((self.hi as u64) << 32) | self.lo as u64
    }
}

#[repr(C)]
struct MemStatus {
    length: u32,
    load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page: u64,
    avail_page: u64,
    total_virt: u64,
    avail_virt: u64,
    avail_ext: u64,
}

#[repr(C)]
struct PdhValue {
    status: u32,
    value: f64,
}

#[repr(C)]
struct PdhItem {
    name: *const u16,
    value: PdhValue,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
    fn GlobalMemoryStatusEx(status: *mut MemStatus) -> i32;
}

#[link(name = "pdh")]
extern "system" {
    fn PdhOpenQueryW(source: *const u16, user_data: usize, query: *mut isize) -> u32;
    fn PdhAddEnglishCounterW(query: isize, path: *const u16, user_data: usize, counter: *mut isize) -> u32;
    fn PdhCollectQueryData(query: isize) -> u32;
    fn PdhGetFormattedCounterArrayW(counter: isize, format: u32, buf_size: *mut u32, item_count: *mut u32, items: *mut c_void) -> u32;
    fn PdhCloseQuery(query: isize) -> u32;
}

// ------------------------------------------------------------------ samplers

struct Cpu {
    prev: Option<(u64, u64, u64)>,
}

impl Cpu {
    /// Percent busy since the previous call (`None` on the first call).
    fn read(&mut self) -> Option<f64> {
        let (mut i, mut k, mut u) = (FileTime::default(), FileTime::default(), FileTime::default());
        // SAFETY: three valid out pointers.
        if unsafe { GetSystemTimes(&mut i, &mut k, &mut u) } == 0 {
            return None;
        }
        let now = (i.v(), k.v(), u.v());
        let out = self.prev.and_then(|p| {
            let idle = now.0.wrapping_sub(p.0);
            // kernel time already includes idle time
            let total = now.1.wrapping_sub(p.1).saturating_add(now.2.wrapping_sub(p.2));
            if total == 0 {
                None
            } else {
                Some((100.0 * (1.0 - idle as f64 / total as f64)).clamp(0.0, 100.0))
            }
        });
        self.prev = Some(now);
        out
    }
}

/// (used GB, total GB)
fn ram() -> Option<(f64, f64)> {
    let mut m = MemStatus {
        length: std::mem::size_of::<MemStatus>() as u32,
        load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page: 0,
        avail_page: 0,
        total_virt: 0,
        avail_virt: 0,
        avail_ext: 0,
    };
    // SAFETY: `m` is a fully initialised MEMORYSTATUSEX with its length set.
    if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
        return None;
    }
    Some((m.total_phys.saturating_sub(m.avail_phys) as f64 / GB, m.total_phys as f64 / GB))
}

struct Gpu {
    query: isize,
    counter: isize,
}

impl Gpu {
    fn new() -> Option<Self> {
        let path = singleton::wide("\\GPU Engine(*)\\Utilization Percentage");
        let (mut query, mut counter) = (0isize, 0isize);
        // SAFETY: valid out pointers and a NUL-terminated counter path.
        unsafe {
            if PdhOpenQueryW(std::ptr::null(), 0, &mut query) != 0 {
                return None;
            }
            if PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) != 0 {
                PdhCloseQuery(query);
                return None;
            }
            PdhCollectQueryData(query);
        }
        Some(Self { query, counter })
    }

    /// Busiest GPU engine in percent, like Task Manager. `None` while no data is ready.
    fn read(&mut self) -> Option<f64> {
        // SAFETY: the buffer is sized by PDH itself (first call), aligned to 8 bytes, and only read as
        // `count` PdhItem structs whose name pointers point inside the same buffer.
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let (mut size, mut count) = (0u32, 0u32);
            let r = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, std::ptr::null_mut());
            if r != PDH_MORE_DATA || size == 0 {
                return None;
            }
            let mut buf = vec![0u64; (size as usize + 7) / 8];
            let r = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, buf.as_mut_ptr() as *mut c_void);
            if r != 0 {
                return None;
            }
            let items = std::slice::from_raw_parts(buf.as_ptr() as *const PdhItem, count as usize);
            // instance names look like `pid_123_luid_0x0_0xABCD_phys_0_eng_0_engtype_3D`:
            // add up every process per engine, then take the busiest engine
            let mut per_engine: HashMap<String, f64> = HashMap::new();
            for it in items {
                if it.name.is_null() || (it.value.status != 0 && it.value.status != 1) {
                    continue;
                }
                let mut len = 0usize;
                while *it.name.add(len) != 0 {
                    len += 1;
                }
                let name = String::from_utf16_lossy(std::slice::from_raw_parts(it.name, len));
                let key = name.split("_luid_").nth(1).unwrap_or(&name).to_string();
                *per_engine.entry(key).or_insert(0.0) += it.value.value;
            }
            let best = per_engine.values().cloned().fold(0.0f64, f64::max);
            Some(best.clamp(0.0, 100.0))
        }
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        // SAFETY: the query handle came from PdhOpenQueryW and is closed once.
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

fn roblox_running() -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq RobloxPlayerBeta.exe", "/NH", "/FO", "CSV"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains("RobloxPlayerBeta.exe"),
        Err(_) => true, // cannot tell: keep the overlay
    }
}

// ------------------------------------------------------------------ window

enum Ev {
    Eval(String),
    Quit,
}

/// Entry point of `ruststrap -overlay [preview]`.
pub fn run() -> i32 {
    let preview = app::launch().data("overlay") == Some("preview");
    let (want_cpu, want_gpu, want_ram) = {
        let s = app::settings();
        (s.prop.show_cpu, s.prop.show_gpu, s.prop.show_ram)
    };
    if !(want_cpu || want_gpu || want_ram) {
        return 0;
    }
    if !singleton::become_overlay_owner() {
        crate::log!(IDENT, "Another overlay is already running");
        return 0;
    }

    let rows = want_cpu as u32 + want_gpu as u32 + want_ram as u32;
    let mut event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = match WindowBuilder::new()
        .with_title("Ruststrap Overlay")
        .with_inner_size(LogicalSize::new(190.0, rows as f64 * 20.0 + 10.0))
        .with_position(LogicalPosition::new(12.0, 12.0))
        .with_decorations(false)
        .with_undecorated_shadow(false)
        .with_transparent(true)
        .with_always_on_top(true)
        .with_resizable(false)
        .with_focused(false)
        .with_skip_taskbar(true)
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            crate::log!(IDENT, "Window failed: {e}");
            return 1;
        }
    };
    // mouse clicks go to the game, not to the overlay
    let _ = window.set_ignore_cursor_events(true);

    let mut ctx = WebContext::new(Some(paths::get().base.join("WebView2Overlay")));
    let cfg = json!({"cpu": want_cpu, "gpu": want_gpu, "ram": want_ram});
    let webview = match WebViewBuilder::new_with_web_context(&mut ctx)
        .with_transparent(true)
        .with_background_color((0, 0, 0, 0))
        .with_initialization_script(format!("window.__CFG={cfg};"))
        .with_html(HTML)
        .build(&window)
    {
        Ok(v) => v,
        Err(e) => {
            crate::log!(IDENT, "WebView2 failed: {e}");
            return 1;
        }
    };

    thread::spawn(move || {
        let mut cpu = Cpu { prev: None };
        let mut gpu = if want_gpu { Gpu::new() } else { None };
        let gpu_ok = gpu.is_some();
        // first reads only prime the counters
        cpu.read();
        if let Some(g) = gpu.as_mut() {
            g.read();
        }
        let started = Instant::now();
        let mut last_check = Instant::now();
        let mut misses = 0u32;
        loop {
            thread::sleep(Duration::from_millis(1000));
            if preview {
                if started.elapsed() > Duration::from_secs(8) {
                    break;
                }
            } else if last_check.elapsed() >= Duration::from_secs(3) {
                last_check = Instant::now();
                if roblox_running() {
                    misses = 0;
                } else {
                    misses += 1;
                    if misses >= 2 {
                        break;
                    }
                }
            }
            let c = if want_cpu { cpu.read() } else { None };
            let g = gpu.as_mut().and_then(|g| g.read());
            let r = if want_ram { ram() } else { None };
            let stats = json!({
                "cpu": c,
                "gpu": g,
                "gpu_ok": gpu_ok,
                "ram_used": r.map(|x| x.0),
                "ram_total": r.map(|x| x.1),
            });
            if proxy.send_event(Ev::Eval(format!("window.__stats&&window.__stats({stats})"))).is_err() {
                return;
            }
        }
        let _ = proxy.send_event(Ev::Quit);
    });

    event_loop.run_return(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let _keep_alive = &window;
        if let Event::UserEvent(ev) = event {
            match ev {
                Ev::Eval(js) => {
                    let _ = webview.evaluate_script(&js);
                }
                Ev::Quit => *flow = ControlFlow::Exit,
            }
        }
    });
    crate::log!(IDENT, "Overlay closed");
    0
}
