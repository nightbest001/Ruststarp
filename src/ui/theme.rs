//! Rust palette: warm surfaces and an orange accent, like the icon. Dark or light, picked in Appearance.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, CornerRadius, Stroke};

static LIGHT: AtomicBool = AtomicBool::new(false);

pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

pub fn set_light(light: bool) {
    LIGHT.store(light, Ordering::Relaxed);
}

/// Follows the Windows "app mode" setting.
pub fn system_is_light() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v| v != 0)
        .unwrap_or(false)
}

/// `mode` is "system" | "light" | "dark".
pub fn mode_is_light(mode: &str) -> bool {
    match mode.trim().to_lowercase().as_str() {
        "light" => true,
        "dark" => false,
        _ => system_is_light(),
    }
}

pub fn set_mode(mode: &str) {
    set_light(mode_is_light(mode));
}

fn pick(dark: (u8, u8, u8), light: (u8, u8, u8)) -> Color32 {
    let (r, g, b) = if is_light() { light } else { dark };
    Color32::from_rgb(r, g, b)
}

pub fn clr_accent() -> Color32 {
    Color32::from_rgb(0xE5, 0x57, 0x1B)
}
pub fn clr_accent_hover() -> Color32 {
    pick((0xFF, 0x7A, 0x33), (0xD1, 0x4A, 0x12))
}
pub fn clr_accent_deep() -> Color32 {
    pick((0xB8, 0x40, 0x12), (0xB8, 0x40, 0x12))
}
pub fn clr_bg() -> Color32 {
    pick((0x16, 0x12, 0x10), (0xF7, 0xF3, 0xF0))
}
pub fn clr_panel() -> Color32 {
    pick((0x1D, 0x17, 0x14), (0xEE, 0xE7, 0xE2))
}
pub fn clr_card() -> Color32 {
    pick((0x27, 0x1F, 0x1B), (0xFF, 0xFF, 0xFF))
}
pub fn clr_card_hover() -> Color32 {
    pick((0x33, 0x29, 0x24), (0xF3, 0xEB, 0xE6))
}
pub fn clr_border() -> Color32 {
    pick((0x3B, 0x2F, 0x29), (0xDC, 0xCF, 0xC7))
}
pub fn clr_text() -> Color32 {
    pick((0xF4, 0xEC, 0xE7), (0x2A, 0x1F, 0x1A))
}
pub fn clr_muted() -> Color32 {
    pick((0xAE, 0xA0, 0x97), (0x7A, 0x6A, 0x61))
}
pub fn clr_danger() -> Color32 {
    Color32::from_rgb(0xE5, 0x4B, 0x4B)
}
pub fn clr_success() -> Color32 {
    Color32::from_rgb(0x4C, 0xB0, 0x7A)
}

pub fn apply(ctx: &egui::Context) {
    let light = is_light();
    let mut v = if light { egui::Visuals::light() } else { egui::Visuals::dark() };
    v.panel_fill = clr_bg();
    v.window_fill = clr_panel();
    v.window_stroke = Stroke::new(1.0, clr_border());
    v.extreme_bg_color = if light { Color32::WHITE } else { Color32::from_rgb(0x12, 0x0E, 0x0C) };
    v.faint_bg_color = clr_card();
    v.hyperlink_color = clr_accent_hover();
    v.selection.bg_fill = clr_accent_deep();
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);

    let hover_fg = if light { clr_text() } else { Color32::WHITE };
    let r = CornerRadius::same(6);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, clr_text());
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, clr_border());
    v.widgets.inactive.bg_fill = clr_card();
    v.widgets.inactive.weak_bg_fill = clr_card();
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, clr_border());
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, clr_text());
    v.widgets.hovered.bg_fill = clr_card_hover();
    v.widgets.hovered.weak_bg_fill = clr_card_hover();
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, clr_accent());
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, hover_fg);
    v.widgets.active.bg_fill = clr_accent_deep();
    v.widgets.active.weak_bg_fill = clr_accent_deep();
    v.widgets.active.bg_stroke = Stroke::new(1.0, clr_accent());
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.open.bg_fill = clr_card_hover();
    v.widgets.open.weak_bg_fill = clr_card_hover();
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    ctx.set_visuals(v);

    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(14.0, 7.0);
        s.spacing.interact_size.y = 30.0;
        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(14.5));
        s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(14.5));
        s.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(26.0));
        s.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(12.5));
    });
}
