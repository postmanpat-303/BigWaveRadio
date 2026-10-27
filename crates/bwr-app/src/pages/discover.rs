//! Browse (Discover) page of the card look: search bar with the codec and
//! bitrate filters, six mood tiles, then the Discover (Trending / Popular /
//! Recently added) and Your stars cards.

use bwr_core::Station;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::app::{BigWaveApp, DiscoverTab, NetJob, Page};
use crate::theme;
use crate::widgets::cards::{self, Icon, Pal, PillStyle, AMBER, HEADER_H, PILL_H, ROW_H};
use crate::widgets::{mood_dial, mood_rail, search_bar, side_nav};

/// Tile header colour per mood id (the mockup palette); other moods cycle.
pub fn mood_colour(id: &str, i: usize) -> Color32 {
    match id {
        "dance" => Color32::from_rgb(0xe0, 0x5a, 0x8a),
        "chill" => Color32::from_rgb(0x5c, 0xc9, 0x7b),
        "trance" => Color32::from_rgb(0x7d, 0x6b, 0xd8),
        "easy" => Color32::from_rgb(0x4a, 0xa8, 0xd8),
        "80s" => Color32::from_rgb(0xf0, 0x8a, 0x4b),
        "90s" => Color32::from_rgb(0xf6, 0xc5, 0x6d),
        _ => [
            Color32::from_rgb(0x4f, 0xb3, 0xa9),
            Color32::from_rgb(0xc9, 0x6b, 0xd8),
            Color32::from_rgb(0x8a, 0x9b, 0xb0),
        ][i % 3],
    }
}

const PAD_X: f32 = 28.0;
const TILE_H: f32 = 156.0;
const TILE_HEAD: f32 = 54.0;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    let pal = Pal::new(ui.ctx(), app);
    egui::ScrollArea::vertical()
        .id_salt("browse-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let full_w = ui.available_width();
            let w = full_w - 2.0 * PAD_X;
            let x0 = ui.max_rect().left() + PAD_X;
            ui.add_space(24.0);

            // Search bar + filter pills.
            let (row, _) = ui.allocate_exact_size(Vec2::new(full_w, 46.0), Sense::hover());
            let pills_left = search_bar::filter_pills(ui, app, &pal, x0 + w, row.center().y);
            let bar = Rect::from_min_max(Pos2::new(x0, row.top()), Pos2::new((pills_left - 14.0).max(x0 + 220.0), row.bottom()));
            search_bar::bar(ui, app, &pal, bar);
            ui.add_space(24.0);

            // "Pick a mood" + show-talk toggle.
            let (head, _) = ui.allocate_exact_size(Vec2::new(full_w, 28.0), Sense::hover());
            ui.painter().text(
                Pos2::new(x0, head.center().y),
                Align2::LEFT_CENTER,
                t!("cardsui.pick_mood"),
                FontId::proportional(20.0),
                pal.text,
            );
            let mut show = app.discovery.show_talk;
            let tw = cards::text_width(ui, t!("rail.show_talk"), FontId::proportional(14.0)) + 34.0;
            let tr = Rect::from_min_size(Pos2::new(x0 + w - tw, head.top()), Vec2::new(tw, 28.0));
            let resp = ui
                .put(tr, egui::Checkbox::new(&mut show, egui::RichText::new(t!("rail.show_talk")).color(pal.sub)))
                .on_hover_text(t!("rail.show_talk_tip"));
            if resp.changed() {
                app.discovery.show_talk = show;
                app.discovery.save();
            }
            ui.add_space(14.0);

            // Mood tiles.
            let slots = mood_dial::slots(&app.moods.moods);
            let cols = if w >= 900.0 { 6 } else if w >= 560.0 { 3 } else { 2 };
            let gap = 16.0;
            let tile_w = (w - gap * (cols as f32 - 1.0)) / cols as f32;
            for chunk in slots.chunks(cols) {
                let (row, _) = ui.allocate_exact_size(Vec2::new(full_w, TILE_H), Sense::hover());
                for (j, &idx) in chunk.iter().enumerate() {
                    let r = Rect::from_min_size(Pos2::new(x0 + j as f32 * (tile_w + gap), row.top()), Vec2::new(tile_w, TILE_H));
                    mood_tile(ui, app, &pal, r, idx);
                }
                ui.add_space(gap);
            }
            ui.add_space(10.0);

            // Discover + Your stars.
            let card_h = 420.0;
            let (row, _) = ui.allocate_exact_size(Vec2::new(full_w, card_h), Sense::hover());
            if w >= 760.0 {
                let left_w = ((w - 20.0) * 0.58).floor();
                discover_card(ui, app, &pal, Rect::from_min_size(Pos2::new(x0, row.top()), Vec2::new(left_w, card_h)));
                stars_card(
                    ui,
                    app,
                    &pal,
                    Rect::from_min_max(Pos2::new(x0 + left_w + 20.0, row.top()), Pos2::new(x0 + w, row.bottom())),
                );
            } else {
                discover_card(ui, app, &pal, Rect::from_min_size(Pos2::new(x0, row.top()), Vec2::new(w, card_h)));
                ui.add_space(20.0);
                let (row2, _) = ui.allocate_exact_size(Vec2::new(full_w, card_h), Sense::hover());
                stars_card(ui, app, &pal, Rect::from_min_size(Pos2::new(x0, row2.top()), Vec2::new(w, card_h)));
            }
            ui.add_space(28.0);
        });
}

