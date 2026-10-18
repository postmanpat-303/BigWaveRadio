//! About page: logo, version, credits, donation links and the YouTube channel.

use eframe::egui::{self, CornerRadius, Stroke, Ui};

use crate::app::BigWaveApp;
use crate::pages::plugins::Btn;

/// Main donation link (Buy Me a Coffee).
pub const DONATE_URL: &str = "https://buymeacoffee.com/postmanpat";
/// Secondary donation link (PayPal.me).
pub const PAYPAL_URL: &str = "https://paypal.me/derekh100";
/// Del's YouTube channel (demos and promos).
pub const YOUTUBE_URL: &str = "https://www.youtube.com/@whatgoesaround1551";
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
    ui.heading(t!("about.title"));
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
                    ui.add_space(6.0);
                    let checking = app.updater.checking();
                    let label = if checking { t!("about.checking") } else { t!("about.check") };
                    if ui
                        .add_enabled_ui(!checking, |ui| {
                            crate::pages::plugins::action_button(ui, "🔄", label, Btn::Secondary)
                        })
                        .inner
                        .clicked()
                    {
                        app.updater.start(ui.ctx(), true);
                    }
                    ui.add_space(8.0);
                    ui.label(t!("about.blurb"));
                    ui.add_space(14.0);
                    ui.label(
                        egui::RichText::new(t!("about.created_by", name = "Del (postmanpat)"))
                            .strong()
                            .size(16.0),
                    );
                    ui.add_space(2.0);
                    ui.label(t!("about.inspired"));
                    ui.add_space(16.0);
                    ui.label(t!("about.coffee_intro"));
                    ui.add_space(8.0);
                    if crate::pages::plugins::action_button(ui, "☕", t!("about.coffee"), Btn::Large)
                        .on_hover_text(DONATE_URL)
                        .clicked()
                    {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(DONATE_URL));
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        // Centre the small "or PayPal" line under the button.
                        let w = ui.fonts(|f| {
                            f.layout_no_wrap(
                                format!("{} paypal.me/derekh100", t!("about.or_paypal")),
                                egui::FontId::proportional(12.5),
                                egui::Color32::WHITE,
                            )
                            .size()
                            .x
                        }) + ui.spacing().item_spacing.x;
                        ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                        ui.label(egui::RichText::new(t!("about.or_paypal")).size(12.5).weak());
                        ui.hyperlink_to(
                            egui::RichText::new("paypal.me/derekh100").size(12.5),
                            PAYPAL_URL,
                        );
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        // Centre the "Demos and promos" line like the PayPal one.
                        let w = ui.fonts(|f| {
                            f.layout_no_wrap(
                                format!("{} ▶ YouTube", t!("about.demos")),
                                egui::FontId::proportional(12.5),
                                egui::Color32::WHITE,
                            )
                            .size()
                            .x
                        }) + ui.spacing().item_spacing.x;
                        ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                        ui.label(egui::RichText::new(t!("about.demos")).size(12.5).weak());
                        ui.hyperlink_to(
                            egui::RichText::new("▶ YouTube")
                                .size(12.5)
                                .color(egui::Color32::from_rgb(0xff, 0x40, 0x40)),
                            YOUTUBE_URL,
                        )
                        .on_hover_text(YOUTUBE_URL);
                    });
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(t!("about.thanks")).weak());
                });
            },
        );
    });
}
