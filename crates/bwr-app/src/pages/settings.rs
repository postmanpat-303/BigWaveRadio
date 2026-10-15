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
            ui.label(egui::RichText::new("Theme").heading().size(16.0));
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
            ui.label(egui::RichText::new("Highlight colour").heading().size(16.0));
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
                        egui::vec2(30.0, 30.0),
                        egui::Sense::click(),
                    );
                    let painter = ui.painter_at(rect.expand(3.0));
                    // Circle strokes are centred on the edge, so give the painter
                    // head-room (expand above) and keep the stroke on the 12px
                    // radius — stroking a larger radius used to eat 1px of fill.
                    painter.circle(
                        rect.center(),
                        12.0,
                        Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
                        if selected {
                            egui::Stroke::new(2.0_f32, ui.visuals().text_color())
                        } else {
                            egui::Stroke::new(1.0_f32, ui.visuals().weak_text_color())
                        },
                    );
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
            ui.label(egui::RichText::new("Stream quality").heading().size(16.0));
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
                ui.add_space(16.0);
                ui.label("Format:");
                egui::ComboBox::from_id_salt("codec-filter")
                    .selected_text(if app.settings.codec.is_empty() {
                        "Any format".to_string()
                    } else {
                        app.settings.codec.to_uppercase()
                    })
                    .show_ui(ui, |ui| {
                        for c in ["", "mp3", "aac", "ogg", "flac", "opus"] {
                            let label = if c.is_empty() {
                                "Any format".to_string()
                            } else {
                                c.to_uppercase()
                            };
                            if ui
                                .selectable_label(app.settings.codec == c, label)
                                .clicked()
                            {
                                app.settings.codec = c.to_string();
                                app.persist_settings();
                                app.reload_current_list();
                            }
                        }
                    });
            });
            ui.label(
                egui::RichText::new(
                    "Applies everywhere: Discover lists, search, tags, countries, Surprise me.",
                )
                .small()
                .weak(),
            );
        });

        ui.add_space(10.0);

        // --- Behaviour ---------------------------------------------------------
        ui.group(|ui| {
            ui.label(egui::RichText::new("Behaviour").heading().size(16.0));
            if ui
                .checkbox(&mut app.settings.minimize_to_tray, "Keep playing in the tray when I close the window")
                .changed()
            {
                app.persist_settings();
            }
            ui.label(
                egui::RichText::new(
                    "When on, closing hides BigWave Radio to the wave icon near the clock — \
                     playback continues. Minimizing keeps the taskbar button with quick controls. \
                     Right-click the tray icon, press Ctrl+Alt+B, or choose Quit there to exit fully.",
                )
                .small()
                .weak(),
            );
        });

        ui.add_space(10.0);

        // --- About -------------------------------------------------------------
        ui.group(|ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let logo = app.about_logo.clone();
                let logo_w = logo.size()[0] as f32;
                let logo_h = logo.size()[1] as f32;
                let w = 120.0_f32.min(ui.available_width());
                let h = w * logo_h / logo_w;
                ui.add(egui::Image::new((logo.id(), egui::vec2(w, h))).corner_radius(8));
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("About BigWave Radio").heading().size(20.0));
                    ui.add_space(6.0);
                    ui.label(
                        "BigWave Radio streams live stations from the community-powered \
                         radio-browser.info directory. Your library and these settings are \
                         stored locally on this PC.",
                    );
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("Created by Derek Heasley")
                            .strong()
                            .size(15.0),
                    );
                    ui.add_space(2.0);
                    ui.label("Inspired by ShortWave on Linux, I wanted a nice clean version for Windows.");
                    ui.add_space(10.0);
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
            });
            ui.add_space(6.0);
        });
    });

    // keep import used even if tweaked later
    let _ = theme::accent_color;
}
