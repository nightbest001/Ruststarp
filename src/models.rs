//! Data models (ports of everything under `Models/` that the core needs).

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LaunchMode {
    None,
    Unknown,
    Player,
    Studio,
    StudioAuth,
}

// ------------------------------------------------------------------ persisted

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct CustomIntegration {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Location")]
    pub location: String,
    #[serde(rename = "LaunchArgs")]
    pub launch_args: String,
    #[serde(rename = "AutoClose")]
    pub auto_close: bool,
}

impl Default for CustomIntegration {
    fn default() -> Self {
        Self { name: String::new(), location: String::new(), launch_args: String::new(), auto_close: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase", default)]
pub struct Settings {
    pub check_for_updates: bool,
    pub confirm_launches: bool,
    pub use_fast_flag_manager: bool,
    pub enable_activity_tracking: bool,
    pub use_disable_app_patch: bool,
    /// Discord Rich Presence (needs activity tracking).
    pub use_discord_rich_presence: bool,
    /// Shows a "Join server" button on the presence.
    pub allow_activity_joining: bool,
    pub show_account_on_rich_presence: bool,
    /// Keep Ruststrap in the system tray after it starts Roblox.
    pub keep_in_tray: bool,
    /// Let several Roblox windows run at once (Multi Account).
    pub multi_instance: bool,
    /// Roblox window layout (see `layout.rs`): columns (0 = automatic), gap in pixels, no title bar,
    /// and arrange again by itself when a window opens or closes.
    pub layout_columns: u32,
    pub layout_gap: u32,
    pub layout_borderless: bool,
    pub layout_auto: bool,
    /// Max alts (see `max_alts.rs`): Roblox windows that are not in front get Idle priority and efficiency
    /// mode; `max_alts_trim` also takes back their unused memory.
    pub max_alts: bool,
    pub max_alts_trim: bool,
    /// Anti-AFK (see `anti_afk.rs`): every `anti_afk_minutes` each background Roblox window gets one small
    /// input; `anti_afk_action` is "nudge" | "space" | "zoom".
    pub anti_afk: bool,
    pub anti_afk_minutes: u32,
    pub anti_afk_action: String,
    /// Performance overlay on top of the game (see `overlay.rs`).
    pub show_cpu: bool,
    pub show_gpu: bool,
    pub show_ram: bool,
    pub show_fps: bool,
    /// Last game used in Multi Account (not edited by the settings page).
    pub account_place: String,
    pub account_job: String,
    /// Player Multi Account follows (empty = not following).
    pub account_follow: String,
    /// "default" | "saving" | "high"; see `gpu.rs`.
    pub gpu_preference: String,
    /// "default" | "above" | "high"; see `boost.rs`.
    pub process_priority: String,
    /// Areas the RAM cleaner tab cleans (bit mask, see `memory.rs`).
    pub memory_mask: u32,
    /// Keep Roblox off CPU core 0 (needs more than 2 cores).
    pub skip_core0: bool,
    /// "system" | "light" | "dark"
    pub theme: String,
    /// "system" | "en" | "id"; see `lang.rs`.
    pub language: String,
    /// Text shown as the title of the launch window.
    pub bootstrapper_title: String,
    pub debug_disable_version_package_cleanup: bool,
    pub custom_integrations: Vec<CustomIntegration>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            check_for_updates: true,
            confirm_launches: false,
            use_fast_flag_manager: true,
            enable_activity_tracking: true,
            use_disable_app_patch: false,
            use_discord_rich_presence: true,
            allow_activity_joining: true,
            show_account_on_rich_presence: false,
            keep_in_tray: true,
            multi_instance: true,
            layout_columns: 0,
            layout_gap: 0,
            layout_borderless: false,
            layout_auto: false,
            max_alts: false,
            max_alts_trim: false,
            anti_afk: false,
            anti_afk_minutes: 12,
            anti_afk_action: "nudge".to_string(),
            show_cpu: false,
            show_gpu: false,
            show_ram: false,
            show_fps: false,
            account_place: String::new(),
            account_job: String::new(),
            account_follow: String::new(),
            gpu_preference: "default".to_string(),
            process_priority: "default".to_string(),
            skip_core0: false,
            memory_mask: crate::memory::DEFAULT_MASK,
            theme: "system".to_string(),
            language: "system".to_string(),
            bootstrapper_title: crate::app::PROJECT_NAME.to_string(),
            debug_disable_version_package_cleanup: false,
            custom_integrations: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase", default)]
pub struct State {
    pub prompt_web_view2_install: bool,
    pub force_reinstall: bool,
}

impl Default for State {
    fn default() -> Self {
        Self { prompt_web_view2_install: true, force_reinstall: false }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct DistributionState {
    pub version_guid: String,
    pub package_hashes: HashMap<String, String>,
    pub size: i64,
    pub mod_manifest: Vec<String>,
}

/// `ClientAppSettings.json`: flag name -> value (always written as strings).
pub type FastFlags = BTreeMap<String, serde_json::Value>;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct WatcherData {
    pub process_id: u32,
    pub log_file: Option<String>,
    pub autoclose_pids: Vec<u32>,
}

// ------------------------------------------------------------------ roblox api

#[derive(Deserialize, Clone, Debug)]
pub struct ClientVersion {
    pub version: String,
    #[serde(rename = "clientVersionUpload")]
    pub version_guid: String,
    #[serde(rename = "bootstrapperVersion", default)]
    pub bootstrapper_version: String,
}

// ------------------------------------------------------------------ manifest

#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    pub signature: String,
    pub packed_size: u64,
    pub size: u64,
}

impl Package {
    pub fn download_path(&self) -> std::path::PathBuf {
        crate::paths::get().downloads.join(&self.signature)
    }
}

/// Port of `PackageManifest.cs`.
pub fn parse_package_manifest(data: &str) -> Result<Vec<Package>, String> {
    let mut lines = data.lines();
    let version = lines.next().unwrap_or("");
    if version != "v0" {
        return Err(format!("Unexpected package manifest version: {version} (expected v0!)"));
    }
    let mut out = Vec::new();
    loop {
        let name = lines.next().unwrap_or("");
        let sig = lines.next().unwrap_or("");
        let packed = lines.next().unwrap_or("");
        let size = lines.next().unwrap_or("");
        if name.is_empty() || sig.is_empty() || packed.is_empty() || size.is_empty() {
            break;
        }
        if name == "RobloxPlayerLauncher.exe" {
            break;
        }
        out.push(Package {
            name: name.to_string(),
            signature: sig.to_string(),
            packed_size: packed.trim().parse().map_err(|_| format!("bad packed size: {packed}"))?,
            size: size.trim().parse().map_err(|_| format!("bad size: {size}"))?,
        });
    }
    Ok(out)
}
