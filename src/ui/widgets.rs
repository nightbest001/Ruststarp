//! Small building blocks shared by every page.

use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, Response, RichText, Sense, Stroke, Ui};

use super::theme::*;

pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(title).heading().strong().color(TEXT));
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(MUTED));
    }
    ui.add_space(10.0);
}

pub fn section_title(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).size(17.0).strong().color(TEXT));
    ui.add_space(2.0);
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::NONE
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Card with a title and a muted description on the left and a control on the right.
pub fn setting_row(ui: &mut Ui, title: &str, desc: &str, control: impl FnOnce(&mut Ui)) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            let total = ui.available_width();
            ui.allocate_ui_with_layout(egui::vec2((total - 190.0).max(120.0), 0.0), Layout::top_down(Align::Min), |ui| {
                ui.label(RichText::new(title).strong().color(TEXT));
                if !desc.is_empty() {
                    ui.label(RichText::new(desc).small().color(MUTED));
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), control);
        });
    });
    ui.add_space(2.0);
}

/// Switch like the one in Windows settings.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let size = egui::vec2(42.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let off = Color32::from_rgb(0x4A, 0x3E, 0x38);
    let fill = Color32::from_rgb(
        (off.r() as f32 + (ACCENT.r() as f32 - off.r() as f32) * t) as u8,
        (off.g() as f32 + (ACCENT.g() as f32 - off.g() as f32) * t) as u8,
        (off.b() as f32 + (ACCENT.b() as f32 - off.b() as f32) * t) as u8,
    );
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(11), fill);
    let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    painter.circle_filled(egui::pos2(x, rect.center().y), 8.0, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(Color32::WHITE))
            .fill(ACCENT)
            .stroke(Stroke::new(1.0, ACCENT_HOVER))
            .corner_radius(CornerRadius::same(6))
            .min_size(egui::vec2(96.0, 32.0)),
    )
}

pub fn danger_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(Color32::WHITE))
            .fill(Color32::from_rgb(0x9C, 0x2E, 0x2E))
            .stroke(Stroke::new(1.0, DANGER))
            .corner_radius(CornerRadius::same(6))
            .min_size(egui::vec2(96.0, 32.0)),
    )
}

pub fn button(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Button::new(text).corner_radius(CornerRadius::same(6)).min_size(egui::vec2(80.0, 32.0)))
}
