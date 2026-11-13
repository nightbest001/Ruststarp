//! Saved Roblox accounts ("Multi Account").
//!
//! Written from scratch for Ruststrap. Cookies are protected with Windows DPAPI (current user), so the
//! file is useless on another PC or another Windows user, and nothing is sent anywhere except to
//! roblox.com. A launch gets a short-lived authentication ticket for the account and starts Roblox
//! through the normal Ruststrap launch path with a `roblox-player:` link.
//!
//! Three ways to add an account: a Roblox sign-in window (`login.rs`), username + password, or a
//! pasted `.ROBLOSECURITY` cookie.

use std::ffi::c_void;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::lang::pick;
use crate::{app, paths};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Account {
    pub user_id: u64,
    pub name: String,
    pub display_name: String,
    pub avatar: String,
    /// DPAPI-protected `.ROBLOSECURITY` value, base64.
    pub cookie: String,
    pub added: i64,
    pub last_used: i64,
    #[serde(skip)]
    pub valid: Option<bool>,
}

static STORE: Mutex<Option<Vec<Account>>> = Mutex::new(None);

fn file() -> PathBuf {
    paths::get().base.join("Accounts.json")
}

fn with<R>(f: impl FnOnce(&mut Vec<Account>) -> R) -> R {
    let mut g = STORE.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        let list: Vec<Account> = fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        *g = Some(list);
    }
    f(g.as_mut().unwrap())
}

fn persist(list: &[Account]) {
    let _ = fs::create_dir_all(&paths::get().base);
    if let Ok(s) = serde_json::to_string_pretty(list) {
        let _ = fs::write(file(), s);
    }
}

// ------------------------------------------------------------------ DPAPI

#[repr(C)]
struct DataBlob {
    cb_data: u32,
    pb_data: *mut u8,
}

#[link(name = "crypt32")]
extern "system" {
    fn CryptProtectData(
        data_in: *const DataBlob,
        description: *const u16,
        entropy: *const DataBlob,
        reserved: *mut c_void,
        prompt: *mut c_void,
        flags: u32,
        data_out: *mut DataBlob,
    ) -> i32;
    fn CryptUnprotectData(
        data_in: *const DataBlob,
        description: *mut *mut u16,
        entropy: *const DataBlob,
        reserved: *mut c_void,
        prompt: *mut c_void,
        flags: u32,
        data_out: *mut DataBlob,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(mem: *mut c_void) -> *mut c_void;
}

/// Extra secret mixed into the encryption, so another program's DPAPI blobs cannot be swapped in.
const ENTROPY: &[u8] = b"Ruststrap.Accounts.v1";

pub(crate) fn protect(plain: &str) -> Result<String, String> {
    let input = DataBlob { cb_data: plain.len() as u32, pb_data: plain.as_ptr() as *mut u8 };
    let entropy = DataBlob { cb_data: ENTROPY.len() as u32, pb_data: ENTROPY.as_ptr() as *mut u8 };
    let mut out = DataBlob { cb_data: 0, pb_data: std::ptr::null_mut() };
    // SAFETY: both input blobs point at memory that outlives the call; the output buffer is copied and
    // then released with LocalFree, as the API requires.
    unsafe {
        let ok = CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut out);
        if ok == 0 || out.pb_data.is_null() {
            return Err(pick("Windows could not encrypt the cookie.", "Windows gagal mengenkripsi cookie."));
        }
        let bytes = std::slice::from_raw_parts(out.pb_data, out.cb_data as usize).to_vec();
        LocalFree(out.pb_data as *mut c_void);
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    }
}

pub(crate) fn unprotect(b64: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|e| e.to_string())?;
    let input = DataBlob { cb_data: bytes.len() as u32, pb_data: bytes.as_ptr() as *mut u8 };
    let entropy = DataBlob { cb_data: ENTROPY.len() as u32, pb_data: ENTROPY.as_ptr() as *mut u8 };
    let mut out = DataBlob { cb_data: 0, pb_data: std::ptr::null_mut() };
    // SAFETY: see `protect`.
    unsafe {
        let ok = CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut out);
        if ok == 0 || out.pb_data.is_null() {
            return Err(pick(
                "This cookie was saved by another Windows user and cannot be read.",
                "Cookie ini disimpan oleh user Windows lain dan tidak bisa dibaca.",
            ));
        }
        let bytes = std::slice::from_raw_parts(out.pb_data, out.cb_data as usize).to_vec();
        LocalFree(out.pb_data as *mut c_void);
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
}

