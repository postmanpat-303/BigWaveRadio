//! Discover page: Trending / Popular / Recently added.

use eframe::egui::Ui;

use crate::app::{BigWaveApp, DiscoverTab, NetJob};
use crate::widgets::station_card::station_grid;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("Discover");
    ui.label(
        egui::RichText::new("Live radio from the community-powered radio-browser.info directory")
            .small()
            .weak(),
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        for (label, tab) in [
            ("Trending", DiscoverTab::Trending),
            ("Popular", DiscoverTab::Popular),
            ("Recently added", DiscoverTab::Recent),
        ] {
            if ui.selectable_label(app.discover_tab == tab, label).clicked()
                && app.discover_tab != tab
            {
                app.discover_tab = tab;
                app.loading = true;
                let _ = app.job_tx.send(NetJob::Discover(tab));
            }
        }
    });
    ui.add_space(8.0);

    station_grid(ui, app);
}
