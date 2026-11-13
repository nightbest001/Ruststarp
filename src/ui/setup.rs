//! The setup wizard, shown when Ruststrap is started from somewhere other than its install folder.
//! Same steps as Bloxstrap's installer: Welcome, Install (location + shortcuts), Completion.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2};

use super::theme::*;
use super::widgets;
use crate::app::{error_code, PROJECT_DOWNLOAD_LINK, PROJECT_NAME};
use crate::{paths, registry};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Welcome,
    Install,
    Done,
}

struct SetupApp {
    step: Step,
    logo: egui::TextureHandle,
    location: String,
    desktop_icon: bool,
    start_menu_icon: bool,
    error: String,
    installing: bool,
    rx: Option<Receiver<Result<(), String>>>,
    /// 0 = install and launch Roblox, 1 = open the settings
    after: usize,
    ask_cancel: bool,
    force_close: bool,
    installed_exe: PathBuf,
}

fn exe_in(dir: &str) -> PathBuf {
    PathBuf::from(dir.trim()).join(format!("{PROJECT_NAME}.exe"))
}

/// Runs `ruststrap install -quiet` as its own process so it sees the chosen install folder.
fn run_install(dir: String, desktop: bool, start_menu: bool) -> Result<(), String> {
    let dir = dir.trim().to_string();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {dir}: {e}"))?;
    registry::set_install_location(&dir).map_err(|e| format!("Cannot write the registry: {e}"))?;

    let mut c = Command::new(paths::process());
    c.arg("install").arg("-quiet");
    if desktop {
        c.arg("-desktop");
    }
    if start_menu {
        c.arg("-startmenu");
    }
    let out = c.creation_flags(CREATE_NO_WINDOW).output().map_err(|e| format!("Cannot start the installer: {e}"))?;
    match out.status.code() {
        Some(code) if code == error_code::SUCCESS => Ok(()),
        _ => Err(format!(
            "{PROJECT_NAME} has been installed to this location before and is still present, however the installer \
             cannot overwrite the old executable.\n\nPlease manually delete {PROJECT_NAME}.exe from the install \
             location or try restarting your system, and then retry installation afterwards."
        )),
    }
}

