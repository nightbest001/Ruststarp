//! Self-update from GitHub Releases.
//!
//! The window asks `check()` when it opens. If a newer release exists, the page shows a banner; the button
//! calls `start()`, which downloads the release's `ruststrap.exe` next to the installed copy, checks it, and
//! swaps it in. Windows lets a running exe be renamed but not overwritten, so the old file is renamed to
//! `Ruststrap.exe.old` and deleted on the next start (`cleanup_leftovers`). No batch script, no browser.
//!
//! Release rule: tag `vX.Y.Z` and an asset named `ruststrap.exe` (the GitHub Actions workflow does both).

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::app::{self, PROJECT_NAME, PROJECT_REPOSITORY, VERSION};
use crate::lang::pick;
use crate::paths;

const IDENT: &str = "Updater";
const ASSET_NAME: &str = "ruststrap.exe";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    /// `sha256:<hex>`; GitHub adds it to release assets.
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Idle,
    Downloading,
    Installing,
    Done,
    Failed,
}

pub fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Idle => "idle",
        Phase::Downloading => "downloading",
        Phase::Installing => "installing",
        Phase::Done => "done",
        Phase::Failed => "failed",
    }
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
}

struct Inner {
    /// What `check()` found last; `start()` installs exactly this (the page never passes a URL).
    latest: Option<UpdateInfo>,
    progress: Progress,
}

static STATE: Mutex<Inner> =
    Mutex::new(Inner { latest: None, progress: Progress { phase: Phase::Idle, done: 0, total: 0, error: None } });

fn with<R>(f: impl FnOnce(&mut Inner) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}

pub fn progress() -> Progress {
    with(|s| s.progress.clone())
}

// ------------------------------------------------------------------ version compare

fn parse_version(s: &str) -> Vec<u64> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core: String = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    core.split('.').filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(0)).collect()
}

pub fn is_newer(remote: &str, local: &str) -> bool {
    let (mut a, mut b) = (parse_version(remote), parse_version(local));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

// ------------------------------------------------------------------ check

/// `Ok(None)`: already the newest (or no release published yet).
pub fn check() -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{PROJECT_REPOSITORY}/releases/latest");
    crate::log!(IDENT, "Checking {url}");
    let resp = app::http()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(15))
        .send()
        .map_err(|e| pick(&format!("Could not reach GitHub: {e}"), &format!("Gagal nyambung ke GitHub: {e}")))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        with(|s| s.latest = None);
        return Ok(None);
    }
    if !status.is_success() {
        return Err(pick(
            &format!("GitHub answered with HTTP {}", status.as_u16()),
            &format!("GitHub membalas dengan HTTP {}", status.as_u16()),
        ));
    }
    let rel: Release = resp.json().map_err(|e| pick(&format!("Unreadable answer from GitHub: {e}"), &format!("Balasan GitHub nggak bisa dibaca: {e}")))?;
    let remote = rel.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
    crate::log!(IDENT, "Latest release {remote}, running {VERSION}");
    if !is_newer(&remote, VERSION) {
        with(|s| s.latest = None);
        return Ok(None);
    }

    let asset = rel
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(ASSET_NAME))
        .or_else(|| rel.assets.iter().find(|a| a.name.to_lowercase().ends_with(".exe")))
        .ok_or_else(|| {
            pick(&format!("Release {remote} has no {ASSET_NAME} file"), &format!("Rilis {remote} nggak punya file {ASSET_NAME}"))
        })?;
    let prefix = format!("https://github.com/{PROJECT_REPOSITORY}/releases/download/").to_lowercase();
    if !asset.browser_download_url.to_lowercase().starts_with(&prefix) {
        return Err(pick(
            "The release points to an unexpected download address",
            "Rilis ini nunjuk ke alamat unduhan yang nggak dikenal",
        ));
    }
    let sha256 = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).map(|d| d.trim().to_lowercase());
    let info = UpdateInfo { version: remote, url: asset.browser_download_url.clone(), size: asset.size, sha256 };
    with(|s| s.latest = Some(info.clone()));
    Ok(Some(info))
}

// ------------------------------------------------------------------ download + swap

/// Starts the update on a background thread. `on_done(true)` runs once the new exe is in place.
pub fn start(on_done: impl FnOnce(bool) + Send + 'static) -> Result<(), String> {
    let info = with(|s| -> Result<UpdateInfo, String> {
        if matches!(s.progress.phase, Phase::Downloading | Phase::Installing) {
            return Err(pick("An update is already running", "Update lagi jalan"));
        }
        let info = s.latest.clone().ok_or_else(|| pick("No update is waiting", "Nggak ada update yang nunggu"))?;
        s.progress = Progress { phase: Phase::Downloading, done: 0, total: info.size, error: None };
        Ok(info)
    })?;
    std::thread::spawn(move || {
        let ok = match run(&info) {
            Ok(()) => {
                with(|s| s.progress.phase = Phase::Done);
                true
            }
            Err(e) => {
                crate::log!(IDENT, "Update failed: {e}");
                with(|s| {
                    s.progress.phase = Phase::Failed;
                    s.progress.error = Some(e);
                });
                false
            }
        };
        on_done(ok);
    });
    Ok(())
}

