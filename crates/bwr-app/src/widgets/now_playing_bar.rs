//! The bottom transport bar: station, status, controls, volume, save.

use eframe::egui::{self, Ui};

use crate::app::BigWaveApp;

pub fn bar(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.horizontal(|ui| {
        // Station + status.
        ui.vertical(|ui| {
            let name = app
                .current
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "Pick a station to start listening".to_string());
            ui.label(egui::RichText::new(name).strong());
            ui.label(egui::RichText::new(app.status.clone()).small().weak());
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Save current station.
            let saved = app
                .current
                .as_ref()
                .is_some_and(|c| app.library.contains(&c.uuid));
            if ui
                .add(egui::Button::new(if saved { "★" } else { "☆" }).selected(saved))
                .on_hover_text("Save to My Library")
                .clicked()
            {
                if let Some(station) = app.current.clone() {
                    app.library.toggle(&station);
                }
            }

            // Volume.
            ui.add_space(8.0);
            let mut vol = app.player.volume();
            if ui
                .add_sized(
                    [120.0, 0.0],
                    egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false),
                )
                .changed()
            {
                app.player.set_volume(vol);
                app.settings.volume = vol;
            }

            // Transport. NOTE: this whole group lives in a right_to_left
            // layout, so widgets must be added in reverse visual order.
            ui.add_space(8.0);
            let has_station = app.current.is_some();
            if ui
                .add_enabled(app.can_go_next(), egui::Button::new("⏭").min_size(egui::vec2(38.0, 0.0)))
                .on_hover_text("Next station")
                .clicked()
            {
                app.next_station();
            }
            if ui
                .add_enabled(has_station, egui::Button::new("⏹").min_size(egui::vec2(38.0, 0.0)))
                .on_hover_text("Stop")
                .clicked()
            {
                app.stop();
            }
            let play_label = if app.is_playing { "⏸" } else { "▶" };
            if ui
                .add_enabled(has_station, egui::Button::new(play_label).min_size(egui::vec2(38.0, 0.0)))
                .on_hover_text("Play / pause")
                .clicked()
            {
                app.toggle_play_pause();
            }
            if ui
                .add_enabled(app.can_go_previous(), egui::Button::new("⏮").min_size(egui::vec2(38.0, 0.0)))
                .on_hover_text("Previous station")
                .clicked()
            {
                app.previous_station();
            }
        });
    });
}
