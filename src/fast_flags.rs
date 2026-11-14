//! Port of `FastFlagManager.cs` (the data side; there is no GUI editor, see `ruststrap flags`).

use crate::app;
use crate::models::FastFlags;

pub const PRESET_FLAGS: &[(&str, &str)] = &[
    ("Rendering.ManualFullscreen", "FFlagHandleAltEnterFullscreenManually"),
    ("Rendering.DisableScaling", "DFFlagDisableDPIScale"),
    ("Rendering.MSAA", "FIntDebugForceMSAASamples"),
    ("Rendering.TextureQuality.OverrideEnabled", "DFFlagTextureQualityOverrideEnabled"),
    ("Rendering.TextureQuality.Level", "DFIntTextureQualityOverride"),
];

/// "Low-spec" preset: shadows off, lowest render quality, lowest textures, no grass, no post effects.
/// Roblox renames or drops flags between updates, so every entry here is a best guess that must be tested.
/// A flag Roblox does not know is simply ignored, so a stale entry is harmless.
pub const LOW_SPEC_FLAGS: &[(&str, &str)] = &[
    ("DFFlagTextureQualityOverrideEnabled", "True"),
    ("DFIntTextureQualityOverride", "0"),
    ("FIntDebugForceMSAASamples", "1"),
    ("FIntRenderShadowIntensity", "0"),
    ("DFIntDebugFRMQualityLevelOverride", "1"),
    ("FIntFRMMinGrassDistance", "0"),
    ("FIntFRMMaxGrassDistance", "0"),
    ("FIntRenderGrassDetailStrands", "0"),
    ("FFlagDisablePostFx", "True"),
];

pub fn set_value(flags: &mut FastFlags, key: &str, value: Option<&str>) {
    const IDENT: &str = "FastFlagManager::SetValue";
    match value {
        None => {
            if flags.remove(key).is_some() {
                crate::log!(IDENT, "Deletion of '{key}' is pending");
            }
        }
        Some(v) => {
            crate::log!(IDENT, "Setting of '{key}' to '{v}' is pending");
            flags.insert(key.to_string(), serde_json::Value::String(v.to_string()));
        }
    }
}

pub fn get_value(flags: &FastFlags, key: &str) -> Option<String> {
    flags.get(key).and_then(|v| match v {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    })
}

pub fn preset_flag(name: &str) -> Option<&'static str> {
    PRESET_FLAGS.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

/// Roblox expects every flag value as a string; call before saving.
pub fn normalise(flags: &mut FastFlags) {
    for v in flags.values_mut() {
        if !v.is_string() {
            *v = serde_json::Value::String(match &*v {
                serde_json::Value::Bool(b) => if *b { "True".into() } else { "False".into() },
                other => other.to_string(),
            });
        }
    }
}

/// `ruststrap flags <list|get|set|remove|preset>`
pub fn cli(args: &[String]) -> i32 {
    let mut ff = app::fast_flags();
    match args.first().map(String::as_str) {
        Some("list") | None => {
            for (k, v) in ff.prop.iter() {
                println!("{k} = {}", get_value(&ff.prop, k).unwrap_or_else(|| v.to_string()));
            }
            0
        }
        Some("get") if args.len() >= 2 => {
            match get_value(&ff.prop, &args[1]) {
                Some(v) => { println!("{v}"); 0 }
                None => { eprintln!("not set"); 1 }
            }
        }
        Some("set") if args.len() >= 3 => {
            set_value(&mut ff.prop, &args[1], Some(&args[2]));
            normalise(&mut ff.prop);
            ff.save();
            println!("ok. Applied the next time Roblox is launched through Ruststrap.");
            0
        }
        Some("remove") if args.len() >= 2 => {
            set_value(&mut ff.prop, &args[1], None);
            ff.save();
            println!("ok");
            0
        }
        Some("preset") if args.len() >= 3 => {
            let (name, val) = (&args[1], &args[2]);
            match name.as_str() {
                "msaa" => {
                    let v = if val == "default" { None } else { Some(val.as_str()) };
                    set_value(&mut ff.prop, preset_flag("Rendering.MSAA").unwrap(), v);
                }
                "texture" => {
                    if val == "default" {
                        set_value(&mut ff.prop, preset_flag("Rendering.TextureQuality.OverrideEnabled").unwrap(), None);
                        set_value(&mut ff.prop, preset_flag("Rendering.TextureQuality.Level").unwrap(), None);
                    } else {
                        set_value(&mut ff.prop, preset_flag("Rendering.TextureQuality.OverrideEnabled").unwrap(), Some("True"));
                        set_value(&mut ff.prop, preset_flag("Rendering.TextureQuality.Level").unwrap(), Some(val));
                    }
                }
                "lowspec" => {
                    for (k, v) in LOW_SPEC_FLAGS {
                        set_value(&mut ff.prop, k, if val == "on" { Some(*v) } else { None });
                    }
                }
                "disable-scaling" => {
                    let v = if val == "on" { Some("True") } else { None };
                    set_value(&mut ff.prop, preset_flag("Rendering.DisableScaling").unwrap(), v);
                }
                _ => {
                    eprintln!("presets: msaa <default|1|2|4>, texture <default|0..3>, disable-scaling <on|off>, lowspec <on|off>");
                    return 1;
                }
            }
            normalise(&mut ff.prop);
            ff.save();
            println!("ok");
            0
        }
        _ => {
            eprintln!("usage: ruststrap flags list | get <flag> | set <flag> <value> | remove <flag> | preset <name> <value>");
            1
        }
    }
}