/// One mood tile: coloured header (name, count, rising spectrum lines),
/// then the top three stations. Header click tunes the Now Playing dial.
fn mood_tile(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, rect: Rect, idx: usize) {
    let Some(mood) = app.moods.moods.get(idx).cloned() else { return };
    let colour = mood_colour(&mood.id, idx);
    let (head, body) = cards::card_frame(ui.painter(), pal, rect, colour, TILE_HEAD);
    let id = ui.id().with(("tile", idx));
    let list = app.mood_stations(idx);
    let pending = app.mood_pending.contains(&idx);

    // Spectrum lines rising from the header's bottom edge (live while playing).
    let lines = Rect::from_min_max(Pos2::new(head.left() + 8.0, head.top() + 10.0), Pos2::new(head.right() - 8.0, head.bottom()));
    let levels: Vec<f32> = if app.current.is_some() { app.bands.to_vec() } else { vec![0.25; 8] };
    cards::spectrum_lines(ui.painter(), lines, Color32::WHITE, &levels, 7.0, true);

    let hr = ui.interact(head, id.with("head"), Sense::click());
    if hr.hovered() {
        ui.painter().rect_filled(head, CornerRadius { nw: cards::CARD_RADIUS, ne: cards::CARD_RADIUS, sw: 0, se: 0 }, cards::white_a(18));
    }
    let name = mood_rail::mood_label(&mood);
    let g = cards::line_galley(ui, &name, FontId::proportional(17.0), Color32::WHITE, head.width() - 24.0);
    ui.painter().galley(Pos2::new(head.left() + 12.0, head.top() + 8.0), g, Color32::WHITE);
    let count = match &list {
        Some(l) => tn!("cardsui.station_count", l.len()),
        None if pending => t!("cardsui.loading").to_string(),
        None => String::new(),
    };
    ui.painter().text(
        Pos2::new(head.left() + 12.0, head.bottom() - 12.0),
        Align2::LEFT_CENTER,
        count,
        FontId::proportional(11.5),
        cards::white_a(225),
    );
    if hr.on_hover_text(t!("cardsui.tune_tip", mood = name.clone())).clicked() {
        app.dial_mood = idx;
        side_nav::go(app, Page::Player);
        if app.page == Page::Player {
            app.tune_mood(idx);
        }
    }

    // Top three stations.
    let mut y = body.top() + 10.0;
    let line_h = (body.height() - 20.0) / 3.0;
    for (k, st) in list.unwrap_or_default().iter().take(3).enumerate() {
        let r = Rect::from_min_size(Pos2::new(body.left() + 6.0, y), Vec2::new(body.width() - 12.0, line_h));
        y += line_h;
        let resp = ui.interact(r, id.with(("st", k)), Sense::click());
        let playing = app.current.as_ref().is_some_and(|c| c.uuid == st.uuid);
        let col = if playing { pal.now } else if resp.hovered() { pal.text } else { pal.sub };
        if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(6), pal.line.gamma_multiply(0.5));
        }
        let g = cards::line_galley(ui, &st.name, FontId::proportional(12.5), col, r.width() - 12.0);
        ui.painter().galley(Pos2::new(r.left() + 6.0, r.center().y - g.size().y / 2.0), g, col);
        if resp.on_hover_text(cards::meta_line(st)).clicked() {
            let st = st.clone();
            app.dial_mood = idx;
            app.play_station(st);
        }
    }
}

