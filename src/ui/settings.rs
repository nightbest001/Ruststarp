//! The settings window. Layout follows Bloxstrap: navigation on the left, one page on the right,
//! a bar with Save / Close at the bottom.

use std::process::Command;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2};

use super::theme::*;
use super::widgets;
use crate::app::{self, PROJECT_NAME};
use crate::models::{FastFlags, Settings};
use crate::{fast_flags, gpu, installer, paths, shortcuts};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Integrations,
    Behaviour,
    Mods,
    FastFlags,
    FastFlagEditor,
    Appearance,
    Shortcuts,
    Gpu,
    Ruststrap,
    About,
}

pub enum Modal {
    Uninstall,
    ResetFlags,
    Unsaved,
}

pub struct SettingsApp {
    pub page: Page,
    pub settings: Settings,
    pub force_reinstall: bool,
    pub flags: FastFlags,
    pub flag_filter: String,
    pub new_flag_name: String,
    pub new_flag_value: String,
    pub adapters: Vec<gpu::Adapter>,
    /// Desktop, Start Menu, Play, Studio, Settings (same order as `shortcuts::ALL`).
    pub shortcuts: [bool; 5],
    pub export_config: bool,
    pub export_logs: bool,
    pub sel_integration: Option<usize>,
    pub toast: Option<(String, bool, Instant)>,
    pub modal: Option<Modal>,
    pub installed: bool,
    pub logo: egui::TextureHandle,
    pub saved_json: String,
    pub force_close: bool,
}