impl SetupApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        super::theme::set_mode(&crate::app::settings().prop.theme);
        super::theme::apply(&cc.egui_ctx);
        let location = paths::default_base().to_string_lossy().to_string();
        Self {
            step: Step::Welcome,
            logo: super::icon_texture(&cc.egui_ctx),
            installed_exe: exe_in(&location),
            location,
            desktop_icon: true,
            start_menu_icon: true,
            error: String::new(),
            installing: false,
            rx: None,
            after: 1,
            ask_cancel: false,
            force_close: false,
        }
    }

    fn start_install(&mut self) {
        self.error.clear();
        if self.location.trim().is_empty() {
            self.error = "Choose a folder first.".to_string();
            return;
        }
        self.installed_exe = exe_in(&self.location);
        self.installing = true;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let (dir, d, s) = (self.location.clone(), self.desktop_icon, self.start_menu_icon);
        thread::spawn(move || {
            let _ = tx.send(run_install(dir, d, s));
        });
    }

    fn finish(&mut self, ctx: &egui::Context) {
        let mut c = Command::new(&self.installed_exe);
        c.arg(if self.after == 0 { "-player" } else { "-menu" });
        let _ = c.spawn();
        self.force_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn step_item(&self, ui: &mut egui::Ui, step: Step, label: &str) {
        let active = self.step == step;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::hover());
        let p = ui.painter();
        if active {
            p.rect_filled(rect, CornerRadius::same(6), Color32::from_rgba_unmultiplied(0xE5, 0x57, 0x1B, 40));
            let bar = egui::Rect::from_min_size(egui::pos2(rect.left() + 2.0, rect.top() + 9.0), Vec2::new(3.0, 20.0));
            p.rect_filled(bar, CornerRadius::same(2), clr_accent());
        }
        let color = if !active {
            clr_muted()
        } else if is_light() {
            clr_accent_deep()
        } else {
            Color32::WHITE
        };
        p.text(
            egui::pos2(rect.left() + 18.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(15.0),
            color,
        );
    }

    fn page_welcome(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Welcome").size(24.0).strong().color(clr_text()));
        ui.add_space(8.0);
        ui.label(RichText::new(format!("Thank you for downloading {PROJECT_NAME}.")).size(14.5).color(clr_text()));
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "The only official place to download it from is the page below, so please verify that you got it \
                 from an official source.",
            )
            .color(clr_text()),
        );
        ui.hyperlink_to(PROJECT_DOWNLOAD_LINK, PROJECT_DOWNLOAD_LINK);
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "This installation process will be quick and simple, and you will be able to configure any of \
                 {PROJECT_NAME}'s settings after installation."
            ))
            .color(clr_text()),
        );
        ui.add_space(10.0);
        ui.label(RichText::new("Please click 'Next' to continue.").color(clr_muted()));
    }

    fn page_install(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(!self.installing, |ui| {
            ui.label(RichText::new("Choose where to install to").size(20.0).strong().color(clr_text()));
            ui.label(
                RichText::new(
                    "Roblox will also be installed to this path. Change this if you prefer to install all your games \
                     to a separate drive. Otherwise, it's recommended that you keep this as it is.",
                )
                .color(clr_text()),
            );
            ui.add_space(4.0);
            widgets::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.location).desired_width((ui.available_width() - 190.0).max(100.0)));
                    if widgets::button(ui, "Browse").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.location = p.to_string_lossy().to_string();
                        }
                    }
                    if widgets::button(ui, "Reset").clicked() {
                        self.location = paths::standard_base().to_string_lossy().to_string();
                    }
                });
            });
            if PathBuf::from(self.location.trim()).join("Settings.json").is_file() {
                ui.label(RichText::new("Existing data found. Your mods and settings will be restored.").color(clr_text()));
            }
            if !self.error.is_empty() {
                ui.label(RichText::new(&self.error).color(clr_danger()));
            }

            widgets::section_title(ui, "Shortcuts");
            ui.label(
                RichText::new(
                    "These are general shortcuts that open the app. Shortcuts for specific functions can be created \
                     later in the settings.",
                )
                .color(clr_text()),
            );
            ui.add_space(4.0);
            ui.columns(2, |cols| {
                widgets::card(&mut cols[0], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Desktop icon").strong().color(clr_text()));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            widgets::toggle(ui, &mut self.desktop_icon);
                        });
                    });
                });
                widgets::card(&mut cols[1], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Start Menu icon").strong().color(clr_text()));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            widgets::toggle(ui, &mut self.start_menu_icon);
                        });
                    });
                });
            });
        });
        if self.installing {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Installing...").color(clr_text()));
            });
        }
    }

    fn page_done(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Completion").size(24.0).strong().color(clr_text()));
        ui.add_space(8.0);
        ui.label(RichText::new(format!("{PROJECT_NAME} has successfully been installed.")).color(clr_text()));
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "Roblox has not yet been installed, that will happen when you launch it with {PROJECT_NAME} for the \
                 first time. However, before you do that, you may want to configure {PROJECT_NAME}'s settings first."
            ))
            .color(clr_text()),
        );
        ui.add_space(6.0);
        ui.label(RichText::new("What would you like to do?").strong().color(clr_text()));
        ui.add_space(6.0);
        if choice_card(
            ui,
            self.after == 1,
            &format!("Configure {PROJECT_NAME}'s settings"),
            "Tweak with all the features it has to offer",
        )
        .clicked()
        {
            self.after = 1;
        }
        if choice_card(
            ui,
            self.after == 0,
            "Install and launch Roblox",
            "Will drop you into the desktop app once everything's done",
        )
        .clicked()
        {
            self.after = 0;
        }
    }
}

