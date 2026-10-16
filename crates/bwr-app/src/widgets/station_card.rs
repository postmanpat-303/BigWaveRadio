//! The compact station card grid — the heart of every list page.

use bwr_core::models::Station;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Ui, Vec2,
};

use crate::app::BigWaveApp;
use crate::theme;
use crate::widgets::spectrum_bars;

pub const CARD_WIDTH: f32 = 260.0;
pub const CARD_HEIGHT: f32 = 72.0;
const GAP: f32 = 10.0;

/// Draw the current station list as a wrapping grid of cards.
pub fn station_grid(ui: &mut Ui, app: &mut BigWaveApp) {
    let available = ui.available_width();
    let cols = ((available + GAP) / (CARD_WIDTH + GAP)).floor().max(1.0) as usize;

    let stations = app.visible_stations(); // honour the star rating filter
    if stations.is_empty() {
        ui.add_space(40.0);
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(if app.loading {
                    "Loading…"
                } else if app.settings.rating_filter > 0 && !app.stations.is_empty() {
                    "No stations with that rating."
                } else {
                    "No stations to show."
                })
                .weak(),
            );
        });
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for chunk in stations.chunks(cols) {
                ui.horizontal(|ui| {
                    for station in chunk {
                        card(ui, station, app);
                        if chunk.len() > 1 {
                            ui.add_space(GAP);
                        }
                    }
                });
                ui.add_space(GAP);
            }
        });
}

fn card(ui: &mut Ui, station: &Station, app: &mut BigWaveApp) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(CARD_WIDTH, CARD_HEIGHT), Sense::click());

    let is_current = app.current.as_ref().is_some_and(|c| c.uuid == station.uuid);
    let playing_here = is_current && app.is_playing;
    let in_library = app.library.contains(&station.uuid);

    // Intersect with the scroll viewport: painter_at REPLACES the clip rect
    // (it does not narrow it), so without this the card fill would paint even
    // where scrolled out of view — over the bottom panel.
    let visible = rect.intersect(ui.clip_rect());
    let mut painter = ui.painter_at(visible);
    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    let accent = theme::accent_color(app.settings.accent);
    let fill = if response.hovered() {
        theme::accent_soft(app.settings.accent)
    } else if dark {
        Color32::from_gray(35)
    } else {
        Color32::from_gray(235)
    };
    painter.rect_filled(rect, CornerRadius::same(8), fill);
    // Outline on the UNCLIPPED painter: an Outside stroke drawn through
    // painter_at() loses its outer half to the clip and looks "cut in".
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(8),
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Outside,
    );

    // Spectrum lives behind the card text while this station is playing.
    if playing_here {
        spectrum_bars::paint(ui, rect.shrink2(Vec2::new(6.0, 4.0)), &app.bands, app.settings.accent);
    }

    // Play circle.
    let circle_center = Pos2::new(rect.left() + 26.0, rect.center().y);
    painter.circle_filled(circle_center, 14.0, accent);
    painter.text(
        Pos2::new(circle_center.x + 1.0, circle_center.y),
        Align2::CENTER_CENTER,
        "▶",
        FontId::proportional(13.0),
        Color32::WHITE,
    );

    // Name + summary, clipped so they never run under the star button AND
    // stay inside the scroll viewport (set_clip_rect REPLACES the clip).
    let text_left = rect.left() + 48.0;
    let text_clip = Rect::from_min_max(
        Pos2::new(rect.left(), rect.top()),
        Pos2::new(rect.right() - 36.0, rect.bottom()),
    )
    .intersect(visible);
    painter.set_clip_rect(text_clip);
    painter.text(
        Pos2::new(text_left, rect.top() + 20.0),
        Align2::LEFT_CENTER,
        station.name.clone(),
        FontId::proportional(14.5),
        if dark { Color32::from_gray(240) } else { Color32::from_gray(20) },
    );
    // Rated stations show their stars ahead of the summary line.
    let rating = app.ratings.get(&station.uuid);
    let meta_x = if rating > 0 {
        let stars: String = (1..=5)
            .map(|s| if s <= rating { '★' } else { '☆' })
            .collect();
        let star_font = FontId::proportional(11.0);
        painter.text(
            Pos2::new(text_left, rect.top() + 46.0),
            Align2::LEFT_CENTER,
            stars.clone(),
            star_font.clone(),
            accent,
        );
        text_left + ui.fonts(|f| f.layout_no_wrap(stars, star_font, Color32::WHITE).size().x) + 6.0
    } else {
        text_left
    };
    painter.text(
        Pos2::new(meta_x, rect.top() + 46.0),
        Align2::LEFT_CENTER,
        station.summary_line(),
        FontId::proportional(12.0),
        if dark { Color32::from_gray(160) } else { Color32::from_gray(90) },
    );
    painter.set_clip_rect(ui.clip_rect());

    // Star toggle (top-right corner of the card). Frameless: only the star
    // glyph lights up in the accent colour — no filled box around it.
    let star_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 20.0, rect.top() + 22.0),
        Vec2::splat(26.0),
    );
    let star_color = if in_library {
        accent
    } else {
        ui.visuals().weak_text_color()
    };
    let star = ui.put(
        star_rect,
        egui::Button::new(
            egui::RichText::new(if in_library { "★" } else { "☆" })
                .size(16.0)
                .color(star_color),
        )
        .frame(false),
    );
    if star.clicked() {
        app.library.toggle(station);
    }

    // Card click plays (star swallows its own clicks).
    if response.clicked() {
        app.play_station(station.clone());
    }
    response.on_hover_text(&station.name);
}