// ------------------------------------------------------------------ roblox.com

/// Accepts the bare value, `.ROBLOSECURITY=value` or a whole `Cookie:` line.
pub fn clean_cookie(raw: &str) -> String {
    let mut s = raw.trim().trim_matches('"').trim();
    if let Some(i) = s.to_lowercase().find(".roblosecurity=") {
        s = &s[i + ".roblosecurity=".len()..];
    }
    s.split(';').next().unwrap_or(s).trim().trim_matches('"').to_string()
}

fn cookie_header(cookie: &str) -> String {
    format!(".ROBLOSECURITY={cookie}")
}

fn net(e: reqwest::Error) -> String {
    pick(&format!("Could not reach Roblox: {e}"), &format!("Tidak bisa terhubung ke Roblox: {e}"))
}

/// (user id, name, display name) of the account the cookie belongs to.
fn whoami(cookie: &str) -> Result<(u64, String, String), String> {
    if cookie.is_empty() || cookie.contains(char::is_whitespace) {
        return Err(pick("That does not look like a Roblox cookie.", "Itu sepertinya bukan cookie Roblox."));
    }
    let r = app::http()
        .get("https://users.roblox.com/v1/users/authenticated")
        .header("Cookie", cookie_header(cookie))
        .send()
        .map_err(net)?;
    let status = r.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(pick(
            "Roblox did not accept this cookie. It may be expired or from a signed-out session.",
            "Roblox tidak menerima cookie ini. Mungkin sudah kedaluwarsa atau sesinya sudah logout.",
        ));
    }
    if !status.is_success() {
        return Err(format!("Roblox answered HTTP {status}"));
    }
    let v: Value = r.json().map_err(net)?;
    let id = v.get("id").and_then(Value::as_u64).ok_or_else(|| "unexpected answer from Roblox".to_string())?;
    let name = v.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
    let display = v.get("displayName").and_then(Value::as_str).unwrap_or(&name).to_string();
    Ok((id, name, display))
}

