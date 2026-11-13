//! Port of `ActivityWatcher.cs` + `ActivityData.cs`: tails the Roblox log and reports join/leave events.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use regex::Regex;

const GAME_JOINING_ENTRY: &str = "[FLog::Output] ! Joining game";
const GAME_TELEPORTING_ENTRY: &str = "[FLog::UgcExperienceController] UgcExperienceController: doTeleport: joinScriptUrl";
const GAME_JOINING_UNIVERSE_ENTRY: &str = "[FLog::GameJoinLoadTime] Report game_join_loadtime:";
const GAME_JOINING_UDMUX_ENTRY: &str = "[FLog::Network] UDMUX Address = ";
const GAME_JOINED_ENTRY: &str = "[FLog::Network] Replicator created: ";
const GAME_DISCONNECTED_ENTRY: &str = "[FLog::Network] Time to disconnect replication data:";
const GAME_LEAVING_ENTRY: &str = "[FLog::SingleSurfaceApp] leaveUGCGameInternal";

const NEW_GAME_PRIVATE_GAME: i32 = 4;
const SPECIFIC_PRIVATE_GAME: i32 = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ServerType {
    #[default]
    Public,
    Private,
    Reserved,
}

#[derive(Clone, Debug, Default)]
pub struct ActivityData {
    pub place_id: i64,
    pub job_id: String,
    pub universe_id: i64,
    pub user_id: i64,
    pub machine_address: String,
    pub server_type: ServerType,
    pub is_teleport: bool,
    pub time_joined: Option<DateTime<Utc>>,
    pub time_left: Option<DateTime<Utc>>,
}

impl ActivityData {
    /// Link that lets someone join the same server (public servers only).
    pub fn invite_deeplink(&self) -> String {
        format!("roblox://experiences/start?placeId={}&gameInstanceId={}", self.place_id, self.job_id)
    }
}

#[derive(Debug)]
pub enum ActivityEvent {
    GameJoin(ActivityData),
    GameLeave(ActivityData),
    AppClose,
}

struct Patterns {
    joining: Regex,
    referral: Regex,
    teleport_join_type: Regex,
    universe: Regex,
    udmux: Regex,
}

impl Patterns {
    fn new() -> Self {
        Self {
            joining: Regex::new(r"! Joining game '([0-9a-f\-]{36})' place ([0-9]+) at ([0-9\.]+)").unwrap(),
            referral: Regex::new(r"referral_page:([^,]+)").unwrap(),
            teleport_join_type: Regex::new(r#"JoinTypeId"%3a(\d+)%2c"#).unwrap(),
            universe: Regex::new(r"universeid:([0-9]+).*userid:([0-9]+)").unwrap(),
            udmux: Regex::new(r"UDMUX Address = ([0-9\.]+), Port = [0-9]+ \| RCC Server Address = ([0-9\.]+), Port = [0-9]+").unwrap(),
        }
    }
}

pub struct ActivityWatcher {
    pat: Patterns,
    in_game: bool,
    data: ActivityData,
    history: Vec<ActivityData>,
    teleport_marker: bool,
    reserved_teleport_marker: bool,
}

impl ActivityWatcher {
    pub fn new() -> Self {
        Self {
            pat: Patterns::new(),
            in_game: false,
            data: ActivityData::default(),
            history: Vec::new(),
            teleport_marker: false,
            reserved_teleport_marker: false,
        }
    }

    /// Blocks until `disposed` is set. Events are passed to `on_event`.
    pub fn run(&mut self, log_file: &Path, disposed: Arc<AtomicBool>, mut on_event: impl FnMut(ActivityEvent)) {
        const IDENT: &str = "ActivityWatcher::Run";
        let deadline = Instant::now() + Duration::from_secs(15);
        let file = loop {
            match OpenOptions::new().read(true).open(log_file) {
                Ok(f) => break f,
                Err(e) => {
                    if Instant::now() > deadline || disposed.load(Ordering::SeqCst) {
                        crate::log!(IDENT, "Cannot open {}: {e}", log_file.display());
                        return;
                    }
                    thread::sleep(Duration::from_millis(500));
                }
            }
        };
        crate::log!(IDENT, "Watching {}", log_file.display());

        let mut reader = BufReader::new(file);
        let mut pending = String::new();
        while !disposed.load(Ordering::SeqCst) {
            let mut chunk = String::new();
            match reader.read_line(&mut chunk) {
                Ok(0) | Err(_) => thread::sleep(Duration::from_millis(500)),
                Ok(_) => {
                    pending.push_str(&chunk);
                    if pending.ends_with('\n') {
                        let line = pending.trim_end_matches(['\r', '\n']).to_string();
                        pending.clear();
                        self.read_entry(&line, &mut on_event);
                    }
                }
            }
        }
    }

