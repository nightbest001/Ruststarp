//! Port of `Logger.cs`.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

struct Inner {
    file: Option<File>,
}

static LOGGER: OnceLock<Mutex<Inner>> = OnceLock::new();

fn inner() -> &'static Mutex<Inner> {
    LOGGER.get_or_init(|| Mutex::new(Inner { file: None }))
}

#[macro_export]
macro_rules! log {
    ($ident:expr, $($arg:tt)*) => {
        $crate::logger::write_line($ident, &format!($($arg)*))
    };
}

pub fn initialize(dir: &Path, prefix: &str) {
    let _ = fs::create_dir_all(dir);
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = dir.join(format!("{prefix}_{stamp}.log"));
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(&path) {
        inner().lock().unwrap().file = Some(f);
    }
    // keep only the 20 newest logs
    if let Ok(rd) = fs::read_dir(dir) {
        let mut logs: Vec<_> = rd.flatten().filter(|e| e.path().extension().map_or(false, |x| x == "log")).collect();
        logs.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        while logs.len() > 20 {
            let _ = fs::remove_file(logs.remove(0).path());
        }
    }
}

/// Hide the user's profile path and Roblox auth cookies from log files.
fn redact(s: &str) -> String {
    let mut out = s.to_string();
    let profile = crate::paths::user_profile().to_string_lossy().to_string();
    if !profile.is_empty() {
        out = out.replace(&profile, "%UserProfile%");
    }
    if let Ok(re) = regex::Regex::new(r"_\|WARNING:[^;\s]+") {
        out = re.replace_all(&out, "<redacted>").to_string();
    }
    out
}

pub fn write_line(ident: &str, message: &str) {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let line = redact(&format!("{ts} [{ident}] {message}"));
    if std::env::var_os("RUSTSTRAP_DEBUG").is_some() {
        eprintln!("{line}");
    }
    if let Some(f) = inner().lock().unwrap().file.as_mut() {
        let _ = writeln!(f, "{line}");
    }
}

pub fn write_exception(ident: &str, err: &dyn std::fmt::Display) {
    write_line(ident, &format!("(Exception) {err}"));
}