/// The Discover card: amber header with Trending / Popular / Recent pills
/// and the ranked list (the current page's station list).
fn discover_card(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, rect: Rect) {
    let (head, body) = cards::card_frame(ui.painter(), pal, rect, AMBER, HEADER_H);
    let id = ui.id().with("discover-card");
    ui.painter().text(
        Pos2::new(head.left() + 20.0, head.center().y),
        Align2::LEFT_CENTER,
        t!("nav.discover"),
        FontId::proportional(20.0),
        Color32::WHITE,
    );
    let mut x = head.right() - 16.0;
    for (label, tab) in [
        (t!("discover.recent"), DiscoverTab::Recent),
        (t!("discover.popular"), DiscoverTab::Popular),
        (t!("discover.trending"), DiscoverTab::Trending),
    ] {
        let w = cards::pill_width(ui, label, None);
        x -= w;
        let r = Rect::from_min_size(Pos2::new(x, head.center().y - PILL_H / 2.0), Vec2::new(w, PILL_H));
        x -= 8.0;
        let selected = app.discover_tab == tab;
        let style = if selected {
            PillStyle { fill: Color32::WHITE, text: pal_amber_dark(), border: Color32::WHITE }
        } else {
            PillStyle { fill: Color32::TRANSPARENT, text: Color32::WHITE, border: cards::white_a(170) }
        };
        if cards::pill_at(ui, r, id.with(("tab", label)), label, None, style).clicked() && !selected {
            app.discover_tab = tab;
            app.loading = true;
            let _ = app.job_tx.send(NetJob::Discover(tab));
        }
    }

    let stations = app.visible_stations();
    let list = body.shrink2(Vec2::new(14.0, 10.0));
    let empty = if app.loading { t!("cardsui.loading") } else { t!("cardsui.list_empty") };
    list_area(ui, app, pal, list, ("discover-list", app.discover_tab as u8), &stations, true, empty);
}

fn pal_amber_dark() -> Color32 {
    Color32::from_rgb(0x9a, 0x6a, 0x12)
}

/// Your stars card (green header): your rated stations, best first.
fn stars_card(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, rect: Rect) {
    let (head, body) = cards::card_frame(ui.painter(), pal, rect, pal.primary, HEADER_H);
    let id = ui.id().with("stars-card");
    ui.painter().text(
        Pos2::new(head.left() + 20.0, head.center().y),
        Align2::LEFT_CENTER,
        t!("stars.title"),
        FontId::proportional(20.0),
        Color32::WHITE,
    );
    let rated = app.ratings.rated_stations();
    if cards::header_icon(ui, Pos2::new(head.right() - 30.0, head.center().y), Icon::Forward, id.with("all"), t!("cardsui.see_all")) {
        side_nav::go(app, Page::Stars);
    }
    let stations: Vec<Station> = rated.into_iter().map(|(_, s)| s).collect();
    let list = body.shrink2(Vec2::new(14.0, 10.0));
    list_area(ui, app, pal, list, ("stars-list", 0), &stations, false, t!("cardsui.stars_empty"));
}

#[allow(clippy::too_many_arguments)]
fn list_area(
    ui: &mut Ui,
    app: &mut BigWaveApp,
    pal: &Pal,
    list: Rect,
    salt: (&str, u8),
    stations: &[Station],
    ranked: bool,
    empty: &str,
) {
    let note = stations.is_empty().then(|| empty.to_string());
    ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
        egui::ScrollArea::vertical().id_salt(salt).auto_shrink([false, false]).show(ui, |ui| {
            if let Some(n) = &note {
                cards::list_note(ui, pal, n);
            }
            for (i, st) in stations.iter().enumerate() {
                let opts = cards::RowOpts { rank: ranked.then_some(i + 1), play_button: ranked };
                cards::station_row(ui, app, pal, st, &opts);
            }
            ui.add_space(ROW_H * 0.3);
        });
    });
    let fade = Rect::from_min_max(Pos2::new(list.left(), list.bottom() - 22.0), list.max);
    cards::fade(ui.painter(), fade, pal.card);
}

/// Outlined pill tab: accent fill when selected, outline always (used by
/// the Library page).
pub(crate) fn tab_button(ui: &mut Ui, app: &BigWaveApp, label: &str, selected: bool) -> bool {
    let accent = theme::accent_color(app.settings.accent);
    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    let font = FontId::proportional(15.0);
    let text_w = ui.fonts(|f| f.layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE).size().x);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(text_w + 28.0, 32.0), Sense::click());

    if response.hovered() && !selected {
        ui.painter().rect_filled(rect, CornerRadius::same(6), accent.gamma_multiply(0.12_f32));
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
