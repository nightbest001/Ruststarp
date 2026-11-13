//! Port of `LaunchSettings.cs`.

use std::collections::HashMap;

use crate::models::LaunchMode;

/// (canonical name, aliases). Every flag is written `-name`, optionally followed by one data value.
const FLAGS: &[(&str, &[&str])] = &[
    ("menu", &["preferences", "menu", "settings"]),
    ("watcher", &["watcher"]),
    ("backgroundupdater", &["backgroundupdater"]),
    ("quiet", &["quiet"]),
    ("uninstall", &["uninstall"]),
    ("install", &["install"]),
    ("nolaunch", &["nolaunch"]),
    ("testmode", &["testmode"]),
    ("nogpu", &["nogpu"]),
    ("upgrade", &["upgrade"]),
    ("player", &["player"]),
    ("studio", &["studio"]),
    ("version", &["version"]),
    ("channel", &["channel"]),
    ("force", &["force"]),
    ("help", &["help", "h", "?"]),
];

pub struct LaunchSettings {
    flags: HashMap<&'static str, Option<String>>,
    pub roblox_launch_mode: LaunchMode,
    pub roblox_launch_args: String,
    pub args: Vec<String>,
}

impl LaunchSettings {
    pub fn parse(args: Vec<String>) -> Self {
        const IDENT: &str = "LaunchSettings::LaunchSettings";
        let mut alias_map: HashMap<&str, &'static str> = HashMap::new();
        for (name, aliases) in FLAGS.iter() {
            for a in aliases.iter() {
                alias_map.insert(*a, *name);
            }
        }

        let mut ls = Self {
            flags: HashMap::new(),
            roblox_launch_mode: LaunchMode::None,
            roblox_launch_args: String::new(),
            args: args.clone(),
        };

        let mut start = 0;
        if let Some(first) = args.first() {
            let lower = first.to_lowercase();
            if lower.starts_with("roblox:") || lower.starts_with("roblox-player:") {
                crate::log!(IDENT, "Got Roblox player argument");
                ls.roblox_launch_mode = LaunchMode::Player;
                ls.roblox_launch_args = first.clone();
                start = 1;
            } else if first.starts_with("version-") {
                crate::log!(IDENT, "Got version argument");
                ls.flags.insert("version", Some(first.clone()));
                start = 1;
            }
        }

        let mut i = start;
        while i < args.len() {
            let arg = &args[i];
            if !arg.starts_with('-') {
                crate::log!(IDENT, "Invalid argument: {arg}");
                i += 1;
                continue;
            }
            let ident = arg.trim_start_matches('-').to_lowercase();
            let Some(&name) = alias_map.get(ident.as_str()) else {
                crate::log!(IDENT, "Unknown argument: {ident}");
                i += 1;
                continue;
            };
            if ls.flags.contains_key(name) {
                crate::log!(IDENT, "Tried to set {ident} flag twice");
                i += 1;
                continue;
            }
            let mut data = None;
            if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                data = Some(args[i + 1].clone());
                i += 1;
                crate::log!(IDENT, "Identifier '{ident}' is active with data");
            } else {
                crate::log!(IDENT, "Identifier '{ident}' is active");
            }
            ls.flags.insert(name, data);
            i += 1;
        }

        if ls.active("version") {
            ls.roblox_launch_mode = LaunchMode::Unknown; // decided by the bootstrapper
        }
        if ls.active("player") {
            ls.parse_player();
        } else if ls.active("studio") {
            ls.parse_studio();
        }
        ls
    }

    pub fn active(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    pub fn data(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(|d| d.as_deref())
    }

    /// Flags that should not trigger an auto-update / install check.
    pub fn bypass_update_check(&self) -> bool {
        self.active("uninstall") || self.active("watcher") || self.active("help")
    }

    fn parse_player(&mut self) {
        self.roblox_launch_mode = LaunchMode::Player;
        if let Some(d) = self.data("player").map(str::to_string) {
            self.roblox_launch_args = d;
        }
    }

    fn parse_studio(&mut self) {
        self.roblox_launch_mode = LaunchMode::Studio;
        let Some(d) = self.data("studio").map(str::to_string) else { return };
        if d.starts_with("roblox-studio:") {
            self.roblox_launch_args = d;
        } else if d.starts_with("roblox-studio-auth:") {
            self.roblox_launch_mode = LaunchMode::StudioAuth;
            self.roblox_launch_args = d;
        } else {
            self.roblox_launch_args = format!("-task EditFile -localPlaceFile \"{d}\"");
        }
    }
}
