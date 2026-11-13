//! Rust palette: warm near-black surfaces and an orange accent, like the icon.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(0xE5, 0x57, 0x1B);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xFF, 0x7A, 0x33);
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(0xB8, 0x40, 0x12);
pub const BG: Color32 = Color32::from_rgb(0x16, 0x12, 0x10);
pub const PANEL: Color32 = Color32::from_rgb(0x1D, 0x17, 0x14);
pub const CARD: Color32 = Color32::from_rgb(0x27, 0x1F, 0x1B);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x33, 0x29, 0x24);
pub const BORDER: Color32 = Color32::from_rgb(0x3B, 0x2F, 0x29);
pub const TEXT: Color32 = Color32::from_rgb(0xF4, 0xEC, 0xE7);
pub const MUTED: Color32 = Color32::from_rgb(0xAE, 0xA0, 0x97);
pub const DANGER: Color32 = Color32::from_rgb(0xE5, 0x4B, 0x4B);
pub const SUCCESS: Color32 = Color32::from_rgb(0x4C, 0xB0, 0x7A);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.extreme_bg_color = Color32::from_rgb(0x12, 0x0E, 0x0C);
    v.faint_bg_color = CARD;
    v.hyperlink_color = ACCENT_HOVER;
    v.selection.bg_fill = ACCENT_DEEP;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);

    let r = CornerRadius::same(6);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.hovered.bg_fill = CARD_HOVER;
    v.widgets.hovered.weak_bg_fill = CARD_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.bg_fill = ACCENT_DEEP;
    v.widgets.active.weak_bg_fill = ACCENT_DEEP;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.open.bg_fill = CARD_HOVER;
    v.widgets.open.weak_bg_fill = CARD_HOVER;
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
