//! egui front end: the settings window and the launch (progress) window, in Rust-orange.

pub mod home;
pub mod launch;
pub mod pages;
pub mod settings;
pub mod setup;
pub mod theme;
pub mod widgets;

use eframe::egui;

const ICON_PNG: &[u8] = include_bytes!("../../assets/Ruststrap-256.png");

pub fn icon_data() -> egui::IconData {
    eframe::icon_data::from_png_bytes(ICON_PNG).unwrap_or_default()
}

pub fn icon_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let d = icon_data();
    let img = egui::ColorImage::from_rgba_unmultiplied([d.width as usize, d.height as usize], &d.rgba);
    ctx.load_texture("ruststrap-icon", img, egui::TextureOptions::LINEAR)
}

pub fn native_options(title: &str, size: [f32; 2], min: [f32; 2], resizable: bool, decorations: bool) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size)
            .with_min_inner_size(min)
            .with_resizable(resizable)
            .with_decorations(decorations)
            .with_icon(icon_data()),
        centered: true,
        ..Default::default()
    }
}
