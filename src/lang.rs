//! Language for the parts written in Rust: native dialogs, update errors and the launch window's messages.
//! The page has its own tables (`ui-web/strings.json` and `strings.id.json`). The choice is `Settings.language`:
//! "system" follows the Windows display language, "en" and "id" force one.

use std::sync::atomic::{AtomicBool, Ordering};

static INDONESIAN: AtomicBool = AtomicBool::new(false);

extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

/// True when Windows itself is shown in Indonesian (primary language id 0x21).
pub fn system_is_indonesian() -> bool {
    // SAFETY: plain Win32 call without arguments.
    unsafe { (GetUserDefaultUILanguage() & 0x3ff) == 0x21 }
}

pub fn init(setting: &str) {
    let id = match setting {
        "id" => true,
        "en" => false,
        _ => system_is_indonesian(),
    };
    INDONESIAN.store(id, Ordering::Relaxed);
}

pub fn is_indonesian() -> bool {
    INDONESIAN.load(Ordering::Relaxed)
}

/// `pick("English", "Indonesia")` gives the text for the language in use.
pub fn pick(en: &str, id: &str) -> String {
    let s = if is_indonesian() { id } else { en };
    s.to_string()
}
