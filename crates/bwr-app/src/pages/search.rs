//! Search page: live-as-you-type station search, with tag chips when idle.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Sense, Stroke, Ui, Vec2};

use crate::app::{BigWaveApp, NetJob};
use crate::theme;
use crate::widgets::station_card::station_grid;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("Search");
    ui.add_space(2.0);
    if let Some(label) = &app.similar_label {
        ui.label(egui::RichText::new(format!("✨ {label}")).size(13.0).weak());
        ui.add_space(4.0);
    }

    let mut changed = false;
    ui.horizontal(|ui| {
        let box_width = (ui.available_width() - 70.0).clamp(260.0, 520.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.query)
                .hint_text("Search stations by name…")
                .font(egui::FontId::proportional(16.0))
                .desired_width(box_width),
        );
        changed = response.changed();
        if app.loading && app.search_active {
            ui.spinner();
        }
        if !app.query.is_empty()
            && ui.button(egui::RichText::new("✕").size(16.0)).on_hover_text("Clear").clicked()
        {
            app.query.clear();
            app.search_active = false;
            app.stations.clear();
            changed = false;
        }
        response.request_focus();
    });
    ui.add_space(10.0);

    if changed && !app.query.trim().is_empty() {
        app.last_keystroke = Some(std::time::Instant::now());
    }

    if app.search_active {
        station_grid(ui, app);
    } else {
        // Idle state: recent searches first, then popular tag chips as quick picks.
        if !app.settings.search_history.is_empty() {
            ui.label(egui::RichText::new("Recent searches").size(13.0).weak());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for q in app.settings.search_history.clone() {
                    if chip(ui, app, &q) {
                        app.query = q.clone();
                        app.last_searched = q.clone();
                        app.last_keystroke = None;
                        app.loading = true;
                        let _ = app.job_tx.send(NetJob::Search(q));
                    }
                }
            });
            ui.add_space(12.0);
        }
        ui.label("Pick a vibe below, or start typing to search every station by name.");
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for tag in app.tags.clone().iter().take(48) {
                if chip(ui, app, &format!("#{}", tag.name)) {
                    app.query = tag.name.clone();
                    app.last_keystroke = Some(std::time::Instant::now());
                }
            }
        });
    }

    let _ = NetJob::Search; // silence unused warning if features are trimmed
}

/// Outlined pill chip, same family as the Discover tabs.
pub(crate) fn chip(ui: &mut Ui, app: &BigWaveApp, label: &str) -> bool {
    let accent = theme::accent_color(app.settings.accent);
    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    let font = FontId::proportional(14.0);
    let text_w = ui.fonts(|f| {
        f.layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    });
    let (rect, response) = ui.allocate_exact_size(Vec2::new(text_w + 22.0, 28.0), Sense::click());

    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(14), accent.gamma_multiply(0.12_f32));
    }
    ui.painter().rect(
        rect,
        CornerRadius::same(14),
        Color32::TRANSPARENT,
        Stroke::new(1.0_f32, accent.gamma_multiply(0.7)),
        egui::StrokeKind::Outside,
    );
    ui.painter().text(
        Pos2::new(rect.center().x, rect.center().y),
        Align2::CENTER_CENTER,
        label,
        font,
        if dark { Color32::from_gray(220) } else { Color32::from_gray(40) },
    );
    response.clicked()
}