fn run(info: &UpdateInfo) -> Result<(), String> {
    let exe = paths::get().application.clone();
    if !exe.is_file() || !paths::running_installed_copy() {
        return Err(pick(
            &format!("Open the installed copy of {PROJECT_NAME} to update it"),
            &format!("Buka dulu {PROJECT_NAME} yang sudah terpasang buat update"),
        ));
    }
    let new = exe.with_extension("exe.new");
    let old = exe.with_extension("exe.old");
    let result = download(info, &new).and_then(|_| {
        with(|s| s.progress.phase = Phase::Installing);
        swap(&exe, &new, &old)
    });
    if result.is_err() {
        let _ = fs::remove_file(&new);
    }
    result
}

fn download(info: &UpdateInfo, dest: &Path) -> Result<(), String> {
    crate::log!(IDENT, "Downloading {} to {}", info.url, dest.display());
    let mut resp = app::http()
        .get(&info.url)
        .timeout(Duration::from_secs(900))
        .send()
        .map_err(|e| pick(&format!("Download failed: {e}"), &format!("Unduhan gagal: {e}")))?;
    if !resp.status().is_success() {
        return Err(pick(
            &format!("Download failed: HTTP {}", resp.status().as_u16()),
            &format!("Unduhan gagal: HTTP {}", resp.status().as_u16()),
        ));
    }
    let total = resp.content_length().unwrap_or(info.size);
    with(|s| s.progress.total = total);

    let mut file = File::create(dest).map_err(|e| {
        pick(&format!("Cannot write {}: {e}", dest.display()), &format!("Gagal nulis {}: {e}", dest.display()))
    })?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done: u64 = 0;
    loop {
        let n = resp.read(&mut buf).map_err(|e| pick(&format!("Download interrupted: {e}"), &format!("Unduhan terputus: {e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| {
            pick(&format!("Cannot write {}: {e}", dest.display()), &format!("Gagal nulis {}: {e}", dest.display()))
        })?;
        done += n as u64;
        with(|s| s.progress.done = done);
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    if info.size > 0 && done != info.size {
        return Err(pick(
            &format!("Downloaded {done} bytes but the release lists {}", info.size),
            &format!("Ukuran unduhan {done} byte, beda dari yang tercatat di rilis ({})", info.size),
        ));
    }
    if done < 100_000 {
        return Err(pick(
            "The downloaded file is too small to be Ruststrap",
            "File yang diunduh terlalu kecil buat jadi Ruststrap",
        ));
    }
    let mut head = [0u8; 2];
    File::open(dest).and_then(|mut f| f.read_exact(&mut head)).map_err(|e| e.to_string())?;
    if &head != b"MZ" {
        return Err(pick("The downloaded file is not a Windows program", "File yang diunduh bukan program Windows"));
    }
    if let Some(want) = &info.sha256 {
        let got = format!("{:x}", hasher.finalize());
        if !got.eq_ignore_ascii_case(want) {
            return Err(pick(
                "The downloaded file does not match the checksum of the release",
                "File yang diunduh nggak cocok sama checksum rilisnya",
            ));
        }
    }
    Ok(())
}

/// A running exe can be renamed but not overwritten: move it aside, then put the new one in its place.
fn swap(exe: &Path, new: &Path, old: &Path) -> Result<(), String> {
    let _ = fs::remove_file(old);
    fs::rename(exe, old).map_err(|e| {
        pick(
            &format!("Cannot move the current {PROJECT_NAME}.exe aside: {e}"),
            &format!("Gagal menyingkirkan {PROJECT_NAME}.exe yang lama: {e}"),
        )
    })?;
    if let Err(e) = fs::rename(new, exe) {
        let _ = fs::rename(old, exe);
        return Err(pick(
            &format!("Cannot put the new version in place: {e}"),
            &format!("Gagal memasang versi baru: {e}"),
        ));
    }
    crate::log!(IDENT, "Swapped in the new executable");
    Ok(())
}

/// Delete the leftovers of an earlier update. Call at start-up of the installed copy.
pub fn cleanup_leftovers() {
    if !paths::running_installed_copy() {
        return;
    }
    let exe = &paths::get().application;
    for ext in ["exe.old", "exe.new"] {
        let f = exe.with_extension(ext);
        if f.exists() {
            let _ = fs::remove_file(&f);
        }
    }
}

/// `ruststrap update`: check, download and swap from a terminal.
pub fn cli() -> i32 {
    match check() {
        Err(e) => {
            eprintln!("{e}");
            1
        }
        Ok(None) => {
            println!(
                "{}",
                pick(
                    &format!("{PROJECT_NAME} {VERSION} is the latest version."),
                    &format!("{PROJECT_NAME} {VERSION} sudah versi terbaru.")
                )
            );
            0
        }
        Ok(Some(info)) => {
            println!(
                "{}",
                pick(
                    &format!("New version {} found (you have {VERSION}). Downloading...", info.version),
                    &format!("Ada versi baru {} (kamu pakai {VERSION}). Lagi mengunduh...", info.version)
                )
            );
            with(|s| s.progress = Progress { phase: Phase::Downloading, done: 0, total: info.size, error: None });
            match run(&info) {
                Ok(()) => {
                    println!("{}", pick(&format!("Updated to {}.", info.version), &format!("Berhasil update ke {}.", info.version)));
                    0
                }
                Err(e) => {
                    eprintln!("{}", pick(&format!("Update failed: {e}"), &format!("Update gagal: {e}")));
                    1
                }
            }
        }
    }
}
