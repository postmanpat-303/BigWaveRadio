//! Browse by tag or country, then the stations under the selection.

use eframe::egui::Ui;

use crate::app::{BigWaveApp, BrowseSel, NetJob, Page};
use crate::widgets::station_card::station_grid;

pub fn tags_page(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("Tags");
    ui.add_space(6.0);
    if app.browse_selection.is_none() {
        if app.tags.is_empty() && !app.loading {
            app.loading = true;
            let _ = app.job_tx.send(NetJob::Tags);
        }
        ui.horizontal_wrapped(|ui| {
            for tag in app.tags.clone().iter().take(100) {
                if ui
                    .button(format!("#{}  ({})", tag.name, tag.station_count))
                    .clicked()
                {
                    app.browse_selection = Some(BrowseSel::Tag(tag.name.clone()));
                    app.loading = true;
                    let _ = app.job_tx.send(NetJob::StationsFor(BrowseSel::Tag(tag.name.clone())));
                }
            }
        });
    } else {
        selection_header(ui, app);
        station_grid(ui, app);
    }
}

pub fn countries_page(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("Countries");
    ui.add_space(6.0);
    if app.browse_selection.is_none() {
        if app.countries.is_empty() && !app.loading {
            app.loading = true;
            let _ = app.job_tx.send(NetJob::Countries);
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for country in app.countries.clone().iter().take(200) {
                if ui
                    .button(format!("{}  ({})", country.name, country.station_count))
                    .clicked()
                {
                    app.browse_selection = Some(BrowseSel::Country(country.name.clone()));
                    app.loading = true;
                    let _ = app
                        .job_tx
                        .send(NetJob::StationsFor(BrowseSel::Country(country.name.clone())));
                }
            }
        });
    } else {
        selection_header(ui, app);
        station_grid(ui, app);
    }
}

fn selection_header(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.horizontal(|ui| {
        if ui.button("← Back").clicked() {
            app.browse_selection = None;
            app.stations.clear();
            app.loading = false;
        }
        let title = match app.browse_selection {
            Some(BrowseSel::Tag(ref t)) => format!("#{t}"),
            Some(BrowseSel::Country(ref c)) => c.clone(),
            None => String::new(),
        };
        ui.heading(title);
    });
    ui.add_space(6.0);
    let _ = Page::Tags; // keep import used if pages are trimmed
}
