//! Discover page: Trending / Popular / Recently added.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Sense, Stroke, Ui, Vec2};

use crate::app::{BigWaveApp, DiscoverTab, NetJob};
use crate::theme;
use crate::widgets::station_card::station_grid;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("Discover");
    ui.label(
        egui::RichText::new("Live radio from the community-powered radio-browser.info directory")
            .small()
            .weak(),
    );
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        for (label, tab) in [
            ("Trending", DiscoverTab::Trending),
            ("Popular", DiscoverTab::Popular),
            ("Recently added", DiscoverTab::Recent),
        ] {
            if tab_button(ui, app, label, app.discover_tab == tab) && app.discover_tab != tab {
                app.discover_tab = tab;
                app.loading = true;
                let _ = app.job_tx.send(NetJob::Discover(tab));
            }
            ui.add_space(8.0);
        }
    });
    ui.add_space(10.0);

    station_grid(ui, app);
}

/// Outlined pill tab: accent fill when selected, outline always.
fn tab_button(ui: &mut Ui, app: &BigWaveApp, label: &str, selected: bool) -> bool {
    let accent = theme::accent_color(app.settings.accent);
    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    let font = FontId::proportional(15.0);
    let text_w = ui.fonts(|f| {
        f.layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    });
    let (rect, response) = ui.allocate_exact_size(Vec2::new(text_w + 28.0, 32.0), Sense::click());

    if response.hovered() && !selected {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(6),
            accent.gamma_multiply(0.12_f32),
        );
    }
    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        if selected { accent.gamma_multiply(0.85_f32) } else { Color32::TRANSPARENT },
        Stroke::new(1.0_f32, accent),
        egui::StrokeKind::Outside,
    );
    ui.painter().text(
        Pos2::new(rect.center().x, rect.center().y),
        Align2::CENTER_CENTER,
        label,
        font,
        if selected {
            Color32::WHITE
        } else if dark {
            Color32::from_gray(220)
        } else {
            Color32::from_gray(40)
        },
    );
    response.clicked()
}
