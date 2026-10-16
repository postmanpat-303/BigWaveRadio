//! My Library page: favourites and the Rated smart list.

use eframe::egui::Ui;

use crate::app::{BigWaveApp, LibraryTab, NetJob};
use crate::pages::discover::tab_button;
use crate::widgets::station_card::station_grid;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("My Library");
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        for (label, tab) in [
            ("Favourites", LibraryTab::Favourites),
            ("Rated", LibraryTab::Rated),
        ] {
            if tab_button(ui, app, label, app.library_tab == tab) && app.library_tab != tab {
                app.library_tab = tab;
                app.stations.clear();
            }
            ui.add_space(8.0);
        }
    });
    ui.add_space(10.0);

    match app.library_tab {
        LibraryTab::Favourites => favourites(ui, app),
        LibraryTab::Rated => rated(ui, app),
    }
}

fn favourites(ui: &mut Ui, app: &mut BigWaveApp) {
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

/// Every station the user rated, best stars first.
fn rated(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.label(
        egui::RichText::new("Every station you rated — best stars first.")
            .small()
            .weak(),
    );
    ui.add_space(8.0);

    if app.ratings.is_empty() {
        ui.add_space(30.0);
        ui.label(
            egui::RichText::new("Nothing rated yet. Play a station and set its stars in the info panel on the right.")
                .weak(),
        );
        return;
    }

    // Fetch full records for rated uuids we don't have yet (deduped).
    let missing: Vec<String> = app
        .ratings
        .missing_records()
        .into_iter()
        .filter(|u| app.rated_inflight.insert(u.clone()))
        .collect();
    if !missing.is_empty() {
        let _ = app.job_tx.send(NetJob::Rated(missing));
    }

    let rated = app.ratings.rated_stations();
    if rated.is_empty() {
        ui.add_space(30.0);
        ui.label(egui::RichText::new("Fetching your rated stations…").weak());
        return;
    }

    let remembered = std::mem::take(&mut app.stations);
    app.stations = rated.into_iter().map(|(_, s)| s).collect();
    station_grid(ui, app);
    let _ = remembered;
}
