//! Port of `Watcher.cs`: a background process that outlives the launcher while Roblox runs.

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use base64::Engine;

use crate::activity_watcher::{ActivityEvent, ActivityWatcher};
use crate::app;
use crate::models::WatcherData;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn process_alive(pid: u32) -> bool {
    let filter = format!("PID eq {pid}");
    let out = Command::new("tasklist")
        .args(["/FI", filter.as_str(), "/NH", "/FO", "CSV"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")),
        Err(_) => false,
    }
}

pub fn close_process(pid: u32, force: bool) {
    let pid_text = pid.to_string();
    let mut cmd = Command::new("taskkill");
    cmd.args(["/PID", pid_text.as_str()]);
    if force {
        cmd.arg("/F");
    }
    let _ = cmd.creation_flags(CREATE_NO_WINDOW).output();
}

pub fn run() -> i32 {
    const IDENT: &str = "Watcher";
    let Some(arg) = app::launch().data("watcher").map(str::to_string) else {
        eprintln!("watcher data not specified");
        return 1;
    };
    let data: WatcherData = match base64::engine::general_purpose::STANDARD
        .decode(arg.trim())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
    {
        Some(d) => d,
        None => {
            crate::log!(IDENT, "Watcher data is invalid");
            return 1;
        }
    };

    let (tracking, close_on_leave, use_rpc) = {
        let s = app::settings();
        (s.prop.enable_activity_tracking, s.prop.use_disable_app_patch, s.prop.use_discord_rich_presence)
    };

    let disposed = Arc::new(AtomicBool::new(false));
    let mut handle = None;
    if tracking {
        if let Some(log_file) = data.log_file.clone() {
            let d = disposed.clone();
            let pid = data.process_id;
            handle = Some(thread::spawn(move || {
                let mut w = ActivityWatcher::new();
                let mut rpc = if use_rpc { Some(crate::discord::DiscordRpc::new()) } else { None };
                w.run(std::path::Path::new(&log_file), d, |ev| match ev {
                    ActivityEvent::AppClose => {
                        if close_on_leave {
                            close_process(pid, false);
                        }
                    }
                    ActivityEvent::GameJoin(a) => {
                        crate::log!(IDENT, "Joined place {} (job {})", a.place_id, a.job_id);
                        if let Some(r) = rpc.as_mut() {
                            r.on_join(&a);
                        }
                    }
                    ActivityEvent::GameLeave(a) => {
                        crate::log!(IDENT, "Left place {}", a.place_id);
                        if let Some(r) = rpc.as_mut() {
                            r.on_leave();
                        }
                    }
                });
                if let Some(r) = rpc.as_mut() {
                    r.shutdown();
                }
            }));
        }
    }

    while process_alive(data.process_id) {
        thread::sleep(Duration::from_secs(1));
    }
    crate::log!(IDENT, "Roblox (PID {}) exited", data.process_id);

    disposed.store(true, Ordering::SeqCst);
    if let Some(h) = handle {
        let _ = h.join();
    }
    for pid in &data.autoclose_pids {
        close_process(*pid, false);
    }
    0
}