impl SettingsApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = app::settings().prop.clone();
        super::theme::set_mode(&settings.theme);
        super::theme::apply(&cc.egui_ctx);
        let force_reinstall = app::state().prop.force_reinstall;
        let flags = app::fast_flags().prop.clone();
        let mut a = Self {
            page: Page::Integrations,
            settings,
            force_reinstall,
            flags,
            flag_filter: String::new(),
            new_flag_name: String::new(),
            new_flag_value: String::new(),
            adapters: gpu::detect(),
            shortcuts: shortcuts::current(),
            export_config: true,
            export_logs: true,
            sel_integration: None,
            toast: None,
            modal: None,
            installed: paths::get().application.exists(),
            logo: super::icon_texture(&cc.egui_ctx),
            saved_json: String::new(),
            force_close: false,
        };
        a.saved_json = a.snapshot_json();
        a
    }

    fn snapshot_json(&self) -> String {
        let s = serde_json::to_string(&self.settings).unwrap_or_default();
        let f = serde_json::to_string(&self.flags).unwrap_or_default();
        format!("{s}|{f}|{}|{:?}", self.force_reinstall, self.shortcuts)
    }

    pub fn dirty(&self) -> bool {
        self.snapshot_json() != self.saved_json
    }

    pub fn say(&mut self, msg: &str, ok: bool) {
        self.toast = Some((msg.to_string(), ok, Instant::now()));
    }

    pub fn save(&mut self) {
        let pref = gpu::Pref::parse(&self.settings.gpu_preference).unwrap_or(gpu::Pref::Default);
        self.settings.gpu_preference = pref.as_str().to_string();
        fast_flags::normalise(&mut self.flags);
        {
            let mut s = app::settings();
            s.prop = self.settings.clone();
            s.save();
        }
        {
            let mut st = app::state();
            st.prop.force_reinstall = self.force_reinstall;
            st.save();
        }
        {
            let mut ff = app::fast_flags();
            ff.prop = self.flags.clone();
            ff.save();
        }
        let mut failed = gpu::apply_installed(pref);
        failed.extend(shortcuts::apply(&self.shortcuts));
        self.saved_json = self.snapshot_json();
        if failed.is_empty() {
            self.say("Settings saved", true);
        } else {
            self.say("Saved, but something could not be written, see the log", false);
        }
    }

    fn nav_item(&mut self, ui: &mut egui::Ui, page: Page, label: &str) {
        let active = self.page == page || (page == Page::FastFlags && self.page == Page::FastFlagEditor);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
        let p = ui.painter();
        if active {
            p.rect_filled(rect, CornerRadius::same(6), Color32::from_rgba_unmultiplied(0xE5, 0x57, 0x1B, 40));
            let bar = egui::Rect::from_min_size(egui::pos2(rect.left() + 2.0, rect.top() + 9.0), Vec2::new(3.0, 20.0));
            p.rect_filled(bar, CornerRadius::same(2), clr_accent());
        } else if resp.hovered() {
            p.rect_filled(rect, CornerRadius::same(6), clr_card());
        }
        let color = if active && !super::theme::is_light() { Color32::WHITE } else if active { clr_accent_deep() } else { clr_text() };
        p.text(
            egui::pos2(rect.left() + 18.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(15.0),
            color,
        );
        if resp.clicked() {
            self.page = page;
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(34.0)));
            ui.label(RichText::new(PROJECT_NAME).size(19.0).strong().color(clr_text()));
        });
        ui.add_space(14.0);
        self.nav_item(ui, Page::Integrations, "Integrations");
        self.nav_item(ui, Page::Behaviour, "Behaviour");
        self.nav_item(ui, Page::Mods, "Mods");
        self.nav_item(ui, Page::FastFlags, "FastFlags");
        self.nav_item(ui, Page::Appearance, "Appearance");
        self.nav_item(ui, Page::Shortcuts, "Shortcuts");
        self.nav_item(ui, Page::Gpu, "GPU");
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(4.0);
            self.nav_item(ui, Page::About, "About");
            self.nav_item(ui, Page::Ruststrap, "Ruststrap");
        });
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_centered(|ui| {
            if !self.installed {
                ui.label(RichText::new("Not installed yet: open the Ruststrap page and press Install.").color(clr_accent_hover()));
            } else if self.dirty() {
                ui.label(RichText::new("You have unsaved changes").color(clr_accent_hover()));
            } else {
                ui.label(RichText::new("All changes saved").color(clr_muted()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(ui, "Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let dirty = self.dirty();
                ui.add_enabled_ui(dirty, |ui| {
                    if widgets::primary_button(ui, "Save").clicked() {
                        self.save();
                    }
                });
                if widgets::button(ui, "Play Roblox").clicked() {
                    let mut c = Command::new(paths::process());
                    c.arg("-player");
                    match c.spawn() {
                        Ok(_) => {
                            self.force_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(e) => self.say(&format!("Could not start Roblox: {e}"), false),
                    }
                }
            });
        });
    }

    fn overlays(&mut self, ctx: &egui::Context) {
        // toast
        if let Some((msg, ok, at)) = self.toast.clone() {
            if at.elapsed() > Duration::from_secs(3) {
                self.toast = None;
            } else {
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(Align2::CENTER_BOTTOM, Vec2::new(110.0, -70.0))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        Frame::NONE
                            .fill(clr_card_hover())
                            .stroke(Stroke::new(1.0, if ok { clr_success() } else { clr_danger() }))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(16, 10))
                            .show(ui, |ui| {
                                ui.label(RichText::new(msg).color(clr_text()));
                            });
                    });
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }

        // modal dialogs
        let Some(kind) = self.modal.as_ref().map(|m| match m {
            Modal::Uninstall => 0,
            Modal::ResetFlags => 1,
            Modal::Unsaved => 2,
        }) else {
            return;
        };
        let (title, body) = match kind {
            0 => ("Uninstall Ruststrap?", "Ruststrap and the Roblox it installed will be removed."),
            1 => ("Reset all FastFlags?", "Every FastFlag in the list will be deleted when you save."),
            _ => ("Unsaved changes", "Save your changes before closing?"),
        };
        let mut action: Option<u8> = None; // 1 = confirm, 2 = discard, 0 = cancel
        egui::Modal::new(egui::Id::new("modal")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(title).size(18.0).strong().color(clr_text()));
            ui.add_space(4.0);
            ui.label(RichText::new(body).color(clr_muted()));
            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kind == 2 {
                    if widgets::primary_button(ui, "Save").clicked() {
                        action = Some(1);
                    }
                    if widgets::button(ui, "Discard").clicked() {
                        action = Some(2);
                    }
                } else if kind == 0 {
                    if widgets::danger_button(ui, "Uninstall").clicked() {
                        action = Some(1);
                    }
                } else if widgets::primary_button(ui, "Reset").clicked() {
                    action = Some(1);
                }
                if widgets::button(ui, "Cancel").clicked() {
                    action = Some(0);
                }
            });
        });
        let Some(act) = action else { return };
        self.modal = None;
        match (kind, act) {
            (0, 1) => {
                let code = installer::uninstall(true, false);
                if code == app::error_code::SUCCESS {
                    self.force_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    self.say("Uninstall failed, see the log", false);
                }
            }
            (1, 1) => self.flags.clear(),
            (2, 1) => {
                self.save();
                self.force_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            (2, 2) => {
                self.force_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            _ => {}
        }
    }
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // theme preview: follows the Appearance dropdown right away
        let want_light = super::theme::mode_is_light(&self.settings.theme);
        if want_light != super::theme::is_light() {
            super::theme::set_light(want_light);
            super::theme::apply(&ctx);
        }

        // ask before throwing unsaved changes away
        if ctx.input(|i| i.viewport().close_requested()) && !self.force_close && self.dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.modal = Some(Modal::Unsaved);
        }

        egui::Panel::bottom("bar")
            .exact_size(56.0)
            .frame(Frame::NONE.fill(clr_panel()).inner_margin(Margin::symmetric(18, 8)))
            .show_inside(ui, |ui| self.bottom_bar(ui, &ctx));

        egui::Panel::left("nav")
            .exact_size(210.0)
            .resizable(false)
            .frame(Frame::NONE.fill(clr_panel()).inner_margin(Margin::symmetric(12, 14)))
            .show_inside(ui, |ui| self.nav(ui));

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(clr_bg()).inner_margin(Margin::symmetric(26, 16)))
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    match self.page {
                        Page::Integrations => self.page_integrations(ui),
                        Page::Behaviour => self.page_behaviour(ui),
                        Page::Mods => self.page_mods(ui),
                        Page::FastFlags => self.page_fastflags(ui),
                        Page::FastFlagEditor => self.page_flag_editor(ui),
                        Page::Appearance => self.page_appearance(ui),
                        Page::Shortcuts => self.page_shortcuts(ui),
                        Page::Gpu => self.page_gpu(ui),
                        Page::Ruststrap => self.page_ruststrap(ui),
                        Page::About => self.page_about(ui),
                    }
                    ui.add_space(12.0);
                });
            });

        self.overlays(&ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        clr_bg().to_normalized_gamma_f32()
    }
}

/// Opens `path` in Explorer (creating it first).
pub fn open_folder(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path);
    let _ = Command::new("explorer").arg(path).spawn();
}

pub fn run() -> i32 {
    let opts = super::native_options("Ruststrap Settings", [1000.0, 580.0], [860.0, 500.0], true, true);
    let res = eframe::run_native(PROJECT_NAME, opts, Box::new(|cc| Ok(Box::new(SettingsApp::new(cc)))));
    if let Err(e) = res {
        crate::log!("Ui::Settings", "Window failed: {e}");
        return 1;
    }
    0
}
