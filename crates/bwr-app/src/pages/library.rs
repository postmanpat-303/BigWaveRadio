//! My Library page.

use eframe::egui::Ui;

use crate::app::BigWaveApp;
use crate::widgets::station_card::station_grid;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("My Library");
    ui.label(
        egui::RichText::new("Stations you starred for quick access — stored locally on this PC.")
            .small()
            .weak(),
    );
    ui.add_space(8.0);

    if app.library.stations.is_empty() {
        ui.add_space(30.0);
        ui.label(egui::RichText::new("No saved stations yet. Tap the ☆ on any station card to save it here.").weak());
    } else {
        // Show library contents (grid reads app.stations — swap them in).
        let remembered = std::mem::take(&mut app.stations);
        app.stations = app.library.stations.clone();
        station_grid(ui, app);
        let _ = remembered;
    }
}
