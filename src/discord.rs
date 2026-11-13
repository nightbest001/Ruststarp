//! Discord Rich Presence (port of the useful part of `DiscordRichPresence.cs`).
//!
//! Runs on the watcher thread, so everything here is plain blocking code. A Discord that is not running
//! is not an error: the presence is simply skipped, and we try again on the next game join.

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use serde_json::Value;

use crate::activity_watcher::{ActivityData, ServerType};
use crate::app;

/// Discord application whose name shows as "Playing ..." and which owns the `roblox` image asset.
/// Replace it with your own application ID from <https://discord.com/developers/applications>
/// (upload a `roblox` image under Rich Presence > Art Assets) if you want your own name there.
const CLIENT_ID: &str = "1005469189907173486";

fn get_json(url: &str) -> Option<Value> {
    let resp = app::http().get(url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<Value>().ok()
}

/// `data[0]` of a Roblox list response.
fn first(v: &Value) -> Option<&Value> {
    v.get("data")?.as_array()?.first()
}

struct Universe {
    name: String,
    creator: String,
    verified: bool,
    icon: Option<String>,
}

fn universe_details(universe_id: i64) -> Option<Universe> {
    let games = get_json(&format!("https://games.roblox.com/v1/games?universeIds={universe_id}"))?;
    let g = first(&games)?;
    let name = g.get("name")?.as_str()?.to_string();
    let creator = g.get("creator");
    let creator_name = creator.and_then(|c| c.get("name")).and_then(Value::as_str).unwrap_or("").to_string();
    let verified = creator.and_then(|c| c.get("hasVerifiedBadge")).and_then(Value::as_bool).unwrap_or(false);

    let icon = get_json(&format!(
        "https://thumbnails.roblox.com/v1/games/icons?universeIds={universe_id}&returnPolicy=PlaceHolder&size=128x128&format=Png&isCircular=false"
    ))
    .and_then(|v| first(&v).and_then(|t| t.get("imageUrl")).and_then(Value::as_str).map(str::to_string));

    Some(Universe { name, creator: creator_name, verified, icon })
}

/// (headshot url, hover text) of the account that is playing.
fn user_details(user_id: i64) -> Option<(Option<String>, String)> {
    let user = get_json(&format!("https://users.roblox.com/v1/users/{user_id}"))?;
    let name = user.get("name")?.as_str()?;
    let display = user.get("displayName").and_then(Value::as_str).unwrap_or(name);
    let image = get_json(&format!(
        "https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds={user_id}&size=48x48&format=Png&isCircular=false"
    ))
    .and_then(|v| first(&v).and_then(|t| t.get("imageUrl")).and_then(Value::as_str).map(str::to_string));
    Some((image, format!("Playing on {display} (@{name})")))
}

#[derive(Clone, Default)]
struct Presence {
    details: String,
    state: String,
    start: i64,
    large_image: String,
    large_text: String,
    small_image: String,
    small_text: String,
    join_url: Option<String>,
    game_url: String,
}

pub struct DiscordRpc {
    client: Option<DiscordIpcClient>,
}

impl DiscordRpc {
    pub fn new() -> Self {
        Self { client: None }
    }

    fn connect(&mut self) -> bool {
        const IDENT: &str = "DiscordRpc::Connect";
        if self.client.is_some() {
            return true;
        }
        let Ok(mut c) = DiscordIpcClient::new(CLIENT_ID) else { return false };
        if c.connect().is_err() {
            crate::log!(IDENT, "Discord is not running, skipping rich presence");
            return false;
        }
        crate::log!(IDENT, "Connected to Discord");
        self.client = Some(c);
        true
    }

    fn send(&mut self, p: &Presence) {
        const IDENT: &str = "DiscordRpc::Send";
        if !self.connect() {
            return;
        }

        let mut assets = activity::Assets::new();
        if !p.large_image.is_empty() {
            assets = assets.large_image(&p.large_image).large_text(&p.large_text);
        }
        if !p.small_image.is_empty() {
            assets = assets.small_image(&p.small_image).small_text(&p.small_text);
        }

        let mut buttons = Vec::new();
        if let Some(url) = &p.join_url {
            buttons.push(activity::Button::new("Join server", url));
        }
        buttons.push(activity::Button::new("See game page", &p.game_url));

        let act = activity::Activity::new()
            .details(&p.details)
            .state(&p.state)
            .timestamps(activity::Timestamps::new().start(p.start))
            .assets(assets)
            .buttons(buttons);

        let ok = match self.client.as_mut() {
            Some(c) => c.set_activity(act).is_ok(),
            None => false,
        };
        if !ok {
            // Discord was closed or restarted: drop the connection, the next join reconnects
            crate::log!(IDENT, "Lost connection to Discord");
            self.client = None;
        }
    }

    pub fn on_join(&mut self, data: &ActivityData) {
        const IDENT: &str = "DiscordRpc::OnJoin";

        let (allow_join, show_account) = {
            let s = app::settings();
            (s.prop.allow_activity_joining, s.prop.show_account_on_rich_presence)
        };

        let Some(u) = universe_details(data.universe_id) else {
            crate::log!(IDENT, "Could not load details of universe {}", data.universe_id);
            return;
        };

        let mut small_image = "roblox".to_string();
        let mut small_text = "Roblox".to_string();
        if show_account {
            if let Some((image, text)) = user_details(data.user_id) {
                if let Some(i) = image {
                    small_image = i;
                }
                small_text = text;
            }
        }

        let state = match data.server_type {
            ServerType::Private => "In a private server".to_string(),
            ServerType::Reserved => "In a reserved server".to_string(),
            ServerType::Public => {
                format!("by {}{}", u.creator, if u.verified { " \u{2611}\u{fe0f}" } else { "" })
            }
        };

        // Discord rejects details shorter than two characters
        let mut details = u.name.clone();
        if details.chars().count() < 2 {
            details.push_str("\u{2800}\u{2800}\u{2800}");
        }

        let p = Presence {
            details,
            state,
            start: data.time_joined.map(|t| t.timestamp()).unwrap_or_else(|| chrono::Utc::now().timestamp()),
            large_image: u.icon.unwrap_or_else(|| "roblox".to_string()),
            large_text: u.name,
            small_image,
            small_text,
            join_url: (allow_join && data.server_type == ServerType::Public).then(|| data.invite_deeplink()),
            game_url: format!("https://www.roblox.com/games/{}", data.place_id),
        };
        self.send(&p);
    }

    pub fn on_leave(&mut self) {
        if let Some(c) = self.client.as_mut() {
            if c.clear_activity().is_err() {
                self.client = None;
            }
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(mut c) = self.client.take() {
            let _ = c.clear_activity();
            let _ = c.close();
        }
    }
}
