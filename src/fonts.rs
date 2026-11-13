//! Custom font mod (the same idea as Bloxstrap's "Use custom font") with a backup of Roblox's own
//! font families, so "Remove applied font" always brings the default look back.
//!
//! How it works: Roblox lists its fonts in `content\fonts\families\*.json`. Each face points at a
//! font file through `assetId`. We copy the chosen font to `Modifications\content\fonts\CustomFont.ttf`
//! and write a copy of every family JSON whose faces all point at that file. The normal mod
//! pipeline then copies those into the Roblox folder. Before anything is overwritten the original
//! JSON files are copied to `<base>\FontBackup\<version>\`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{app, paths};

const FONT_ASSET: &str = "rbxasset://fonts/CustomFont.ttf";
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
struct Meta {
    /// Family JSON files we generated into the mods folder.
    generated: Vec<String>,
    font_file: String,
    font_family: String,
}

fn mods_root() -> PathBuf {
    paths::get().modifications.join("content").join("fonts")
}
pub fn mod_font_path() -> PathBuf {
    mods_root().join("CustomFont.ttf")
}
fn mod_families_dir() -> PathBuf {
    mods_root().join("families")
}
fn backup_root() -> PathBuf {
    paths::get().base.join("FontBackup")
}
fn backup_dir(guid: &str) -> PathBuf {
    backup_root().join(guid)
}
fn meta_path() -> PathBuf {
    backup_root().join("font.json")
}

