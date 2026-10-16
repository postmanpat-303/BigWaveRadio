//! Settings page: theme, highlight colour, bitrate filter and behaviour.

use bwr_core::settings::{Accent, Theme, ACCENTS};
use eframe::egui::{self, Color32, CornerRadius, Stroke, Ui};

use crate::app::BigWaveApp;
use crate::theme;

/// Card styled like the transport buttons: rounded corners, base fill and a
/// thin accent outline.
fn card<R>(
    ui: &mut Ui,
    dark: bool,
    accent: Color32,
    title: &str,
    body: impl FnOnce(&mut Ui) -> R,
) -> R {
    let fill = if dark {
        Color32::from_gray(40)
    } else {
        Color32::from_gray(250)
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(12))
        .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(title).heading().size(16.0));
            ui.add_space(6.0);
            body(ui)
        })
        .inner
}

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("Settings");
    ui.add_space(8.0);
    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    let accent = theme::accent_color(app.settings.accent);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // --- Appearance ------------------------------------------------------
        card(ui, dark, accent, "Theme", |ui| {
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
        card(ui, dark, accent, "Stream quality", |ui| {
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

        // --- Station cards -----------------------------------------------------
        card(ui, dark, accent, "Station cards", |ui| {
            ui.horizontal(|ui| {
                let mut comfy = app.discovery.comfortable_cards;
                let a = ui.radio_value(&mut comfy, false, "Compact");
                let b = ui.radio_value(&mut comfy, true, "Comfortable");
                if a.changed() || b.changed() || comfy != app.discovery.comfortable_cards {
                    app.discovery.comfortable_cards = comfy;
                    app.discovery.save();
                }
            });
            ui.label(
                egui::RichText::new("Compact fits about twice as many stations on screen.")
                    .small()
                    .weak(),
            );
        });

        ui.add_space(10.0);

        // --- Behaviour ---------------------------------------------------------
        card(ui, dark, accent, "Behaviour", |ui| {
            if ui
                .checkbox(&mut app.settings.minimize_to_tray, "Keep playing in the tray when I close the window")
                .changed()
            {
                app.persist_settings();
            }
            ui.label(
                egui::RichText::new(
                    "When on, closing hides BigWave Radio to the tray — playback continues, and \
                     a notification tells you where it went. Windows 11 puts the wave icon in the \
                     hidden ^ list beside the clock until you drag it out once; Ctrl+Alt+B also \
                     brings the window back. Minimizing always keeps the taskbar button.",
                )
                .small()
                .weak(),
            );
        });

        ui.add_space(10.0);

        // --- Idle Visions -------------------------------------------------------
        card(ui, dark, accent, "Idle Visions", |ui| {
            if ui
                .checkbox(
                    &mut app.settings.idle_visions,
                    "Start the Visions visualiser when the app is left untouched",
                )
                .changed()
            {
                app.persist_settings();
            }
            ui.horizontal(|ui| {
                ui.label("After");
                egui::ComboBox::from_id_salt("idle-minutes")
                    .selected_text(format!("{} minutes", app.settings.idle_minutes))
                    .show_ui(ui, |ui| {
                        for m in [1u32, 2, 5, 10, 15, 30] {
                            if ui
                                .selectable_label(app.settings.idle_minutes == m, format!("{m} minutes"))
                                .clicked()
                            {
                                app.settings.idle_minutes = m;
                                app.persist_settings();
                            }
                        }
                    });
            });
            ui.label(
                egui::RichText::new(
                    "Any mouse move or keypress closes the visualiser again. Only applies to \
                     Visions sessions started by the timer — ones you start yourself keep running.",
                )
                .small()
                .weak(),
            );
        });
    });

    // keep import used even if tweaked later
    let _ = theme::accent_color;
}
