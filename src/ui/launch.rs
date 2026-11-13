//! The small progress window shown while Roblox is installed / updated / started.
//! Layout follows Bloxstrap's "Fluent" dialog: big icon, one line of text, a bar, Cancel.

use std::thread;
use std::time::Duration;

use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2};

use super::theme::*;
use super::widgets;
use crate::app::{self, error_code, PROJECT_NAME};
use crate::bootstrapper::Bootstrapper;
use crate::models::LaunchMode;
use crate::progress;

enum Phase {
    Confirm,
    Running,
}

struct LaunchApp {
    mode: LaunchMode,
    logo: egui::TextureHandle,
    phase: Phase,
}

impl LaunchApp {
    fn start(&mut self) {
        let mode = self.mode;
        self.phase = Phase::Running;
        thread::spawn(move || {
            let code = Bootstrapper::new(mode).run();
            progress::finish(code);
        });
    }
}

/// Indeterminate bars slide, determinate ones fill.
fn bar(ui: &mut egui::Ui, fraction: Option<f32>, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 10.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(5), clr_border());
    match fraction {
        Some(f) => {
            let mut r = rect;
            r.set_width((rect.width() * f.clamp(0.0, 1.0)).max(10.0));
            p.rect_filled(r, CornerRadius::same(5), color);
        }
        None => {
            let t = ui.input(|i| i.time) as f32;
            let w = rect.width() * 0.3;
            let x = egui::lerp(rect.left()..=(rect.right() - w), (t * 0.9).fract());
            let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(w, rect.height()));
            p.rect_filled(r, CornerRadius::same(5), color);
        }
    }
}

impl eframe::App for LaunchApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let snap = progress::snapshot();
        ctx.request_repaint_after(Duration::from_millis(33));

        if let Some(code) = snap.finished {
            // success or the user cancelled: nothing more to say
            if code == error_code::clr_success() || progress::cancelled() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(clr_bg()).stroke(Stroke::new(1.0, clr_border())).inner_margin(Margin::symmetric(32, 16)))
            .show_inside(ui, |ui| {
                // no title bar: let the whole window be dragged
                let drag = ui.interact(ui.max_rect(), egui::Id::new("drag-window"), Sense::click_and_drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                let top = ui.max_rect().min;
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(egui::pos2(top.x - 32.0, top.y - 16.0), Vec2::new(ui.max_rect().width() + 64.0, 3.0)),
                    CornerRadius::ZERO,
                    clr_accent(),
                );

                ui.vertical_centered(|ui| {
                    ui.add_space(14.0);
                    ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(80.0)));
                    ui.add_space(18.0);

                    let failed = snap.error.is_some() && snap.finished.is_some();
                    match self.phase {
                        Phase::Confirm => {
                            ui.label(RichText::new("Roblox wants to start a game. Launch it?").size(18.0).color(clr_text()));
                            ui.add_space(22.0);
                            ui.horizontal(|ui| {
                                ui.add_space((ui.available_width() - 216.0) / 2.0);
                                if widgets::primary_button(ui, "Launch").clicked() {
                                    self.start();
                                }
                                if widgets::button(ui, "Cancel").clicked() {
                                    progress::cancel();
                                    progress::finish(error_code::CANCELLED);
                                }
                            });
                        }
                        Phase::Running if failed => {
                            let msg = snap.error.clone().unwrap_or_default();
                            ui.label(RichText::new("Something went wrong").size(18.0).color(clr_danger()));
                            ui.label(RichText::new(msg).small().color(clr_muted()));
                            ui.add_space(10.0);
                            bar(ui, Some(1.0), clr_danger());
                            ui.add_space(12.0);
                            if widgets::button(ui, "Close").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        Phase::Running => {
                            let text = if snap.message.is_empty() { "Starting...".to_string() } else { snap.message.clone() };
                            ui.label(RichText::new(text).size(18.0).color(clr_text()));
                            ui.add_space(8.0);
                            bar(ui, snap.fraction, clr_accent());
                            ui.add_space(14.0);
                            ui.add_enabled_ui(snap.can_cancel, |ui| {
                                if widgets::button(ui, "Cancel").clicked() {
                                    progress::cancel();
                                }
                            });
                        }
                    }
                });
            });
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        clr_bg().to_normalized_gamma_f32()
    }
}

/// Shows the window and runs the bootstrapper. Returns the process exit code.
pub fn run(mode: LaunchMode) -> i32 {
    let ask = {
        let s = app::settings();
        s.prop.confirm_launches && !app::launch().roblox_launch_args.is_empty() && mode == LaunchMode::Player
    };

    let (title, theme) = {
        let s = app::settings();
        let t = s.prop.bootstrapper_title.trim().to_string();
        (if t.is_empty() { PROJECT_NAME.to_string() } else { t }, s.prop.theme.clone())
    };
    let opts = super::native_options(&title, [500.0, 280.0], [500.0, 280.0], false, false);
    let res = eframe::run_native(
        &title,
        opts,
        Box::new(move |cc| {
            super::theme::set_mode(&theme);
            super::theme::apply(&cc.egui_ctx);
            let logo = super::icon_texture(&cc.egui_ctx);
            let mut a = LaunchApp { mode, logo, phase: if ask { Phase::Confirm } else { Phase::Running } };
            if !ask {
                a.start();
            }
            Ok(Box::new(a))
        }),
    );
    if let Err(e) = res {
        crate::log!("Ui::Launch", "Window failed: {e}");
        // no window available: still try to start Roblox
        return Bootstrapper::new(mode).run();
    }
    progress::snapshot().finished.unwrap_or(error_code::CANCELLED)
}

#[allow(dead_code)]
fn _layout_hint() -> Layout {
    Layout::top_down(Align::Center)
}
