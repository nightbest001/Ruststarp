//! Port of `RobloxInterfaces/Deployment.cs`.

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::app::http;
use crate::models::ClientVersion;

pub const DEFAULT_CHANNEL: &str = "production";
const VERSION_STUDIO_HASH: &str = "version-012732894899482c";

/// (url, delay in seconds before trying). The primary CDN goes first, the others are fallbacks.
const BASE_URLS: &[(&str, u64)] = &[
    ("https://setup.rbxcdn.com", 0),
    ("https://setup-aws.rbxcdn.com", 2),
    ("https://setup-ak.rbxcdn.com", 2),
    ("https://roblox-setup.cachefly.net", 2),
    ("https://s3.amazonaws.com/setup.roblox.com", 4),
];

#[derive(Debug)]
pub enum DeployError {
    /// The channel does not exist or is not accessible (401/403/404).
    InvalidChannel(u16),
    Other(String),
}

impl std::fmt::Display for DeployError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeployError::InvalidChannel(c) => write!(f, "invalid channel (HTTP {c})"),
            DeployError::Other(s) => write!(f, "{s}"),
        }
    }
}

pub struct Deployment {
    pub channel: String,
    pub binary_type: String,
    pub base_url: String,
    cache: HashMap<String, ClientVersion>,
}

fn test_connection(url: &str) -> Result<(), String> {
    crate::log!(&format!("Deployment::TestConnection<{url}>"), "Connecting...");
    let resp = http().get(format!("{url}/versionStudio")).send().map_err(|e| e.to_string())?;
    let resp = resp.error_for_status().map_err(|e| e.to_string())?;
    let body = resp.text().map_err(|e| e.to_string())?;
    if body.trim() != VERSION_STUDIO_HASH {
        return Err(format!("versionStudio response does not match (expected \"{VERSION_STUDIO_HASH}\", got \"{body}\")"));
    }
    Ok(())
}

impl Deployment {
    pub fn new() -> Self {
        Self {
            channel: DEFAULT_CHANNEL.to_string(),
            binary_type: "WindowsPlayer".to_string(),
            base_url: String::new(),
            cache: HashMap::new(),
        }
    }

    pub fn is_default_channel(&self) -> bool {
        self.channel.eq_ignore_ascii_case(DEFAULT_CHANNEL)
    }

    /// Races every mirror, keeps the first one that answers correctly.
    pub fn init_connectivity(&mut self) -> Result<(), String> {
        const IDENT: &str = "Deployment::InitializeConnectivity";
        crate::log!(IDENT, "Testing connectivity...");
        let (tx, rx) = mpsc::channel::<Result<String, String>>();
        for (url, delay) in BASE_URLS {
            let tx = tx.clone();
            let url = url.to_string();
            let delay = *delay;
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(delay));
                let r = test_connection(&url).map(|_| url.clone());
                if let Err(e) = &r {
                    crate::log!(&format!("Deployment::TestConnection<{url}>"), "{e}");
                }
                let _ = tx.send(r);
            });
        }
        drop(tx);

        let mut first_err: Option<String> = None;
        for r in rx {
            match r {
                Ok(url) => {
                    crate::log!(IDENT, "Got {url} as the optimal base URL");
                    self.base_url = url;
                    return Ok(());
                }
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }
        Err(first_err.unwrap_or_else(|| "All connection attempts timed out.".to_string()))
    }

    pub fn get_location(&self, resource: &str) -> String {
        format!("{}/channel/common{}", self.base_url, resource)
    }

    fn fetch_info(&self, host: &str, path: &str) -> Result<ClientVersion, DeployError> {
        let resp = http().get(format!("{host}{path}")).send().map_err(|e| DeployError::Other(e.to_string()))?;
        let code = resp.status().as_u16();
        if !self.is_default_channel() && matches!(code, 401 | 403 | 404) {
            return Err(DeployError::InvalidChannel(code));
        }
        let resp = resp.error_for_status().map_err(|e| DeployError::Other(e.to_string()))?;
        resp.json::<ClientVersion>().map_err(|e| DeployError::Other(e.to_string()))
    }

    pub fn get_info(&mut self, channel: Option<&str>) -> Result<ClientVersion, DeployError> {
        const IDENT: &str = "Deployment::GetInfo";
        let channel = channel.map(str::to_string).unwrap_or_else(|| self.channel.clone());
        let is_default = channel.eq_ignore_ascii_case(DEFAULT_CHANNEL);
        crate::log!(IDENT, "Getting deploy info for channel {channel}");

        let key = format!("{}-{}", channel, self.binary_type);
        if let Some(v) = self.cache.get(&key) {
            crate::log!(IDENT, "Deploy information is cached");
            return Ok(v.clone());
        }

        let mut path = format!("/v2/client-version/{}", self.binary_type);
        if !is_default {
            path = format!("/v2/client-version/{}/channel/{}", self.binary_type, channel);
        }

        let info = match self.fetch_info("https://clientsettingscdn.roblox.com", &path) {
            Ok(v) => v,
            Err(DeployError::InvalidChannel(c)) => return Err(DeployError::InvalidChannel(c)),
            Err(e) => {
                crate::log!(IDENT, "Failed to contact clientsettingscdn! Falling back to clientsettings...");
                crate::logger::write_exception(IDENT, &e);
                self.fetch_info("https://clientsettings.roblox.com", &path)?
            }
        };
        self.cache.insert(key, info.clone());
        Ok(info)
    }
}
