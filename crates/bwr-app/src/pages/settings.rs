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
    ui.heading(t!("settings.title"));
    ui.add_space(8.0);
    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    let accent = theme::accent_color(app.settings.accent);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // --- Language --------------------------------------------------------
        card(ui, dark, accent, &format!("🌐  {}", t!("settings.language")), |ui| {
            use crate::i18n::{Choice, Lang};
            let current = Choice::parse(&app.ui_prefs.language);
            let system = crate::i18n::system_lang();
            let name = |c: Choice| match c {
                Choice::System => t!("settings.language_system", lang = system.native_name()),
                Choice::Fixed(l) => crate::rtl::visual(l.native_name()).into_owned(),
            };
            let mut picked = None;
            egui::ComboBox::from_id_salt("ui-language")
                .selected_text(name(current))
                .width(240.0)
                .show_ui(ui, |ui| {
                    let choices = std::iter::once(Choice::System).chain(Lang::ALL.map(Choice::Fixed));
                    for c in choices {
                        if ui.selectable_label(current == c, name(c)).clicked() {
                            picked = Some(c);
                        }
                    }
                });
            if let Some(c) = picked.filter(|c| *c != current) {
                app.set_language(c);
            }
            ui.label(egui::RichText::new(t!("settings.language_hint")).small().weak());
        });

        ui.add_space(10.0);

        // --- Appearance ------------------------------------------------------
        card(ui, dark, accent, t!("settings.theme"), |ui| {
            ui.horizontal(|ui| {
                for (label, value) in [
                    (t!("settings.match_system"), Theme::System),
                    (t!("settings.light"), Theme::Light),
                    (t!("settings.dark"), Theme::Dark),
                ] {
                    if ui.selectable_label(app.settings.theme == value, label).clicked() {
                        app.settings.theme = value;
                        app.persist_settings();
                    }
                }
            });

            ui.add_space(10.0);
            ui.label(egui::RichText::new(t!("settings.highlight")).heading().size(16.0));
            ui.horizontal(|ui| {
                // "Match system" swatch.
                let system_selected = app.settings.accent == Accent::System;
                if ui
                    .selectable_label(system_selected, t!("settings.match_system"))
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
        card(ui, dark, accent, t!("settings.quality"), |ui| {
            ui.horizontal(|ui| {
                ui.label(t!("settings.min_bitrate"));
                egui::ComboBox::from_id_salt("min-bitrate")
                    .selected_text(match app.settings.min_bitrate {
                        0 => t!("filter.any_bitrate").to_string(),
                        b => t!("filter.kbps_or_more", kbps = b),
                    })
                    .show_ui(ui, |ui| {
                        for b in [0u32, 64, 128, 192, 256, 320] {
                            let label = if b == 0 {
                                t!("filter.any_bitrate").to_string()
                            } else {
                                t!("filter.kbps_or_more", kbps = b)
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
                ui.label(t!("settings.format"));
                egui::ComboBox::from_id_salt("codec-filter")
                    .selected_text(if app.settings.codec.is_empty() {
                        t!("filter.any_format").to_string()
                    } else {
                        app.settings.codec.to_uppercase()
                    })
                    .show_ui(ui, |ui| {
                        for c in ["", "mp3", "aac", "ogg", "flac", "opus"] {
                            let label = if c.is_empty() {
                                t!("filter.any_format").to_string()
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
                    t!("settings.quality_hint"),
                )
                .small()
                .weak(),
            );
        });

        ui.add_space(10.0);

        // --- Station cards -----------------------------------------------------
        card(ui, dark, accent, t!("settings.cards"), |ui| {
            ui.horizontal(|ui| {
                let mut comfy = app.discovery.comfortable_cards;
                let a = ui.radio_value(&mut comfy, false, t!("settings.compact"));
                let b = ui.radio_value(&mut comfy, true, t!("settings.comfortable"));
                if a.changed() || b.changed() || comfy != app.discovery.comfortable_cards {
                    app.discovery.comfortable_cards = comfy;
                    app.discovery.save();
                }
            });
            ui.label(
                egui::RichText::new(t!("settings.cards_hint"))
                    .small()
                    .weak(),
            );
        });

        ui.add_space(10.0);

        // --- Behaviour ---------------------------------------------------------
        card(ui, dark, accent, t!("settings.behaviour"), |ui| {
            if ui
                .checkbox(&mut app.updater.state.check_at_startup, t!("settings.check_updates"))
                .on_hover_text(t!("settings.check_updates_tip"))
                .changed()
            {
                app.updater.state.save();
            }
            // The tray (and Ctrl+Alt+B) exist on Windows only; elsewhere closing quits.
            #[cfg(windows)]
            {
                if ui
                    .checkbox(&mut app.settings.minimize_to_tray, t!("settings.tray"))
                    .changed()
                {
                    app.persist_settings();
                }
                ui.label(
                    egui::RichText::new(t!("settings.tray_hint"))
                    .small()
                    .weak(),
                );
            }
        });

        ui.add_space(10.0);

        // --- Idle Visions -------------------------------------------------------
        card(ui, dark, accent, t!("settings.idle"), |ui| {
            if ui
                .checkbox(
                    &mut app.settings.idle_visions,
                    t!("settings.idle_start"),
                )
                .changed()
            {
                app.persist_settings();
            }
            ui.horizontal(|ui| {
                ui.label(t!("settings.idle_after"));
                egui::ComboBox::from_id_salt("idle-minutes")
                    .selected_text(tn!("settings.minutes", app.settings.idle_minutes))
                    .show_ui(ui, |ui| {
                        for m in [1u32, 2, 5, 10, 15, 30] {
                            if ui
                                .selectable_label(app.settings.idle_minutes == m, tn!("settings.minutes", m))
                                .clicked()
                            {
                                app.settings.idle_minutes = m;
                                app.persist_settings();
                            }
                        }
                    });
            });
            ui.label(
                egui::RichText::new(t!("settings.idle_hint"))
                .small()
                .weak(),
            );
        });
    });

    // keep import used even if tweaked later
    let _ = theme::accent_color;
}