    fn read_entry(&mut self, entry: &str, on_event: &mut impl FnMut(ActivityEvent)) {
        const IDENT: &str = "ActivityWatcher::ReadLogEntry";
        let Some(idx) = entry.find(' ') else { return };
        let msg = &entry[idx + 1..];

        if msg.starts_with(GAME_LEAVING_ENTRY) {
            crate::log!(IDENT, "Opening Roblox's leave game confirmation");
            on_event(ActivityEvent::AppClose);
            if self.data.place_id != 0 && !self.in_game {
                self.data = ActivityData::default();
            }
            return;
        }

        if !self.in_game && self.data.place_id == 0 {
            // waiting for a join
            if msg.starts_with(GAME_JOINING_ENTRY) {
                let Some(m) = self.pat.joining.captures(msg) else { return };
                self.in_game = false;
                self.data.place_id = m[2].parse().unwrap_or(0);
                self.data.job_id = m[1].to_string();
                self.data.machine_address = m[3].to_string();
                if self.teleport_marker {
                    self.data.is_teleport = true;
                    self.teleport_marker = false;
                }
                if self.reserved_teleport_marker {
                    self.data.server_type = ServerType::Reserved;
                    self.reserved_teleport_marker = false;
                }
                crate::log!(IDENT, "Joining Game ({}/{}/{})", self.data.place_id, self.data.job_id, self.data.machine_address);
            }
        } else if !self.in_game && self.data.place_id != 0 {
            // joining
            if msg.starts_with(GAME_JOINING_UNIVERSE_ENTRY) {
                let Some(m) = self.pat.universe.captures(msg) else { return };
                self.data.universe_id = m[1].parse().unwrap_or(0);
                self.data.user_id = m[2].parse().unwrap_or(0);
                if let Some(r) = self.pat.referral.captures(msg) {
                    let referral = r[1].to_lowercase();
                    if referral.contains("requestprivategame") || referral.contains("gamedetailpagejshybridevent") {
                        self.data.server_type = ServerType::Private;
                    }
                }
            } else if msg.starts_with(GAME_JOINING_UDMUX_ENTRY) {
                let Some(m) = self.pat.udmux.captures(msg) else { return };
                if m[2] != self.data.machine_address {
                    return;
                }
                self.data.machine_address = m[1].to_string();
                crate::log!(IDENT, "Server is UDMUX protected ({})", self.data.machine_address);
            } else if msg.starts_with(GAME_JOINED_ENTRY) {
                crate::log!(IDENT, "Joined Game ({}/{}/{})", self.data.place_id, self.data.job_id, self.data.machine_address);
                self.in_game = true;
                self.data.time_joined = Some(Utc::now());
                on_event(ActivityEvent::GameJoin(self.data.clone()));
            }
        } else if self.in_game && self.data.place_id != 0 {
            // in a game
            if msg.starts_with(GAME_DISCONNECTED_ENTRY) {
                crate::log!(IDENT, "Disconnected from Game ({}/{}/{})", self.data.place_id, self.data.job_id, self.data.machine_address);
                self.data.time_left = Some(Utc::now());
                let left = std::mem::take(&mut self.data);
                self.history.insert(0, left.clone());
                self.in_game = false;
                on_event(ActivityEvent::GameLeave(left));
            } else if msg.starts_with(GAME_TELEPORTING_ENTRY) {
                crate::log!(IDENT, "Teleport detected");
                self.teleport_marker = true;
                if let Some(c) = self.pat.teleport_join_type.captures(msg) {
                    if let Ok(t) = c[1].parse::<i32>() {
                        if t == NEW_GAME_PRIVATE_GAME || t == SPECIFIC_PRIVATE_GAME {
                            self.reserved_teleport_marker = true;
                        }
                    }
                }
            }
        }
    }
}
