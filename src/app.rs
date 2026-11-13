//! Port of `App.xaml.cs` (globals only; startup lives in `main.rs`).

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use crate::json_manager::JsonManager;
use crate::launch_settings::LaunchSettings;
use crate::models::{DistributionState, FastFlags, Settings, State};
use crate::paths;

pub const PROJECT_NAME: &str = "Ruststrap";
pub const PROJECT_REPOSITORY: &str = "nightbest001/Ruststarp";
pub const PROJECT_HELP_LINK: &str = "https://github.com/nightbest001/Ruststarp/wiki";
pub const PROJECT_SUPPORT_LINK: &str = "https://github.com/nightbest001/Ruststarp/issues/new";
pub const PROJECT_DOWNLOAD_LINK: &str = "https://github.com/nightbest001/Ruststarp";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Ruststrap";

pub mod error_code {
    pub const SUCCESS: i32 = 0;
    pub const FILE_NOT_FOUND: i32 = 2;
    pub const CANCELLED: i32 = 1223;
    pub const INSTALL_USER_EXIT: i32 = 1602;
    pub const INSTALL_FAILURE: i32 = 1603;
}

static SETTINGS: OnceLock<Mutex<JsonManager<Settings>>> = OnceLock::new();
static STATE: OnceLock<Mutex<JsonManager<State>>> = OnceLock::new();
static PLAYER_STATE: OnceLock<Mutex<JsonManager<DistributionState>>> = OnceLock::new();
static STUDIO_STATE: OnceLock<Mutex<JsonManager<DistributionState>>> = OnceLock::new();
static FAST_FLAGS: OnceLock<Mutex<JsonManager<FastFlags>>> = OnceLock::new();
static LAUNCH: OnceLock<LaunchSettings> = OnceLock::new();
static HTTP: OnceLock<reqwest::blocking::Client> = OnceLock::new();

/// Load every persisted file. Call after `paths::init`.
pub fn load_all() {
    let p = paths::get();
    let mut s = JsonManager::<Settings>::new("Settings", p.base.join("Settings.json"));
    s.load();
    let mut st = JsonManager::<State>::new("State", p.base.join("State.json"));
    st.load();
    let mut ps = JsonManager::<DistributionState>::new("PlayerState", p.base.join("PlayerState.json"));
    ps.load();
    let mut ss = JsonManager::<DistributionState>::new("StudioState", p.base.join("StudioState.json"));
    ss.load();
    let mut ff = JsonManager::<FastFlags>::new(
        "FastFlagManager",
        p.modifications.join("ClientSettings").join("ClientAppSettings.json"),
    );
    ff.load();
    let _ = SETTINGS.set(Mutex::new(s));
    let _ = STATE.set(Mutex::new(st));
    let _ = PLAYER_STATE.set(Mutex::new(ps));
    let _ = STUDIO_STATE.set(Mutex::new(ss));
    let _ = FAST_FLAGS.set(Mutex::new(ff));
}

pub fn settings() -> MutexGuard<'static, JsonManager<Settings>> {
    SETTINGS.get().expect("load_all not called").lock().unwrap()
}
pub fn state() -> MutexGuard<'static, JsonManager<State>> {
    STATE.get().expect("load_all not called").lock().unwrap()
}
pub fn player_state() -> MutexGuard<'static, JsonManager<DistributionState>> {
    PLAYER_STATE.get().expect("load_all not called").lock().unwrap()
}
pub fn studio_state() -> MutexGuard<'static, JsonManager<DistributionState>> {
    STUDIO_STATE.get().expect("load_all not called").lock().unwrap()
}
pub fn fast_flags() -> MutexGuard<'static, JsonManager<FastFlags>> {
    FAST_FLAGS.get().expect("load_all not called").lock().unwrap()
}

pub fn dist_state(studio: bool) -> MutexGuard<'static, JsonManager<DistributionState>> {
    if studio {
        studio_state()
    } else {
        player_state()
    }
}

pub fn set_launch(ls: LaunchSettings) {
    let _ = LAUNCH.set(ls);
}
pub fn launch() -> &'static LaunchSettings {
    LAUNCH.get().expect("launch settings not set")
}

pub fn http() -> &'static reqwest::blocking::Client {
    HTTP.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent(format!("{PROJECT_NAME}/{VERSION} ({PROJECT_REPOSITORY})"))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build http client")
    })
}

/// Print to the console and the log.
pub fn say(msg: &str) {
    crate::progress::set_message(msg);
    println!("{msg}");
    crate::logger::write_line("App", msg);
}

pub fn terminate(code: i32) -> ! {
    crate::log!("App::Terminate", "Terminating with exit code {code}");
    std::process::exit(code)
}

/// Used for `BTreeMap` flag helpers.
pub type FlagMap = BTreeMap<String, serde_json::Value>;
