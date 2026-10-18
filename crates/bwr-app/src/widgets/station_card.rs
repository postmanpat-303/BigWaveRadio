//! The compact station card grid — the heart of every list page.

use bwr_core::models::Station;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Ui, Vec2,
};

use crate::app::BigWaveApp;
use crate::theme;
use crate::widgets::spectrum_bars;

/// Card metrics for one density.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub min_width: f32,
    pub height: f32,
    pub gap: f32,
    pub play_r: f32,
    pub pad: f32,
    pub name_size: f32,
    pub meta_size: f32,
}

pub const COMPACT: Metrics = Metrics {
    min_width: 200.0,
    height: 46.0,
    gap: 6.0,
    play_r: 11.0,
    pad: 8.0,
    name_size: 13.5,
    meta_size: 11.0,
};

pub const COMFORTABLE: Metrics = Metrics {
    min_width: 260.0,
    height: 72.0,
    gap: 10.0,
    play_r: 14.0,
    pad: 12.0,
    name_size: 14.5,
    meta_size: 12.0,
};

/// Columns and card width for the available width: as many columns as fit
/// at the minimum width, then cards stretch to fill the row exactly.
pub fn grid_layout(available: f32, m: &Metrics) -> (usize, f32) {
    let cols = ((available + m.gap) / (m.min_width + m.gap)).floor().max(1.0) as usize;
    let width = ((available - m.gap * (cols as f32 - 1.0)) / cols as f32).max(m.min_width.min(available));
    (cols, width.floor())
}

/// Draw the current station list as a wrapping grid of cards.
pub fn station_grid(ui: &mut Ui, app: &mut BigWaveApp) {
    let m = if app.discovery.comfortable_cards { COMFORTABLE } else { COMPACT };
    // Leave room for the scroll bar so the last column never clips.
    let available = ui.available_width() - 12.0;
    let (cols, card_w) = grid_layout(available, &m);

    let stations = app.visible_stations(); // talk filter (unless Show talk)
    if stations.is_empty() {
        ui.add_space(40.0);
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(if app.loading {
                    t!("cards.loading")
                } else if !app.stations.is_empty() {
                    t!("cards.only_talk")
                } else {
                    t!("cards.empty")
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
                    ui.spacing_mut().item_spacing.x = m.gap;
                    for station in chunk {
                        card(ui, station, app, &m, card_w);
                    }
                });
                ui.add_space(m.gap);
            }
        });
}

fn card(ui: &mut Ui, station: &Station, app: &mut BigWaveApp, m: &Metrics, width: f32) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, m.height), Sense::click());
    let compact = m.height < 60.0;
    let radius = if compact { 6 } else { 8 };

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
    painter.rect_filled(rect, CornerRadius::same(radius), fill);
    // Outline on the UNCLIPPED painter: an Outside stroke drawn through
    // painter_at() loses its outer half to the clip and looks "cut in".
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(radius),
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Outside,
    );

    // Spectrum lives behind the card text while this station is playing.
    if playing_here {
        spectrum_bars::paint(ui, rect.shrink2(Vec2::new(6.0, 4.0)), &app.bands, app.settings.accent);
    }

    // Play circle.
    let circle_center = Pos2::new(rect.left() + m.pad + m.play_r, rect.center().y);
    painter.circle_filled(circle_center, m.play_r, accent);
    painter.text(
        Pos2::new(circle_center.x + 1.0, circle_center.y),
        Align2::CENTER_CENTER,
        "▶",
        FontId::proportional(m.play_r * 0.9),
        Color32::WHITE,
    );

    // Name + summary, clipped so they never run under the star button AND
    // stay inside the scroll viewport (set_clip_rect REPLACES the clip).
    let text_left = circle_center.x + m.play_r + m.pad;
    let star_w = if compact { 24.0 } else { 36.0 };
    let (name_y, meta_y) = if compact {
        (rect.top() + 15.0, rect.top() + 32.0)
    } else {
        (rect.top() + 20.0, rect.top() + 46.0)
    };
    let text_clip = Rect::from_min_max(
        Pos2::new(rect.left(), rect.top()),
        Pos2::new(rect.right() - star_w, rect.bottom()),
    )
    .intersect(visible);
    painter.set_clip_rect(text_clip);
    painter.text(
        Pos2::new(text_left, name_y),
        Align2::LEFT_CENTER,
        station.name.clone(),
        FontId::proportional(m.name_size),
        if dark { Color32::from_gray(240) } else { Color32::from_gray(20) },
    );
    // Rated stations show their stars ahead of the summary line.
    let rating = app.ratings.get(&station.uuid);
    let meta_x = if rating > 0 {
        let stars: String = (1..=5)
            .map(|s| if s <= rating { '★' } else { '☆' })
            .collect();
        let star_font = FontId::proportional(m.meta_size);
        painter.text(
            Pos2::new(text_left, meta_y),
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
        Pos2::new(meta_x, meta_y),
        Align2::LEFT_CENTER,
        summary_line(station),
        FontId::proportional(m.meta_size),
        if dark { Color32::from_gray(160) } else { Color32::from_gray(90) },
    );
    painter.set_clip_rect(ui.clip_rect());

    // Star toggle (top-right corner of the card). Frameless: only the star
    // glyph lights up in the accent colour — no filled box around it.
    let star_rect = Rect::from_center_size(
        Pos2::new(rect.right() - star_w / 2.0 - 2.0, if compact { rect.center().y } else { rect.top() + 22.0 }),
        Vec2::splat(if compact { 22.0 } else { 26.0 }),
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
                .size(if compact { 14.0 } else { 16.0 })
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

/// "Country · Language · MP3 · 128 kbps · 42 votes" card subtitle (the
/// translated twin of `Station::summary_line`).
fn summary_line(s: &bwr_core::Station) -> String {
    let mut parts: Vec<String> = [s.country.trim(), s.language.trim()]
        .into_iter()
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    if !s.codec.trim().is_empty() {
        parts.push(s.codec.trim().to_uppercase());
    }
    if s.bitrate > 0 {
        parts.push(format!("{} kbps", s.bitrate));
    }
    if s.votes > 0 {
        parts.push(tn!("cards.votes", s.votes));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_fits_far_more_cards_and_columns_adapt() {
        let (w, h) = (840.0, 470.0);
        let count = |m: &Metrics| {
            let (cols, _) = grid_layout(w, m);
            let rows = ((h + m.gap) / (m.height + m.gap)).floor() as usize;
            cols * rows
        };
        let ratio = count(&COMPACT) as f32 / count(&COMFORTABLE) as f32;
        assert!((1.5..=2.6).contains(&ratio), "ratio {ratio}");
        // Columns follow the width and cards fill the row exactly.
        for avail in [300.0, 640.0, 900.0, 1500.0] {
            let (cols, cw) = grid_layout(avail, &COMPACT);
            assert!(cw >= COMPACT.min_width.min(avail));
            assert!(cols as f32 * cw + (cols as f32 - 1.0) * COMPACT.gap <= avail + 0.5);
        }
        assert!(grid_layout(1500.0, &COMPACT).0 > grid_layout(640.0, &COMPACT).0);
    }
}