fn avatar_url(user_id: u64) -> String {
    let url = format!("https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds={user_id}&size=150x150&format=Png&isCircular=false");
    app::http()
        .get(url)
        .send()
        .ok()
        .and_then(|r| r.json::<Value>().ok())
        .and_then(|v| v.pointer("/data/0/imageUrl").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

fn auth_ticket(cookie: &str) -> Result<String, String> {
    let url = "https://auth.roblox.com/v1/authentication-ticket";
    let mut csrf = String::new();
    for _ in 0..3 {
        let mut rq = app::http()
            .post(url)
            .header("Cookie", cookie_header(cookie))
            .header("Referer", "https://www.roblox.com/")
            .header("Origin", "https://www.roblox.com")
            .header("User-Agent", "Roblox/WinInet")
            .header("Content-Type", "application/json")
            .body("{}");
        if !csrf.is_empty() {
            rq = rq.header("X-CSRF-TOKEN", csrf.clone());
        }
        let r = rq.send().map_err(net)?;
        if let Some(t) = r.headers().get("rbx-authentication-ticket").and_then(|v| v.to_str().ok()) {
            return Ok(t.to_string());
        }
        if r.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(pick(
                "This account is signed out. Remove it and add it again.",
                "Akun ini sudah logout. Hapus lalu tambahkan lagi.",
            ));
        }
        match r.headers().get("x-csrf-token").and_then(|v| v.to_str().ok()) {
            Some(t) => csrf = t.to_string(),
            None => return Err(format!("Roblox answered HTTP {}", r.status())),
        }
    }
    Err(pick("Could not get a sign-in ticket from Roblox.", "Gagal mengambil tiket login dari Roblox."))
}

fn pct(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

/// Cheap unique-ish number without pulling in a random crate.
pub(crate) fn rand_u64(salt: u64) -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(salt);
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    h.finish()
}

fn guid() -> String {
    let (a, b) = (rand_u64(1), rand_u64(2));
    format!(
        "{:08x}-{:04x}-4{:03x}-a{:03x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        (a & 0xfff) as u16,
        (b & 0xfff) as u16,
        (b >> 16) & 0xffff_ffff_ffff
    )
}

fn launch_uri(ticket: &str, place: u64, job: &str) -> String {
    let tracker = 100_000_000_000u64 + rand_u64(3) % 900_000_000_000u64;
    let now = chrono::Utc::now().timestamp_millis();
    let base = "https://assets.roblox.com/game/placelauncher.ashx";
    let join = guid();
    let target = if job.is_empty() {
        format!("{base}?request=RequestGame&browserTrackerId={tracker}&placeId={place}&isPlayTogetherGame=false&joinAttemptId={join}&joinAttemptOrigin=PlayButton")
    } else {
        format!("{base}?request=RequestGameJob&browserTrackerId={tracker}&placeId={place}&gameId={job}&isPlayTogetherGame=false&joinAttemptId={join}&joinAttemptOrigin=PlayButton")
    };
    format!(
        "roblox-player:1+launchmode:play+gameinfo:{ticket}+launchtime:{now}+placelauncherurl:{}+browsertrackerid:{tracker}+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp",
        pct(&target)
    )
}

/// Opens the Roblox app (home screen) as the signed-in account, with no game.
fn launch_app_uri(ticket: &str) -> String {
    let tracker = 100_000_000_000u64 + rand_u64(3) % 900_000_000_000u64;
    let now = chrono::Utc::now().timestamp_millis();
    format!(
        "roblox-player:1+launchmode:app+gameinfo:{ticket}+launchtime:{now}+browsertrackerid:{tracker}+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp"
    )
}

// ------------------------------------------------------------------ public API

fn to_json(a: &Account) -> Value {
    json!({
        "user_id": a.user_id,
        "username": a.name,
        "display_name": a.display_name,
        "avatar": a.avatar,
        "added": a.added,
        "last_used": a.last_used,
        "valid": a.valid,
    })
}

pub fn list_json() -> Value {
    with(|l| Value::Array(l.iter().map(to_json).collect()))
}

/// Checks the cookie with Roblox and saves (or updates) the account. Way 3: a pasted cookie.
pub fn add_cookie(raw: &str) -> Result<Value, String> {
    let cookie = clean_cookie(raw);
    let (id, name, display) = whoami(&cookie)?;
    let avatar = avatar_url(id);
    let enc = protect(&cookie)?;
    let now = chrono::Utc::now().timestamp();
    with(|l| {
        let acc = match l.iter_mut().find(|a| a.user_id == id) {
            Some(a) => {
                a.name = name;
                a.display_name = display;
                if !avatar.is_empty() {
                    a.avatar = avatar;
                }
                a.cookie = enc;
                a.valid = Some(true);
                a.clone()
            }
            None => {
                let a = Account { user_id: id, name, display_name: display, avatar, cookie: enc, added: now, last_used: 0, valid: Some(true) };
                l.push(a.clone());
                a
            }
        };
        persist(l);
        Ok(to_json(&acc))
    })
}

/// Way 2: username and password. Roblox may demand a captcha or a code; that cannot be done here, so the
/// message points to the sign-in window.
pub fn add_password(username: &str, password: &str) -> Result<Value, String> {
    let username = username.trim();
    if username.is_empty() || password.is_empty() {
        return Err(pick("Fill in every field first.", "Isi semua kolom dulu."));
    }
    let body = json!({"ctype": "Username", "cvalue": username, "password": password}).to_string();
    let mut csrf = String::new();
    for _ in 0..3 {
        let mut rq = app::http()
            .post("https://auth.roblox.com/v2/login")
            .header("Content-Type", "application/json")
            .header("Referer", "https://www.roblox.com/")
            .header("Origin", "https://www.roblox.com")
            .body(body.clone());
        if !csrf.is_empty() {
            rq = rq.header("X-CSRF-TOKEN", csrf.clone());
        }
        let r = rq.send().map_err(net)?;
        let status = r.status();
        let token = r.headers().get("x-csrf-token").and_then(|v| v.to_str().ok()).map(str::to_string);
        let challenge = r.headers().contains_key("rblx-challenge-id");
        let cookie = r
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(|s| s.strip_prefix(".ROBLOSECURITY="))
            .map(|s| s.split(';').next().unwrap_or("").to_string());

        if status.is_success() {
            return match cookie {
                Some(c) if !c.is_empty() => add_cookie(&c),
                _ => Err(pick(
                    "Roblox did not give a session. Use Sign in with Roblox instead.",
                    "Roblox tidak memberi sesi. Pakai Masuk lewat Roblox saja.",
                )),
            };
        }
        if status == reqwest::StatusCode::FORBIDDEN && csrf.is_empty() {
            if let Some(t) = token {
                csrf = t; // first answer only hands out the CSRF token: send the same request again
                continue;
            }
        }
        if challenge {
            return Err(pick(
                "Roblox asks for a captcha or a code. Use Sign in with Roblox instead.",
                "Roblox meminta captcha atau kode. Pakai Masuk lewat Roblox saja.",
            ));
        }
        let v: Value = r.json().unwrap_or(Value::Null);
        let msg = v.pointer("/errors/0/message").and_then(Value::as_str).unwrap_or_default().to_string();
        return Err(if msg.is_empty() { format!("Roblox answered HTTP {status}") } else { msg });
    }
    Err(pick("Roblox did not accept the request.", "Roblox tidak menerima permintaan."))
}

/// Way 1: opens a Roblox sign-in window in its own process and waits for the cookie it brings back.
pub fn add_window() -> Result<Value, String> {
    let out = paths::get().base.join(format!("login-{:x}.tmp", rand_u64(7)));
    let _ = fs::create_dir_all(&paths::get().base);
    Command::new(paths::process()).arg("-accountlogin").arg(&out).status().map_err(|e| e.to_string())?;
    // the window process wrote the cookie already protected with DPAPI
    let enc = fs::read_to_string(&out).unwrap_or_default();
    let _ = fs::remove_file(&out);
    if enc.trim().is_empty() {
        return Err(pick(
            "The sign-in window was closed before you signed in.",
            "Jendela masuk ditutup sebelum kamu selesai masuk.",
        ));
    }
    let cookie = unprotect(&enc)?;
    add_cookie(&cookie)
}

pub fn remove(user_id: u64) {
    with(|l| {
        l.retain(|a| a.user_id != user_id);
        persist(l);
    });
}

/// Is the saved session still good? `Ok(false)` means Roblox says it is signed out; `Err` means we could not ask.
pub fn check(user_id: u64) -> Result<bool, String> {
    let enc = with(|l| l.iter().find(|a| a.user_id == user_id).map(|a| a.cookie.clone()))
        .ok_or_else(|| pick("That account is gone.", "Akun itu sudah tidak ada."))?;
    let cookie = unprotect(&enc)?;
    let resp = app::http()
        .get("https://users.roblox.com/v1/users/authenticated")
        .header("Cookie", cookie_header(&cookie))
        .send()
        .map_err(net)?;
    let ok = match resp.status().as_u16() {
        200 => true,
        401 | 403 => false,
        s => return Err(pick(&format!("Roblox answered with status {s}."), &format!("Roblox menjawab dengan status {s}."))),
    };
    with(|l| {
        if let Some(a) = l.iter_mut().find(|a| a.user_id == user_id) {
            a.valid = Some(ok);
        }
    });
    Ok(ok)
}

/// Digits of a place id, or the id inside a roblox.com/games/<id>/ link.
pub fn parse_place(input: &str) -> Option<u64> {
    let s = input.trim();
    if let Ok(n) = s.parse::<u64>() {
        return (n > 0).then_some(n);
    }
    let i = s.find("/games/")? + "/games/".len();
    let digits: String = s[i..].chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|n| *n > 0)
}

