//! Mood chip results and the Your Stars view.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Sense, Ui, Vec2};

use crate::app::{BigWaveApp, NetJob};
use crate::theme;
use crate::widgets::station_card::station_grid;

pub fn mood_page(ui: &mut Ui, app: &mut BigWaveApp, idx: usize) {
    let Some(mood) = app.moods.moods.get(idx).cloned() else {
        ui.label(t!("moods.missing"));
        return;
    };
    ui.horizontal(|ui| {
        ui.heading(format!("{} {}", mood.icon, crate::widgets::mood_rail::mood_label(&mood)));
        ui.add_space(12.0);
        if ui
            .button(egui::RichText::new(format!("🎲  {}", t!("moods.surprise", mood = crate::widgets::mood_rail::mood_label_raw(&mood)))).size(14.0))
            .on_hover_text(t!("moods.surprise_tip"))
            .clicked()
        {
            app.surprise();
        }
        if app.loading {
            ui.spinner();
        }
    });
    ui.label(
        egui::RichText::new(format!("#{}", mood.tags.join("  #")))
            .small()
            .weak(),
    )
    .on_hover_text(t!("moods.mapping_tip", path = bwr_core::MoodConfig::path().display()));
    talk_note(ui, app);
    ui.add_space(8.0);
    station_grid(ui, app);
}

/// "N talk stations hidden · Show" line under a result header.
pub fn talk_note(ui: &mut Ui, app: &mut BigWaveApp) {
    let hidden = app.hidden_talk_count();
    if hidden == 0 {
        return;
    }
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(tn!("moods.talk_hidden", hidden))
            .small()
            .weak(),
        );
        if ui.small_button(t!("rail.show_talk")).clicked() {
            app.discovery.show_talk = true;
            app.discovery.save();
        }
    });
}

/// Your Stars: every rated station, independent of the current category.
/// Each row on the left is a star level; "All rated" lists everything,
/// best stars first.
pub fn stars_page(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading(format!("★ {}", t!("stars.title")));
    ui.label(
        egui::RichText::new(t!("stars.subtitle"))
            .small()
            .weak(),
    );
    ui.add_space(10.0);

    // Fetch full records for rated uuids we don't have cached yet (deduped).
    let missing: Vec<String> = app
        .ratings
        .missing_records()
        .into_iter()
        .filter(|u| app.rated_inflight.insert(u.clone()))
        .collect();
    if !missing.is_empty() {
        app.loading = true;
        let _ = app.job_tx.send(NetJob::Rated(missing));
    }

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(150.0);
            let total: usize = (1..=5).map(|r| app.ratings.count(r)).sum();
            if level_row(ui, app, 0, total) {
                app.stars_level = 0;
            }
            for r in (1..=5u8).rev() {
                let n = app.ratings.count(r);
                if level_row(ui, app, r, n) {
                    app.stars_level = if app.stars_level == r { 0 } else { r };
                }
            }
        });
        ui.add_space(12.0);
        ui.vertical(|ui| {
            if app.ratings.is_empty() {
                ui.add_space(30.0);
                ui.label(
                    egui::RichText::new(t!("stars.empty"))
                        .weak(),
                );
                return;
            }
            let level = app.stars_level;
            let list: Vec<_> = app
                .ratings
                .rated_stations()
                .into_iter()
                .filter(|(r, _)| level == 0 || *r == level)
                .map(|(_, s)| s)
                .collect();
            if list.is_empty() {
                ui.add_space(30.0);
                ui.label(egui::RichText::new(if app.loading {
                    t!("stars.fetching")
                } else {
                    t!("stars.level_empty")
                }).weak());
                return;
            }
            app.stations = list;
            station_grid(ui, app);
        });
    });
}

fn level_row(ui: &mut Ui, app: &BigWaveApp, level: u8, count: usize) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(150.0, 30.0), Sense::click());
    let accent = theme::accent_color(app.settings.accent);
    let selected = app.stars_level == level;
    let weak = ui.visuals().weak_text_color();
    let strong = ui.visuals().strong_text_color();
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::same(6), accent.gamma_multiply(0.22));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), accent.gamma_multiply(0.10));
    }
    let y = rect.center().y;
    if level == 0 {
        ui.painter().text(Pos2::new(rect.left() + 10.0, y), Align2::LEFT_CENTER, t!("stars.all"), FontId::proportional(14.0), strong);
    } else {
        for s in 0..5u8 {
            let filled = s < level;
            ui.painter().text(
                Pos2::new(rect.left() + 10.0 + s as f32 * 16.0, y),
                Align2::LEFT_CENTER,
                if filled { "★" } else { "☆" },
                FontId::proportional(15.0),
                if filled { Color32::from_rgb(0xff, 0xc8, 0x2e) } else { weak.gamma_multiply(0.6) },
            );
        }
    }
    ui.painter().text(Pos2::new(rect.right() - 10.0, y), Align2::RIGHT_CENTER, count.to_string(), FontId::proportional(12.0), weak);
    resp.clicked()
}
