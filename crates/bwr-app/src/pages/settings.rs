//! Settings page: theme, highlight colour, bitrate filter, About.

use bwr_core::settings::{Accent, Theme, ACCENTS};
use eframe::egui::{self, Color32, Ui};

use crate::app::BigWaveApp;
use crate::theme;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("Settings");
    ui.add_space(8.0);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // --- Appearance ------------------------------------------------------
        ui.group(|ui| {
            ui.label(egui::RichText::new("Theme").strong());
            ui.horizontal(|ui| {
                for (label, value) in [
                    ("Match system", Theme::System),
                    ("Light", Theme::Light),
                    ("Dark", Theme::Dark),
                ] {
                    if ui.selectable_label(app.settings.theme == value, label).clicked() {
                        app.settings.theme = value;
                        app.persist_settings();
                    }
                }
            });

            ui.add_space(10.0);
            ui.label(egui::RichText::new("Highlight colour").strong());
            ui.horizontal(|ui| {
                // "Match system" swatch.
                let system_selected = app.settings.accent == Accent::System;
                if ui
                    .selectable_label(system_selected, "Match system")
                    .clicked()
                {
                    app.settings.accent = Accent::System;
                    app.persist_settings();
                }
                for (key, _name, rgb) in ACCENTS {
                    let accent = match *key {
                        "ocean" => Accent::Ocean,
                        "teal" => Accent::Teal,
                        "forest" => Accent::Forest,
                        "gold" => Accent::Gold,
                        "ember" => Accent::Ember,
                        "ruby" => Accent::Ruby,
                        "pink" => Accent::Pink,
                        _ => Accent::Violet,
                    };
                    let selected = app.settings.accent == accent;
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(28.0, 28.0),
                        egui::Sense::click(),
                    );
                    let painter = ui.painter_at(rect);
                    painter.circle_filled(
                        rect.center(),
                        12.0,
                        Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
                    );
                    if selected {
                        painter.circle_stroke(rect.center(), 13.0, egui::Stroke::new(2.0_f32, ui.visuals().text_color()));
                    }
                    if response.clicked() {
                        app.settings.accent = accent;
                        app.persist_settings();
                    }
                }
            });
        });

        ui.add_space(10.0);

        // --- Stream quality ---------------------------------------------------
        ui.group(|ui| {
            ui.label(egui::RichText::new("Stream quality").strong());
            ui.horizontal(|ui| {
                ui.label("Minimum bitrate:");
                egui::ComboBox::from_id_salt("min-bitrate")
                    .selected_text(match app.settings.min_bitrate {
                        0 => "Any bitrate".to_string(),
                        b => format!("{b} kbps or more"),
                    })
                    .show_ui(ui, |ui| {
                        for b in [0u32, 64, 128, 192, 256, 320] {
                            let label = if b == 0 {
                                "Any bitrate".to_string()
                            } else {
                                format!("{b} kbps or more")
                            };
                            if ui
                                .selectable_label(app.settings.min_bitrate == b, label)
                                .clicked()
                            {
                                app.settings.min_bitrate = b;
                                app.persist_settings();
                                app.reload_current_list();
                            }
                        }
                    });
            });
        });

        ui.add_space(10.0);

        // --- About -------------------------------------------------------------
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(egui::include_image!("../../../../assets/bigwave-logo.png"))
                        .max_width(90.0),
                );
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("About BigWave Radio").strong());
                    ui.label(
                        egui::RichText::new(
                            "BigWave Radio streams live stations from the community-powered \
                             radio-browser.info directory. Your library and these settings are \
                             stored locally on this PC.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Created by Derek Heasley").strong());
                    ui.label(
                        egui::RichText::new(
                            "Inspired by ShortWave on Linux, I wanted a nice clean version for Windows.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("If you find this app useful, you can donate to").small().weak());
                        ui.add(egui::Hyperlink::from_label_and_url(
                            "paypal.me/derekh100",
                            "https://paypal.me/derekh100",
                        ));
                    });
                    ui.label(egui::RichText::new("Thank you. :)").small().weak());
                });
            });
        });
    });

    // keep import used even if tweaked later
    let _ = theme::accent_color;
}
