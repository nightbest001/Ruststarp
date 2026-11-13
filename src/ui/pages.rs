//! One function per settings page.

use eframe::egui::{self, Align, Layout, RichText, Vec2};

use super::settings::{open_folder, Page, SettingsApp};
use super::theme::*;
use super::widgets::*;
use crate::models::CustomIntegration;
use crate::{fast_flags, gpu, installer, paths};

impl SettingsApp {
    // ------------------------------------------------------------ Integrations
    pub fn page_integrations(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Integrations", "Extra features that run next to Roblox.");

        setting_row(
            ui,
            "Activity tracking",
            "Reads the Roblox log to know when you join or leave a game. Needed to close integrations when Roblox exits.",
            |ui| {
                toggle(ui, &mut self.settings.enable_activity_tracking);
            },
        );
        setting_row(
            ui,
            "Close Roblox when it returns to the app",
            "Closes Roblox instead of leaving its home screen open after you leave a game.",
            |ui| {
                ui.add_enabled_ui(self.settings.enable_activity_tracking, |ui| {
                    toggle(ui, &mut self.settings.use_disable_app_patch);
                });
            },
        );

        section_title(ui, "Custom integrations");
        ui.label(RichText::new("Programs that start together with Roblox.").color(MUTED));
        ui.add_space(6.0);

        card(ui, |ui| {
            ui.horizontal_top(|ui| {
                // list
                ui.allocate_ui_with_layout(Vec2::new(200.0, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(200.0);
                    for i in 0..self.settings.custom_integrations.len() {
                        let name = {
                            let n = &self.settings.custom_integrations[i].name;
                            if n.trim().is_empty() { "(unnamed)".to_string() } else { n.clone() }
                        };
                        let selected = self.sel_integration == Some(i);
                        if ui.selectable_label(selected, name).clicked() {
                            self.sel_integration = Some(i);
                        }
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if button(ui, "New").clicked() {
                            self.settings.custom_integrations.push(CustomIntegration::default());
                            self.sel_integration = Some(self.settings.custom_integrations.len() - 1);
                        }
                        if let Some(i) = self.sel_integration {
                            if i < self.settings.custom_integrations.len() && button(ui, "Delete").clicked() {
                                self.settings.custom_integrations.remove(i);
                                self.sel_integration = None;
                            }
                        }
                    });
                });
                ui.separator();
                // editor
                ui.vertical(|ui| {
                    let idx = self.sel_integration.filter(|i| *i < self.settings.custom_integrations.len());
                    let Some(i) = idx else {
                        ui.label(RichText::new("Pick an integration, or press New.").color(MUTED));
                        return;
                    };
                    let it = &mut self.settings.custom_integrations[i];
                    ui.label(RichText::new("Name").color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut it.name).desired_width(f32::INFINITY));
                    ui.label(RichText::new("App location").color(MUTED));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut it.location).desired_width(ui.available_width() - 100.0));
                        if button(ui, "Browse").clicked() {
                            if let Some(p) = rfd::FileDialog::new().pick_file() {
                                it.location = p.to_string_lossy().to_string();
                            }
                        }
                    });
                    ui.label(RichText::new("Launch arguments").color(MUTED));
                    ui.add(egui::TextEdit::multiline(&mut it.launch_args).desired_rows(3).desired_width(f32::INFINITY));
                    ui.horizontal(|ui| {
                        toggle(ui, &mut it.auto_close);
                        ui.label("Close it when Roblox closes");
                    });
                });
            });
        });
    }

    // ------------------------------------------------------------ Behaviour
    pub fn page_behaviour(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Behaviour", "How Ruststrap starts and updates Roblox.");
        setting_row(
            ui,
            "Confirm before launching",
            "Asks first when a website or a friend opens a game through Ruststrap.",
            |ui| {
                toggle(ui, &mut self.settings.confirm_launches);
            },
        );
        setting_row(
            ui,
            "Reinstall Roblox on next launch",
            "Downloads Roblox again. Use it if the game will not start or files look broken.",
            |ui| {
                toggle(ui, &mut self.force_reinstall);
            },
        );
        setting_row(
            ui,
            "Keep downloaded packages",
            "Keeps old package files in the cache instead of deleting them after an update.",
            |ui| {
                toggle(ui, &mut self.settings.debug_disable_version_package_cleanup);
            },
        );
    }

    // ------------------------------------------------------------ Mods
    pub fn page_mods(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Mods", "Replace Roblox files with your own.");
        let mods = paths::get().modifications.clone();
        setting_row(
            ui,
            "Open mods folder",
            &mods.to_string_lossy(),
            |ui| {
                if button(ui, "Open").clicked() {
                    open_folder(&mods);
                }
            },
        );
        card(ui, |ui| {
            ui.label(RichText::new("How it works").strong().color(TEXT));
            ui.label(
                RichText::new(
                    "Every file you put in the mods folder replaces the file with the same path inside the Roblox folder. \
                     For example, content\\sounds\\ouch.ogg changes the death sound. Delete the file and the original \
                     comes back the next time Roblox starts through Ruststrap.",
                )
                .color(MUTED),
            );
        });
    }

    // ------------------------------------------------------------ FastFlags
    fn flag_value(&self, name: &str) -> Option<String> {
        fast_flags::preset_flag(name).and_then(|f| fast_flags::get_value(&self.flags, f))
    }

    fn set_flag(&mut self, name: &str, value: Option<&str>) {
        if let Some(f) = fast_flags::preset_flag(name) {
            match value {
                Some(v) => {
                    self.flags.insert(f.to_string(), serde_json::Value::String(v.to_string()));
                }
                None => {
                    self.flags.remove(f);
                }
            }
        }
    }

    pub fn page_fastflags(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "FastFlags", "Hidden Roblox settings. Wrong values can stop Roblox from starting.");
        setting_row(ui, "Enable FastFlag manager", "Applies the flags below every time Roblox starts.", |ui| {
            toggle(ui, &mut self.settings.use_fast_flag_manager);
        });

        section_title(ui, "Presets");

        let msaa = self.flag_value("Rendering.MSAA");
        let mut msaa_sel = msaa.clone().unwrap_or_default();
        setting_row(ui, "Anti-aliasing (MSAA)", "Smoother edges, uses more GPU.", |ui| {
            egui::ComboBox::from_id_salt("msaa")
                .selected_text(match msaa_sel.as_str() {
                    "" => "Default".to_string(),
                    v => format!("{v}x"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut msaa_sel, String::new(), "Default");
                    for v in ["1", "2", "4"] {
                        ui.selectable_value(&mut msaa_sel, v.to_string(), format!("{v}x"));
                    }
                });
        });
        if msaa_sel != msaa.unwrap_or_default() {
            let v = if msaa_sel.is_empty() { None } else { Some(msaa_sel.as_str()) };
            self.set_flag("Rendering.MSAA", v);
        }

        let tex = self.flag_value("Rendering.TextureQuality.Level");
        let tex_on = self.flag_value("Rendering.TextureQuality.OverrideEnabled").is_some();
        let mut tex_sel = if tex_on { tex.clone().unwrap_or_default() } else { String::new() };
        let tex_name = |v: &str| match v {
            "0" => "Lowest",
            "1" => "Low",
            "2" => "Medium",
            "3" => "High",
            _ => "Default",
        };
        setting_row(ui, "Texture quality", "Lower it to save video memory.", |ui| {
            egui::ComboBox::from_id_salt("tex")
                .selected_text(tex_name(&tex_sel))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut tex_sel, String::new(), "Default");
                    for v in ["0", "1", "2", "3"] {
                        ui.selectable_value(&mut tex_sel, v.to_string(), tex_name(v));
                    }
                });
        });
        if tex_sel != if tex_on { tex.unwrap_or_default() } else { String::new() } {
            if tex_sel.is_empty() {
                self.set_flag("Rendering.TextureQuality.OverrideEnabled", None);
                self.set_flag("Rendering.TextureQuality.Level", None);
            } else {
                self.set_flag("Rendering.TextureQuality.OverrideEnabled", Some("True"));
                self.set_flag("Rendering.TextureQuality.Level", Some(&tex_sel));
            }
        }

        let mut scale_off = self.flag_value("Rendering.DisableScaling").as_deref() == Some("True");
        let before = scale_off;
        setting_row(ui, "Fix display scaling", "Stops Windows from blurring Roblox on high-DPI screens.", |ui| {
            toggle(ui, &mut scale_off);
        });
        if scale_off != before {
            self.set_flag("Rendering.DisableScaling", if scale_off { Some("True") } else { None });
        }

        section_title(ui, "Advanced");
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{} flags in the list", self.flags.len()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if button(ui, "Open editor").clicked() {
                        self.page = Page::FastFlagEditor;
                    }
                    if button(ui, "Reset all").clicked() {
                        self.modal = Some(super::settings::Modal::ResetFlags);
                    }
                });
            });
        });
    }

    pub fn page_flag_editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if button(ui, "Back").clicked() {
                self.page = Page::FastFlags;
            }
        });
        page_header(ui, "FastFlag editor", "Edit the raw list. Press Save at the bottom to keep your changes.");

        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.add(egui::TextEdit::singleline(&mut self.flag_filter).desired_width(ui.available_width()));
            });
        });

        card(ui, |ui| {
            ui.label(RichText::new("Add a flag").strong().color(TEXT));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.new_flag_name).hint_text("Name").desired_width(280.0));
                ui.add(egui::TextEdit::singleline(&mut self.new_flag_value).hint_text("Value").desired_width(160.0));
                if primary_button(ui, "Add").clicked() && !self.new_flag_name.trim().is_empty() {
                    self.flags.insert(
                        self.new_flag_name.trim().to_string(),
                        serde_json::Value::String(self.new_flag_value.trim().to_string()),
                    );
                    self.new_flag_name.clear();
                    self.new_flag_value.clear();
                }
            });
        });
        ui.add_space(6.0);

        let filter = self.flag_filter.to_lowercase();
        let keys: Vec<String> = self
            .flags
            .keys()
            .filter(|k| filter.is_empty() || k.to_lowercase().contains(&filter))
            .cloned()
            .collect();
        if keys.is_empty() {
            ui.label(RichText::new("No flags.").color(MUTED));
        }
        let mut remove: Option<String> = None;
        for k in keys {
            ui.horizontal(|ui| {
                ui.add_sized([360.0, 28.0], egui::Label::new(RichText::new(&k).monospace().color(TEXT)).truncate());
                let mut v = fast_flags::get_value(&self.flags, &k).unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut v).desired_width(160.0)).changed() {
                    self.flags.insert(k.clone(), serde_json::Value::String(v));
                }
                if button(ui, "Delete").clicked() {
                    remove = Some(k.clone());
                }
            });
        }
        if let Some(k) = remove {
            self.flags.remove(&k);
        }
    }

    // ------------------------------------------------------------ GPU
    pub fn page_gpu(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "GPU", "Pick which graphics card Roblox uses. Useful on laptops with two GPUs.");

        section_title(ui, "Detected");
        if self.adapters.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("No GPU was found in the registry.").color(MUTED));
            });
        }
        for a in &self.adapters {
            let kind = match a.kind {
                gpu::Kind::Discrete => "discrete",
                gpu::Kind::Integrated => "integrated",
                gpu::Kind::Other => "unknown type",
            };
            let vram = a.vram_mb.map(|m| format!("{m} MB")).unwrap_or_else(|| "VRAM unknown".into());
            card(ui, |ui| {
                ui.label(RichText::new(&a.name).strong().color(TEXT));
                ui.label(RichText::new(format!("{}  ·  {kind}  ·  {vram}", a.vendor)).small().color(MUTED));
            });
        }

        section_title(ui, "Roblox should use");
        let current = gpu::Pref::parse(&self.settings.gpu_preference).unwrap_or(gpu::Pref::Default);
        let mut chosen = current;
        for (pref, title, desc) in [
            (gpu::Pref::Default, "Let Windows decide", "No change. Windows picks the GPU."),
            (gpu::Pref::PowerSaving, "Power saving GPU", "Usually the integrated GPU. Longer battery life."),
            (gpu::Pref::High, "High performance GPU", "Usually the discrete GPU (NVIDIA / AMD). Best for playing."),
        ] {
            setting_row(ui, title, desc, |ui| {
                if ui.radio(chosen == pref, "").clicked() {
                    chosen = pref;
                }
            });
        }
        if chosen != current {
            self.settings.gpu_preference = chosen.as_str().to_string();
        }
        ui.label(
            RichText::new("Windows keeps this per Roblox version. Ruststrap writes it again after every update.")
                .small()
                .color(MUTED),
        );
    }

    // ------------------------------------------------------------ Ruststrap
    pub fn page_ruststrap(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Ruststrap", "Install, repair and folders.");

        let installed = self.installed;
        setting_row(
            ui,
            if installed { "Repair installation" } else { "Install Ruststrap" },
            "Copies Ruststrap to your user folder and makes roblox:// links open through it.",
            |ui| {
                if primary_button(ui, if installed { "Repair" } else { "Install" }).clicked() {
                    let code = installer::install(true);
                    if code == crate::app::error_code::SUCCESS {
                        self.installed = true;
                        self.say("Ruststrap installed", true);
                    } else {
                        self.say("Install failed, see the log", false);
                    }
                }
            },
        );

        let base = paths::get().base.clone();
        let logs = paths::get().logs.clone();
        setting_row(ui, "Data folder", &base.to_string_lossy(), |ui| {
            if button(ui, "Open").clicked() {
                open_folder(&base);
            }
        });
        setting_row(ui, "Logs", &logs.to_string_lossy(), |ui| {
            if button(ui, "Open").clicked() {
                open_folder(&logs);
            }
        });

        section_title(ui, "Danger zone");
        setting_row(ui, "Uninstall", "Removes Ruststrap and the Roblox it installed.", |ui| {
            if danger_button(ui, "Uninstall").clicked() {
                self.modal = Some(super::settings::Modal::Uninstall);
            }
        });
    }

    // ------------------------------------------------------------ About
    pub fn page_about(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "About", "");
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(88.0)));
                ui.vertical(|ui| {
                    ui.label(RichText::new(crate::app::PROJECT_NAME).size(24.0).strong().color(TEXT));
                    ui.label(RichText::new(format!("Version {}", crate::app::VERSION)).color(MUTED));
                    ui.add_space(4.0);
                    ui.hyperlink_to("Source code on GitHub", crate::app::PROJECT_DOWNLOAD_LINK);
                    ui.hyperlink_to("Report a problem", crate::app::PROJECT_SUPPORT_LINK);
                });
            });
        });
        card(ui, |ui| {
            ui.label(RichText::new("Credits").strong().color(TEXT));
            ui.label(
                RichText::new(
                    "Ruststrap is a Rust port of Bloxstrap. The launch, update and mod pipeline comes from Bloxstrap by \
                     pizzaboxer and contributors, used under the MIT license.",
                )
                .color(MUTED),
            );
            ui.hyperlink_to("Bloxstrap on GitHub", "https://github.com/bloxstraplabs/bloxstrap");
        });
    }
}