fn load_meta() -> Meta {
    fs::read_to_string(meta_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
fn save_meta(m: &Meta) {
    let _ = fs::create_dir_all(backup_root());
    let _ = fs::write(meta_path(), serde_json::to_string_pretty(m).unwrap_or_default());
}

fn norm(s: &str) -> String {
    s.replace('/', "\\").to_lowercase()
}
fn family_rel(name: &str) -> String {
    format!("content\\fonts\\families\\{name}")
}
fn modified_set(studio: bool) -> HashSet<String> {
    app::dist_state(studio).prop.mod_manifest.iter().map(|s| norm(s)).collect()
}
fn json_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.to_lowercase().ends_with(".json"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

// ------------------------------------------------------------------ font file check

fn be16(b: &[u8], o: usize) -> Option<usize> {
    Some(u16::from_be_bytes(b.get(o..o + 2)?.try_into().ok()?) as usize)
}
fn be32(b: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_be_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize)
}

/// Checks that `bytes` is a TrueType / OpenType font and returns its family name.
pub fn inspect(bytes: &[u8]) -> Result<String, ()> {
    let tag = bytes.get(0..4).ok_or(())?;
    if !(tag == &[0u8, 1, 0, 0][..] || tag == &b"true"[..] || tag == &b"OTTO"[..]) {
        return Err(());
    }
    let num = be16(bytes, 4).ok_or(())?;
    if num == 0 || num > 256 {
        return Err(());
    }
    let (mut name_tbl, mut has_cmap, mut has_head, mut has_outline) = (None, false, false, false);
    for i in 0..num {
        let o = 12 + 16 * i;
        let t = bytes.get(o..o + 4).ok_or(())?;
        let off = be32(bytes, o + 8).ok_or(())?;
        let len = be32(bytes, o + 12).ok_or(())?;
        if off.checked_add(len).map_or(true, |end| end > bytes.len()) {
            return Err(());
        }
        match t {
            b"name" => name_tbl = Some(off),
            b"cmap" => has_cmap = true,
            b"head" => has_head = true,
            b"glyf" | b"CFF " | b"CFF2" => has_outline = true,
            _ => {}
        }
    }
    if !(has_cmap && has_head && has_outline) {
        return Err(());
    }
    Ok(name_tbl.and_then(|o| family_name(bytes, o)).unwrap_or_default())
}

fn family_name(b: &[u8], tbl: usize) -> Option<String> {
    let count = be16(b, tbl + 2)?;
    let str_off = tbl + be16(b, tbl + 4)?;
    let mut best: Option<(u8, String)> = None;
    for i in 0..count.min(512) {
        let r = tbl + 6 + 12 * i;
        let platform = be16(b, r)?;
        let name_id = be16(b, r + 6)?;
        let len = be16(b, r + 8)?;
        let off = be16(b, r + 10)?;
        if name_id != 1 && name_id != 16 {
            continue;
        }
        let raw = b.get(str_off + off..str_off + off + len)?;
        let text = match platform {
            3 | 0 => {
                let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&units)
            }
            _ => String::from_utf8_lossy(raw).to_string(),
        };
        if text.trim().is_empty() {
            continue;
        }
        // typographic family (16) beats plain family (1); Windows names beat Mac ones
        let score = (if name_id == 16 { 2 } else { 0 }) + (if platform == 3 { 1 } else { 0 });
        if best.as_ref().map_or(true, |(s, _)| score > *s) {
            best = Some((score, text));
        }
    }
    best.map(|(_, t)| t)
}

// ------------------------------------------------------------------ backup

/// Copies the untouched family JSONs of one Roblox version to `FontBackup\<guid>`.
/// Does nothing when that backup already exists, so the first (pristine) copy is the one kept.
fn backup_originals(version_dir: &Path, guid: &str, modified: &HashSet<String>) {
    const IDENT: &str = "Fonts::Backup";
    if guid.is_empty() {
        return;
    }
    let dst = backup_dir(guid);
    if !json_names(&dst).is_empty() {
        return;
    }
    let src = version_dir.join("content").join("fonts").join("families");
    let mut n = 0;
    for name in json_names(&src) {
        if modified.contains(&norm(&family_rel(&name))) {
            continue; // already replaced by us, not an original
        }
        if fs::create_dir_all(&dst).is_ok() && fs::copy(src.join(&name), dst.join(&name)).is_ok() {
            n += 1;
        }
    }
    if n > 0 {
        crate::log!(IDENT, "Backed up {n} original font family file(s) for {guid}");
    }
}

/// Drop backups of Roblox versions that are no longer installed.
fn prune_backups() {
    let keep: HashSet<String> = [false, true].iter().map(|s| app::dist_state(*s).prop.version_guid.clone()).collect();
    if let Ok(rd) = fs::read_dir(backup_root()) {
        for e in rd.flatten() {
            if e.path().is_dir() && !keep.contains(&e.file_name().to_string_lossy().to_string()) {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }
}

// ------------------------------------------------------------------ used by the bootstrapper

fn remove_generated(meta: &mut Meta) {
    let dir = mod_families_dir();
    for n in meta.generated.drain(..) {
        let _ = fs::remove_file(dir.join(n));
    }
    let _ = fs::remove_dir(&dir); // only succeeds when empty
}

/// Runs right before the mods are copied into the Roblox folder.
pub fn prepare_mods(version_dir: &Path, guid: &str, studio: bool) {
    const IDENT: &str = "Fonts::Prepare";
    let mut meta = load_meta();

    if !mod_font_path().is_file() {
        if !meta.generated.is_empty() {
            crate::log!(IDENT, "Custom font removed, deleting the generated family files");
            remove_generated(&mut meta);
            save_meta(&meta);
        }
        return;
    }

    let modified = modified_set(studio);
    backup_originals(version_dir, guid, &modified);
    prune_backups();

    let fam_dir = mod_families_dir();
    if fs::create_dir_all(&fam_dir).is_err() {
        return;
    }
    let backup = backup_dir(guid);
    let live = version_dir.join("content").join("fonts").join("families");

    let mut names = json_names(&backup);
    for n in json_names(&live) {
        if !names.contains(&n) {
            names.push(n);
        }
    }

    let mut generated = Vec::new();
    for name in names {
        let out = fam_dir.join(&name);
        // a family file the user put there themselves is left alone
        if out.exists() && !meta.generated.contains(&name) {
            continue;
        }
        let source = if backup.join(&name).is_file() {
            backup.join(&name)
        } else if !modified.contains(&norm(&family_rel(&name))) && live.join(&name).is_file() {
            live.join(&name)
        } else {
            continue;
        };
        let Ok(text) = fs::read_to_string(&source) else { continue };
        let Ok(mut v) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
            crate::log!(IDENT, "Skipping {name}: not valid JSON");
            continue;
        };
        let mut changed = false;
        if let Some(faces) = v.get_mut("faces").and_then(Value::as_array_mut) {
            for f in faces {
                if let Some(o) = f.as_object_mut() {
                    o.insert("assetId".to_string(), Value::String(FONT_ASSET.to_string()));
                    changed = true;
                }
            }
        }
        if !changed {
            continue;
        }
        let body = serde_json::to_string_pretty(&v).unwrap_or_default();
        if fs::read_to_string(&out).map(|old| old != body).unwrap_or(true) {
            if let Err(e) = fs::write(&out, body) {
                crate::log!(IDENT, "Failed to write {name}: {e}");
                continue;
            }
        }
        generated.push(name);
    }
    crate::log!(IDENT, "Custom font active for {} font family file(s)", generated.len());
    meta.generated = generated;
    save_meta(&meta);
}

// ------------------------------------------------------------------ used by the settings page

pub fn state_json() -> Value {
    let m = load_meta();
    let backup = [false, true].iter().any(|s| {
        let g = app::dist_state(*s).prop.version_guid.clone();
        !g.is_empty() && !json_names(&backup_dir(&g)).is_empty()
    });
    json!({
        "applied": mod_font_path().is_file(),
        "family": m.font_family,
        "file": m.font_file,
        "backup": backup,
    })
}

/// Checks and stores `src` as the custom font. Returns its family name.
pub fn set_custom_font(src: &Path) -> Result<String, String> {
    const INVALID: &str = "invalid";
    let len = fs::metadata(src).map_err(|e| e.to_string())?.len();
    if len == 0 || len > MAX_FONT_BYTES {
        return Err(INVALID.into());
    }
    let bytes = fs::read(src).map_err(|e| e.to_string())?;
    let family = inspect(&bytes).map_err(|_| INVALID.to_string())?;

    // back the originals up now, while the Roblox folder is still untouched
    for studio in [false, true] {
        let guid = app::dist_state(studio).prop.version_guid.clone();
        if !guid.is_empty() {
            backup_originals(&paths::get().versions.join(&guid), &guid, &modified_set(studio));
        }
    }

    fs::create_dir_all(mods_root()).map_err(|e| e.to_string())?;
    fs::write(mod_font_path(), &bytes).map_err(|e| e.to_string())?;

    let mut meta = load_meta();
    meta.font_file = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    meta.font_family = family.clone();
    save_meta(&meta);
    Ok(if family.is_empty() { meta.font_file } else { family })
}

/// Removes the custom font and puts Roblox's own font files back from the backup.
pub fn reset_custom_font() -> Result<(), String> {
    const IDENT: &str = "Fonts::Reset";
    let mut meta = load_meta();
    let generated = meta.generated.clone();

    let _ = fs::remove_file(mod_font_path());
    remove_generated(&mut meta);
    meta.font_file.clear();
    meta.font_family.clear();
    save_meta(&meta);

    for studio in [false, true] {
        let guid = app::dist_state(studio).prop.version_guid.clone();
        if guid.is_empty() {
            continue;
        }
        let version_dir = paths::get().versions.join(&guid);
        if !version_dir.is_dir() {
            continue;
        }
        let fam = version_dir.join("content").join("fonts").join("families");
        let backup = backup_dir(&guid);
        let mut restored: Vec<String> = Vec::new();
        for name in json_names(&backup) {
            if fs::create_dir_all(&fam).is_ok() && fs::copy(backup.join(&name), fam.join(&name)).is_ok() {
                restored.push(family_rel(&name));
            }
        }
        let _ = fs::remove_file(version_dir.join("content").join("fonts").join("CustomFont.ttf"));
        crate::log!(IDENT, "Restored {} original font file(s) for {guid}", restored.len());

        // these are no longer mods, so the bootstrapper must not try to "restore" them again
        let mut gone: HashSet<String> = restored.iter().map(|s| norm(s)).collect();
        gone.insert(norm("content\\fonts\\CustomFont.ttf"));
        for g in &generated {
            gone.insert(norm(&family_rel(g)));
        }
        let mut ds = app::dist_state(studio);
        ds.prop.mod_manifest.retain(|p| !gone.contains(&norm(p)));
        ds.save();
    }
    Ok(())
}
