//! Search page: live-as-you-type station search, with tag chips when idle.

use eframe::egui::Ui;

use crate::app::{BigWaveApp, NetJob};
use crate::widgets::station_card::station_grid;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("Search");

    let mut changed = false;
    ui.horizontal(|ui| {
        let box_width = (ui.available_width() - 60.0).clamp(240.0, 480.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.query)
                .hint_text("Search stations by name…")
                .desired_width(box_width),
        );
        changed = response.changed();
        if app.loading && app.search_active {
            ui.spinner();
        }
        if !app.query.is_empty()
            && ui.button("✕").on_hover_text("Clear").clicked()
        {
            app.query.clear();
            app.search_active = false;
            app.stations.clear();
            changed = false;
        }
        response.request_focus();
    });
    ui.add_space(8.0);

    if changed && !app.query.trim().is_empty() {
        app.last_keystroke = Some(std::time::Instant::now());
    }

    if app.search_active {
        station_grid(ui, app);
    } else {
        // Idle state: popular tag chips as quick picks.
        ui.label(egui::RichText::new("Pick a vibe, or start typing above.").small().weak());
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for tag in app.tags.clone().iter().take(40) {
                if ui
                    .button(egui::RichText::new(format!("#{}", tag.name)).small())
                    .clicked()
                {
                    app.query = tag.name.clone();
                    app.last_keystroke = Some(std::time::Instant::now());
                }
            }
        });
    }

    let _ = NetJob::Search; // silence unused warning if features are trimmed
}
