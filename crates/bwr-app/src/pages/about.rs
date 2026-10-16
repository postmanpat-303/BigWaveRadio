//! About page: logo, version, credits and donation link.

use eframe::egui::{self, CornerRadius, Stroke, Ui};

use crate::app::BigWaveApp;
use crate::theme;

/// Card styled like the transport buttons: rounded corners, base fill and a
/// thin accent outline.
fn card<R>(
    ui: &mut Ui,
    dark: bool,
    accent: egui::Color32,
    body: impl FnOnce(&mut Ui) -> R,
) -> R {
    let fill = if dark {
        egui::Color32::from_gray(40)
    } else {
        egui::Color32::from_gray(250)
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(12))
        .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
        .inner_margin(egui::Margin::same(20))
        .show(ui, body)
        .inner
}

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("About");
    ui.add_space(8.0);
    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    let accent = theme::accent_color(app.settings.accent);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // Keep the card readable on wide windows: cap its width and centre it.
        let max_w = 620.0_f32.min(ui.available_width());
        ui.allocate_ui_with_layout(
            egui::vec2(max_w, ui.available_height()),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                card(ui, dark, accent, |ui| {
                    // Full logo in dark theme; the square icon in light, where the
                    // logo's silver "RADIO" text would wash out on the light card.
                    let logo = if dark {
                        app.about_logo.clone()
                    } else {
                        app.about_logo_icon.clone()
                    };
                    let logo_w = logo.size()[0] as f32;
                    let logo_h = logo.size()[1] as f32;
                    let w = (if dark { 190.0f32 } else { 160.0 }).min(ui.available_width());
                    let h = w * logo_h / logo_w;
                    ui.add(egui::Image::new((logo.id(), egui::vec2(w, h))).corner_radius(8));
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("BigWave Radio").heading().size(24.0));
                        ui.add_space(4.0);
                        // Version pill, matching the transport-button outline.
                        egui::Frame::new()
                            .fill(accent.gamma_multiply(0.15))
                            .corner_radius(CornerRadius::same(8))
                            .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
                            .inner_margin(egui::Margin::symmetric(8, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                        .size(12.0)
                                        .color(accent),
                                );
                            });
                    });
                    ui.add_space(8.0);
                    ui.label(
                        "BigWave Radio streams live stations from the community-powered \
                         radio-browser.info directory. Your library and settings are \
                         stored locally on this PC.",
                    );
                    ui.add_space(14.0);
                    ui.label(
                        egui::RichText::new("Created by Derek Heasley")
                            .strong()
                            .size(16.0),
                    );
                    ui.add_space(2.0);
                    ui.label("Inspired by ShortWave on Linux, I wanted a nice clean version for Windows.");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label("If you find this app useful, you can donate to");
                        ui.add(egui::Hyperlink::from_label_and_url(
                            "paypal.me/derekh100",
                            "https://paypal.me/derekh100",
                        ));
                    });
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Thank you. :)").weak());
                });
            },
        );
    });
}
