//! The window you get when you open the installed app with no arguments (Bloxstrap's launch menu):
//! logo and links on the left, Launch Roblox / Launch Roblox Studio / Configure settings on the right.

use std::process::Command;

use eframe::egui::{self, Align, Align2, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2};

use super::theme::*;
use crate::app::{self, PROJECT_DOWNLOAD_LINK, PROJECT_HELP_LINK, PROJECT_NAME, PROJECT_SUPPORT_LINK};
use crate::paths;

struct HomeApp {
    logo: egui::TextureHandle,
}

/// Starts this exe again with `arg` and closes the menu.
fn start_and_close(ctx: &egui::Context, arg: &str) {
    let _ = Command::new(paths::process()).arg(arg).spawn();
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
}

/// A wide card with an icon, one or two lines of text and a chevron (WPF-UI "CardAction").
fn card_action(ui: &mut egui::Ui, icon: &str, title: &str, desc: &str) -> egui::Response {
    let h = if desc.is_empty() { 52.0 } else { 64.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
    let fill = if resp.hovered() { clr_card_hover() } else { clr_card() };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(6), fill);
    p.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, clr_border()), egui::StrokeKind::Inside);
    let cy = rect.center().y;
    p.text(egui::pos2(rect.left() + 28.0, cy), Align2::CENTER_CENTER, icon, egui::FontId::proportional(20.0), clr_text());
    if desc.is_empty() {
        p.text(egui::pos2(rect.left() + 56.0, cy), Align2::LEFT_CENTER, title, egui::FontId::proportional(15.0), clr_text());
    } else {
        p.text(egui::pos2(rect.left() + 56.0, cy - 9.0), Align2::LEFT_CENTER, title, egui::FontId::proportional(15.0), clr_text());
        p.text(egui::pos2(rect.left() + 56.0, cy + 11.0), Align2::LEFT_CENTER, desc, egui::FontId::proportional(12.5), clr_muted());
    }
    p.text(egui::pos2(rect.right() - 20.0, cy), Align2::CENTER_CENTER, "\u{203A}", egui::FontId::proportional(20.0), clr_muted());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

impl eframe::App for HomeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().frame(Frame::NONE.fill(clr_bg())).show_inside(ui, |ui| {
            let total = ui.available_size();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                // left: logo, name, version, links
                ui.allocate_ui_with_layout(Vec2::new(280.0, total.y), Layout::top_down(Align::Center), |ui| {
                    ui.add_space(((total.y - 170.0) / 2.0).max(8.0));
                    ui.horizontal(|ui| {
                        ui.add_space(48.0);
                        ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(64.0)));
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.add_space(6.0);
                            ui.label(RichText::new(PROJECT_NAME).size(24.0).color(clr_text()));
                            ui.label(RichText::new(format!("Version {}", app::VERSION)).size(12.0).color(clr_muted()));
                        });
                    });
                    ui.add_space(32.0);
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);
                    ui.hyperlink_to("About Ruststrap", PROJECT_DOWNLOAD_LINK);
                    ui.hyperlink_to("Ruststrap Wiki", PROJECT_HELP_LINK);
                    ui.hyperlink_to("Report a problem", PROJECT_SUPPORT_LINK);
                });

                // right: actions
                ui.allocate_ui_with_layout(Vec2::new(total.x - 280.0, total.y), Layout::top_down(Align::Min), |ui| {
                    Frame::NONE.inner_margin(Margin::same(16)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                        if card_action(ui, "\u{2192}", "Launch Roblox", "").clicked() {
                            start_and_close(&ctx, "-player");
                        }
                        if card_action(ui, "\u{1F527}", "Launch Roblox Studio", "").clicked() {
                            start_and_close(&ctx, "-studio");
                        }
                        if card_action(ui, "\u{2699}", "Configure settings", "").clicked() {
                            start_and_close(&ctx, "-settings");
                        }
                        ui.add_space(16.0);
                        if card_action(ui, "?", "Having an issue?", "See the Wiki for help").clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(PROJECT_HELP_LINK));
                        }
                    });
                });
            });
        });
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        clr_bg().to_normalized_gamma_f32()
    }
}

pub fn run() -> i32 {
    let opts = super::native_options(PROJECT_NAME, [580.0, 340.0], [580.0, 340.0], false, true);
    let res = eframe::run_native(
        PROJECT_NAME,
        opts,
        Box::new(|cc| {
            let theme = app::settings().prop.theme.clone();
            super::theme::set_mode(&theme);
            super::theme::apply(&cc.egui_ctx);
            Ok(Box::new(HomeApp { logo: super::icon_texture(&cc.egui_ctx) }))
        }),
    );
    if let Err(e) = res {
        crate::log!("Ui::Home", "Window failed: {e}");
        return 1;
    }
    0
}
