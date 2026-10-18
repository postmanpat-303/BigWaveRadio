//! My Library page: favourites and Recently played. (Ratings live in the
//! Your Stars view in the left rail.)

use eframe::egui::Ui;

use crate::app::{BigWaveApp, LibraryTab};
use crate::pages::discover::tab_button;
use crate::widgets::station_card::station_grid;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading(t!("library.title"));
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        for (label, tab) in [
            (t!("library.favourites"), LibraryTab::Favourites),
            (t!("library.recent"), LibraryTab::Recent),
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
        LibraryTab::Recent => recent(ui, app),
    }
}

fn favourites(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.label(
        egui::RichText::new(t!("library.favourites_hint"))
            .small()
            .weak(),
    );
    ui.add_space(8.0);

    if app.library.stations.is_empty() {
        ui.add_space(30.0);
        ui.label(egui::RichText::new(t!("library.empty")).weak());
    } else {
        // Show library contents (grid reads app.stations — swap them in).
        let remembered = std::mem::take(&mut app.stations);
        app.stations = app.library.stations.clone();
        station_grid(ui, app);
        let _ = remembered;
    }
}

/// Recently played stations, newest first.
fn recent(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t!("library.recent_hint")).small().weak());
        if !app.discovery.recent.is_empty() && ui.small_button(t!("common.clear")).clicked() {
            app.discovery.recent.clear();
            app.discovery.save();
        }
    });
    ui.add_space(8.0);
    if app.discovery.recent.is_empty() {
        ui.add_space(30.0);
        ui.label(egui::RichText::new(t!("library.recent_empty")).weak());
        return;
    }
    app.stations = app.discovery.recent.clone();
    station_grid(ui, app);
}
