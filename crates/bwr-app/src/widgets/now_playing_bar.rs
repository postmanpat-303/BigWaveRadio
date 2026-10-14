//! The bottom transport bar: station, status, controls, volume, save.

use eframe::egui::{self, Ui};

use crate::app::BigWaveApp;

pub fn bar(ui: &mut Ui, app: &mut BigWaveApp) {
    // Separator line across the top of the bar.
    let top = ui.max_rect().top();
    ui.painter().hline(
        ui.max_rect().left()..=ui.max_rect().right(),
        top,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );

    ui.horizontal(|ui| {
        ui.add_space(6.0);
        // Station + status.
        ui.vertical(|ui| {
            ui.add_space(4.0);
            let name = app
                .current
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "Pick a station to start listening".to_string());
            ui.label(egui::RichText::new(name).strong().size(15.0));
            let status: String = app.status.chars().take(90).collect();
            ui.label(egui::RichText::new(status).size(12.0).weak());
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);

            // Save current station.
            let saved = app
                .current
                .as_ref()
                .is_some_and(|c| app.library.contains(&c.uuid));
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(if saved { "★" } else { "☆" }).size(18.0))
                        .selected(saved)
                        .min_size(egui::vec2(44.0, 38.0)),
                )
                .on_hover_text("Save to My Library")
                .clicked()
            {
                if let Some(station) = app.current.clone() {
                    app.library.toggle(&station);
                }
            }

            ui.add_space(10.0);

            // Volume.
            let mut vol = app.player.volume();
            if ui
                .add_sized(
                    [130.0, 0.0],
                    egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false),
                )
                .changed()
            {
                app.player.set_volume(vol);
                app.settings.volume = vol;
            }

            ui.add_space(12.0);

            // Transport. NOTE: this whole group lives in a right_to_left
            // layout, so widgets must be added in reverse visual order.
            // Visual result, left to right: previous, play/pause, stop, next.
            let has_station = app.current.is_some();
            let btn = |label: &str| {
                egui::Button::new(egui::RichText::new(label).size(17.0))
                    .min_size(egui::vec2(48.0, 38.0))
            };
            if ui
                .add_enabled(app.can_go_next(), btn("⏭"))
                .on_hover_text("Next station")
                .clicked()
            {
                app.next_station();
            }
            ui.add_space(6.0);
            if ui
                .add_enabled(has_station, btn("⏹"))
                .on_hover_text("Stop")
                .clicked()
            {
                app.stop();
            }
            ui.add_space(6.0);
            let play_label = if app.is_playing { "⏸" } else { "▶" };
            if ui
                .add_enabled(has_station, btn(play_label))
                .on_hover_text("Play / pause")
                .clicked()
            {
                app.toggle_play_pause();
            }
            ui.add_space(6.0);
            if ui
                .add_enabled(app.can_go_previous(), btn("⏮"))
                .on_hover_text("Previous station")
                .clicked()
            {
                app.previous_station();
            }
        });
    });
}