/// A card you click to pick it; the picked one gets an orange outline.
fn choice_card(ui: &mut egui::Ui, selected: bool, title: &str, desc: &str) -> egui::Response {
    let stroke = if selected { Stroke::new(2.0, clr_accent()) } else { Stroke::new(1.0, clr_border()) };
    let r = Frame::NONE
        .fill(clr_card())
        .stroke(stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong().color(clr_text()));
            ui.label(RichText::new(desc).small().color(clr_muted()));
        });
    let resp = ui.interact(r.response.rect, ui.id().with(title), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

impl eframe::App for SetupApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // result of the background install
        if let Some(res) = self.rx.as_ref().map(|rx| rx.try_recv()) {
            match res {
                Ok(Ok(())) => {
                    self.installing = false;
                    self.rx = None;
                    self.step = Step::Done;
                }
                Ok(Err(msg)) => {
                    self.installing = false;
                    self.rx = None;
                    self.error = msg;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => {
                    self.installing = false;
                    self.rx = None;
                    self.error = "The installer stopped unexpectedly.".to_string();
                }
            }
        }

        // closing the window before the end counts as cancel
        if ctx.input(|i| i.viewport().close_requested()) && !self.force_close && self.step != Step::Done {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.installing {
                self.ask_cancel = true;
            }
        }

        egui::Panel::bottom("setup-bar")
            .exact_size(56.0)
            .frame(Frame::NONE.fill(clr_panel()).inner_margin(Margin::symmetric(18, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| match self.step {
                        Step::Welcome => {
                            if widgets::primary_button(ui, "Next").clicked() {
                                self.step = Step::Install;
                            }
                            if widgets::button(ui, "Cancel").clicked() {
                                self.ask_cancel = true;
                            }
                        }
                        Step::Install => {
                            ui.add_enabled_ui(!self.installing, |ui| {
                                if widgets::primary_button(ui, "Install").clicked() {
                                    self.start_install();
                                }
                                if widgets::button(ui, "Cancel").clicked() {
                                    self.ask_cancel = true;
                                }
                            });
                        }
                        Step::Done => {
                            if widgets::primary_button(ui, "Finish").clicked() {
                                self.finish(&ctx);
                            }
                        }
                    });
                });
            });

        egui::Panel::left("setup-nav")
            .exact_size(210.0)
            .resizable(false)
            .frame(Frame::NONE.fill(clr_panel()).inner_margin(Margin::symmetric(12, 14)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(34.0)));
                    ui.label(RichText::new(PROJECT_NAME).size(19.0).strong().color(clr_text()));
                });
                ui.add_space(14.0);
                self.step_item(ui, Step::Welcome, "Welcome");
                self.step_item(ui, Step::Install, "Install");
                self.step_item(ui, Step::Done, "Completion");
            });

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(clr_bg()).inner_margin(Margin::symmetric(26, 16)))
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.step {
                    Step::Welcome => self.page_welcome(ui),
                    Step::Install => self.page_install(ui),
                    Step::Done => self.page_done(ui),
                });
            });

        if self.ask_cancel {
            let mut answer: Option<bool> = None;
            egui::Modal::new(egui::Id::new("setup-cancel")).show(&ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new("Cancel setup?").size(18.0).strong().color(clr_text()));
                ui.add_space(4.0);
                ui.label(RichText::new("Are you sure you want to cancel the installation?").color(clr_muted()));
                ui.add_space(14.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::danger_button(ui, "Yes").clicked() {
                        answer = Some(true);
                    }
                    if widgets::button(ui, "No").clicked() {
                        answer = Some(false);
                    }
                });
            });
            match answer {
                Some(true) => {
                    self.force_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Some(false) => self.ask_cancel = false,
                None => {}
            }
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        clr_bg().to_normalized_gamma_f32()
    }
}

pub fn run() -> i32 {
    let opts = super::native_options("Ruststrap Setup", [840.0, 540.0], [840.0, 540.0], false, true);
    let res = eframe::run_native("Ruststrap Setup", opts, Box::new(|cc| Ok(Box::new(SetupApp::new(cc)))));
    if let Err(e) = res {
        crate::log!("Ui::Setup", "Window failed: {e}");
        return 1;
    }
    0
}