/// Starts Roblox as `user_id`. An empty `place` opens the Roblox app without a game.
/// The window and updates come from the normal launch path.
pub fn launch(user_id: u64, place: &str, job: &str) -> Result<(), String> {
    let place_id = if place.trim().is_empty() {
        None
    } else {
        Some(parse_place(place).ok_or_else(|| {
            pick("That Place ID or game link looks wrong.", "Place ID atau link game sepertinya salah.")
        })?)
    };
    let job = job.trim();
    if !job.is_empty() && !job.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(pick("The server ID looks wrong.", "Server ID sepertinya salah."));
    }
    let enc = with(|l| l.iter().find(|a| a.user_id == user_id).map(|a| a.cookie.clone()))
        .ok_or_else(|| pick("That account is gone.", "Akun itu sudah tidak ada."))?;
    let cookie = unprotect(&enc)?;
    let ticket = auth_ticket(&cookie)?;
    let uri = match place_id {
        Some(id) => launch_uri(&ticket, id, job),
        None => launch_app_uri(&ticket),
    };
    Command::new(paths::process()).arg(uri).spawn().map_err(|e| e.to_string())?;
    with(|l| {
        if let Some(a) = l.iter_mut().find(|a| a.user_id == user_id) {
            a.last_used = chrono::Utc::now().timestamp();
        }
        persist(l);
    });
    // remember the game for next time
    {
        let mut s = app::settings();
        s.prop.account_place = place.trim().to_string();
        s.prop.account_job = job.to_string();
        s.save();
    }
    Ok(())
}
