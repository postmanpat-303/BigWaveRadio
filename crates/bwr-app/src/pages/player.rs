//! Now Playing: the player card (readout, Mood dial, pills, transport)
//! beside the current mood's station list.

use bwr_core::Station;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Key, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::app::{BigWaveApp, Page};
use crate::widgets::cards::{self, Icon, Pal, PillStyle, AMBER, HEADER_H, PILL_H};
use crate::widgets::{mood_dial, side_nav};

/// The pills above the station list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListFilter {
    #[default]
    All,
    Favourites,
    FourPlus,
    Kbps128,
}

impl ListFilter {
    pub fn accepts(self, app: &BigWaveApp, s: &Station) -> bool {
        match self {
            ListFilter::All => true,
            ListFilter::Favourites => app.library.contains(&s.uuid),
            ListFilter::FourPlus => app.ratings.get(&s.uuid) >= 4,
            ListFilter::Kbps128 => s.bitrate >= 128,
        }
    }
}

const PLAYER_MIN_H: f32 = 600.0;
const LIST_MIN_H: f32 = 480.0;
const GAP: f32 = 18.0;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    let pal = Pal::new(ui.ctx(), app);
    arrow_keys(ui, app);
    let avail = ui.available_rect_before_wrap();
    let pad = Vec2::new(28.0, 26.0);
    let inner_w = avail.width() - 2.0 * pad.x;
    let wide = inner_w >= 700.0;
    let content_h = if wide {
        (avail.height() - 2.0 * pad.y).max(PLAYER_MIN_H)
    } else {
        PLAYER_MIN_H + GAP + LIST_MIN_H
    };
    egui::ScrollArea::vertical()
        .id_salt("player-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let (full, _) = ui.allocate_exact_size(Vec2::new(avail.width(), content_h + 2.0 * pad.y), Sense::hover());
            let area = full.shrink2(pad);
            let (player, list) = if wide {
                let pw = ((area.width() - GAP) * 0.54).clamp(360.0, 540.0);
                (
                    Rect::from_min_size(area.min, Vec2::new(pw, area.height())),
                    Rect::from_min_max(Pos2::new(area.left() + pw + GAP, area.top()), area.max),
                )
            } else {
                (
                    Rect::from_min_size(area.min, Vec2::new(area.width(), PLAYER_MIN_H)),
                    Rect::from_min_max(Pos2::new(area.left(), area.top() + PLAYER_MIN_H + GAP), area.max),
                )
            };
            player_card(ui, app, &pal, player);
            stations_card(ui, app, &pal, list);
        });
}

/// ← / → step through the mood's stations. Only when nothing has keyboard
/// focus and no plugin overlay is up (the host owns the keyboard, and Esc,
/// while an overlay runs).
fn arrow_keys(ui: &Ui, app: &mut BigWaveApp) {
    if ui.ctx().wants_keyboard_input() || app.plugins.host.overlay.is_some() {
        return;
    }
    let (left, right) = ui.input(|i| (i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight)));
    if left {
        app.step_station(false);
    } else if right {
        app.step_station(true);
    }
}

fn dial_labels(app: &BigWaveApp) -> (Vec<usize>, Vec<String>) {
    let slots = mood_dial::slots(&app.moods.moods);
    let labels = slots
        .iter()
        .map(|&i| crate::widgets::mood_rail::mood_label(&app.moods.moods[i]))
        .collect();
    (slots, labels)
}

/// Format the strobe cap: "3", "2.5".
pub fn hz_label(hz: f64) -> String {
    let s = format!("{hz:.1}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn player_card(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, rect: Rect) {
    let (head, body) = cards::card_frame(ui.painter(), pal, rect, pal.primary, HEADER_H);
    let id = ui.id().with("player-card");

    // --- Header: menu · "Now Playing" · power -------------------------------
    ui.painter().text(
        head.center(),
        Align2::CENTER_CENTER,
        t!("cardsui.now_playing"),
        FontId::proportional(21.0),
        Color32::WHITE,
    );
    let menu_c = Pos2::new(head.left() + 30.0, head.center().y);
    let menu_rect = Rect::from_center_size(menu_c, Vec2::splat(36.0));
    let menu_resp = ui.interact(menu_rect, id.with("menu"), Sense::click());
    if menu_resp.hovered() {
        ui.painter().circle_filled(menu_c, 18.0, cards::white_a(36));
    }
    cards::icon(ui.painter(), Icon::Menu, menu_c, 24.0, Color32::WHITE);
    let popup_id = id.with("menu-popup");
    if menu_resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    let menu_resp = menu_resp.on_hover_text(t!("cardsui.menu"));
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &menu_resp,
        egui::popup::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(220.0);
            header_menu(ui, app);
        },
    );
    if cards::header_icon(ui, Pos2::new(head.right() - 30.0, head.center().y), Icon::Power, id.with("power"), t!("cardsui.stop")) {
        app.stop();
    }

    let inner = body.shrink2(Vec2::new(22.0, 0.0));
    let current = app.current.clone();

    // --- Heart (library) and stars ------------------------------------------
    let top_y = body.top() + 30.0;
    if let Some(st) = &current {
        let saved = app.library.contains(&st.uuid);
        let hc = Pos2::new(inner.left() + 15.0, top_y);
        let hr = ui.interact(Rect::from_center_size(hc, Vec2::splat(32.0)), id.with("heart"), Sense::click());
        let p = ui.painter();
        p.circle_stroke(hc, 15.0, Stroke::new(1.4_f32, if hr.hovered() { pal.now } else { pal.primary }));
        cards::icon(p, if saved { Icon::HeartFilled } else { Icon::Heart }, hc, 16.0, pal.primary);
        let tip = if saved { t!("player.remove_library") } else { t!("player.save_library") };
        if hr.on_hover_text(tip).clicked() {
            app.library.toggle(st);
        }
        let rating = app.ratings.get(&st.uuid);
        let sr = Rect::from_min_size(Pos2::new(inner.right() - 84.0, top_y - 8.0), Vec2::new(84.0, 16.0));
        if let Some(new) = cards::rating_input(ui, pal, sr, rating, id.with("stars")) {
            cards::set_rating(app, st, new);
        }
    }

    // --- Bottom-up: transport, pills, dial ----------------------------------
    let transport_y = rect.bottom() - 44.0;
    let pills_y = transport_y - 62.0;
    let dial_top = pills_y - PILL_H / 2.0 - 16.0 - mood_dial::HEIGHT;
    let region = Rect::from_min_max(Pos2::new(inner.left(), top_y + 22.0), Pos2::new(inner.right(), dial_top - 8.0));

    readout(ui, app, pal, region, current.as_ref(), id);

    // Mood dial.
    let (slots, labels) = dial_labels(app);
    if !slots.is_empty() {
        let selected = slots.iter().position(|&i| i == app.dial_mood).unwrap_or(0);
        let dial_rect = Rect::from_min_size(Pos2::new(inner.left(), dial_top), Vec2::new(inner.width(), mood_dial::HEIGHT));
        if let Some(slot) = mood_dial::dial(ui, pal, dial_rect, &labels, selected) {
            app.tune_mood(slots[slot]);
        }
    }

    // Pills: Station info (left), Visions + Beat Strobe (right).
    let info_label = t!("cardsui.station_info");
    let w = cards::pill_width(ui, info_label, Some(Icon::Info));
    let r = Rect::from_min_size(Pos2::new(inner.left(), pills_y - PILL_H / 2.0), Vec2::new(w, PILL_H));
    let style = if app.show_info { PillStyle::active(pal.primary) } else { PillStyle::normal(pal) };
    if cards::pill_at(ui, r, id.with("info-pill"), info_label, Some(Icon::Info), style)
        .on_hover_text(t!("cardsui.station_info_tip"))
        .clicked()
    {
        app.show_info = !app.show_info;
    }
    let mut x = inner.right();
    for (pid, icon) in [("strobe", Icon::Bolt), ("visions", Icon::Eye)] {
        let Some(idx) = app.plugins.pages.iter().position(|(p, _)| p == pid) else { continue };
        let title = app.plugins.pages[idx].1.title_for(crate::i18n::current().code()).to_string();
        let label = if pid == "strobe" {
            t!("cardsui.strobe_rate", title = title, hz = hz_label(app.plugins.host.flash_cap()))
        } else {
            title.clone()
        };
        let w = cards::pill_width(ui, &label, Some(icon));
        let r = Rect::from_min_size(Pos2::new(x - w, pills_y - PILL_H / 2.0), Vec2::new(w, PILL_H));
        x -= w + 10.0;
        let open = app.page == Page::PluginPage(idx);
        let style = if open { PillStyle::active(pal.primary) } else { PillStyle::normal(pal) };
        if cards::pill_at(ui, r, id.with(("plugin-pill", pid)), &label, Some(icon), style)
            .on_hover_text(t!("nav.plugin_tip", title = title))
            .clicked()
        {
            side_nav::go(app, Page::PluginPage(idx));
        }
    }

    // Transport + volume.
    transport(ui, app, pal, Rect::from_min_max(Pos2::new(inner.left(), transport_y - 30.0), Pos2::new(inner.right(), transport_y + 30.0)), id, true);
}

/// Prev / play / next and the volume bar along one row.
pub fn transport(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, row: Rect, id: egui::Id, big: bool) {
    let y = row.center().y;
    let (small, large, step) = if big { (21.0, 29.0, 64.0) } else { (15.0, 20.0, 46.0) };
    let has_queue = !app.play_queue().is_empty();
    let prev_c = Pos2::new(row.left() + small, y);
    if cards::round_button(ui, prev_c, small, Icon::Prev, None, pal, has_queue, id.with("prev"), t!("cardsui.prev")) {
        app.step_station(false);
    }
    let play_c = Pos2::new(prev_c.x + step, y);
    let (ic, tip) = if app.is_playing { (Icon::Pause, t!("cardsui.pause")) } else { (Icon::Play, t!("cardsui.play")) };
    let can_play = app.current.is_some() || has_queue;
    if cards::round_button(ui, play_c, large, ic, Some(pal.primary), pal, can_play, id.with("play"), tip) {
        if app.current.is_some() {
            app.toggle_play_pause();
        } else {
            app.step_station(true);
        }
    }
    let next_c = Pos2::new(play_c.x + step, y);
    if cards::round_button(ui, next_c, small, Icon::Next, None, pal, has_queue, id.with("next"), t!("cardsui.next")) {
        app.step_station(true);
    }
    // Volume.
    let vol = app.player.volume();
    let sp = Pos2::new(next_c.x + small + 26.0, y);
    cards::icon(ui.painter(), Icon::Speaker, sp, 16.0, pal.sub);
    let pct_w = if big { 44.0 } else { 0.0 };
    let slider = Rect::from_min_max(Pos2::new(sp.x + 16.0, y - 6.0), Pos2::new(row.right() - pct_w, y + 6.0));
    if slider.width() > 30.0 {
        if let Some(v) = cards::thin_slider(ui, slider, vol, pal, id.with("vol")) {
            app.player.set_volume(v);
            app.settings.volume = v;
        }
    }
    if big {
        ui.painter().text(
            Pos2::new(row.right(), y),
            Align2::RIGHT_CENTER,
            format!("{}%", (app.player.volume() * 100.0).round() as i32),
            FontId::proportional(13.0),
            pal.sub,
        );
    }
}

/// Fit a station name into `max_w` with the readout face: shrink from
/// `max` down to `min` px, then elide.
pub fn fit_readout(ui: &Ui, name: &str, max: f32, min: f32, max_w: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    let mut size = max;
    while size > min && cards::text_width(ui, name, cards::readout_font(size)) > max_w {
        size -= 2.0;
    }
    cards::line_galley(ui, name, cards::readout_font(size), color, max_w)
}

fn readout(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, region: Rect, current: Option<&Station>, id: egui::Id) {
    let name = current
        .map(|s| s.name.trim().to_string())
        .unwrap_or_else(|| "BigWave Radio".to_string());
    let arrows_w = 44.0;
    let text_w = region.width() - 2.0 * arrows_w;
    let galley = fit_readout(ui, &name, 52.0, 26.0, text_w, pal.text);
    let meta = current.map(cards::meta_line).unwrap_or_default();
    let tags: Vec<String> = current
        .map(|s| {
            s.tags
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .take(3)
                .map(|t| format!("#{t}"))
                .collect()
        })
        .unwrap_or_default();
    let (now_text, now_col) = match (&app.song_title, current) {
        (Some(song), Some(_)) => (t!("cardsui.now_line", song = song), pal.now),
        (_, Some(_)) => (app.status.clone(), pal.weak),
        _ => (t!("player.pick").to_string(), pal.weak),
    };

    let name_h = galley.size().y;
    let tags_h = if tags.is_empty() { 0.0 } else { 22.0 + 12.0 };
    let total = name_h + 4.0 + 18.0 + 12.0 + tags_h + 18.0;
    let mut y = region.center().y - total / 2.0;
    let name_cy = y + name_h / 2.0;

    // Live spectrum lines behind the text.
    let lines = Rect::from_center_size(
        Pos2::new(region.center().x, name_cy + 8.0),
        Vec2::new(region.width() - 2.0 * arrows_w + 40.0, (region.height() * 0.95).min(250.0)),
    );
    cards::spectrum_lines(ui.painter(), lines, pal.primary, &app.bands, 7.0, false);

    let p = ui.painter();
    p.galley(Pos2::new(region.center().x - galley.size().x / 2.0, y), galley, pal.text);
    y += name_h + 4.0;
    let mg = cards::line_galley(ui, &meta, FontId::proportional(14.5), pal.sub, text_w);
    ui.painter().galley(Pos2::new(region.center().x - mg.size().x / 2.0, y), mg, pal.sub);
    y += 18.0 + 12.0;
    if !tags.is_empty() {
        let font = FontId::proportional(12.0);
        let widths: Vec<f32> = tags.iter().map(|t| cards::text_width(ui, t, font.clone()) + 18.0).collect();
        let total_w: f32 = widths.iter().sum::<f32>() + 8.0 * (widths.len() as f32 - 1.0);
        let mut x = region.center().x - total_w / 2.0;
        for (i, (tag, w)) in tags.iter().zip(widths).enumerate() {
            let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 22.0));
            let resp = ui.interact(r, id.with(("tag", i)), Sense::click());
            let fill = pal.primary.gamma_multiply(if resp.hovered() { 0.3 } else { 0.16 });
            ui.painter().rect_filled(r, CornerRadius::same(5), fill);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, tag, font.clone(), pal.now);
            if resp.on_hover_text(t!("info.more_like")).clicked() {
                if let Some(st) = current {
                    let tag = tag.trim_start_matches('#').to_string();
                    app.more_like_this(&tag, &st.name.clone());
                }
            }
            x += w + 8.0;
        }
        y += tags_h;
    }
    let ng = cards::line_galley(ui, &now_text, FontId::proportional(13.5), now_col, text_w);
    ui.painter().galley(Pos2::new(region.center().x - ng.size().x / 2.0, y), ng, now_col);

    // Prev / next arrows step through the mood's stations.
    let enabled = !app.play_queue().is_empty();
    for (forward, x, ic, tip) in [
        (false, region.left() + 16.0, Icon::Back, t!("cardsui.prev")),
        (true, region.right() - 16.0, Icon::Forward, t!("cardsui.next")),
    ] {
        let c = Pos2::new(x, name_cy);
        let resp = ui.interact(Rect::from_center_size(c, Vec2::splat(40.0)), id.with(("arrow", forward)), Sense::click());
        let col = if !enabled {
            pal.weak.gamma_multiply(0.5)
        } else if resp.hovered() {
            pal.now
        } else {
            pal.text
        };
        cards::icon(ui.painter(), ic, c, 26.0, col);
        if enabled && resp.on_hover_text(tip).clicked() {
            app.step_station(forward);
        }
    }
}

/// ☰ menu in the Now Playing header.
fn header_menu(ui: &mut Ui, app: &mut BigWaveApp) {
    if ui
        .selectable_label(app.show_info, format!("ⓘ  {}", t!("cardsui.station_info")))
        .clicked()
    {
        app.show_info = !app.show_info;
    }
    if let Some(st) = app.current.clone() {
        if ui.button(format!("✨  {}", t!("info.more_like"))).clicked() {
            app.more_like_station(&st);
        }
        let saved = app.library.contains(&st.uuid);
        let label = if saved { t!("player.remove_library") } else { t!("player.save_library") };
        if ui.button(format!("♥  {label}")).clicked() {
            app.library.toggle(&st);
        }
    }
    if ui.button(format!("🎲  {}", t!("info.surprise"))).clicked() {
        app.surprise();
    }
    ui.separator();
    if ui.button(format!("▭  {}", t!("cardsui.mini_player"))).clicked() {
        app.mini_open = true;
    }
}

fn stations_card(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, rect: Rect) {
    let (head, body) = cards::card_frame(ui.painter(), pal, rect, AMBER, HEADER_H);
    let id = ui.id().with("stations-card");
    let mood_idx = app.dial_mood;
    let mood_name = app
        .moods
        .moods
        .get(mood_idx)
        .map(crate::widgets::mood_rail::mood_label)
        .unwrap_or_default();
    let all = app.visible_stations();

    // Header: back · "Chill 48 stations" · search.
    if cards::header_icon(ui, Pos2::new(head.left() + 30.0, head.center().y), Icon::Back, id.with("back"), t!("cardsui.browse")) {
        side_nav::go(app, Page::Discover);
    }
    if cards::header_icon(ui, Pos2::new(head.right() - 30.0, head.center().y), Icon::Search, id.with("search"), t!("nav.search")) {
        side_nav::go(app, Page::Search);
    }
    let count = tn!("cardsui.station_count", all.len());
    let title_font = FontId::proportional(21.0);
    let count_font = FontId::proportional(12.5);
    let max_title = head.width() - 140.0;
    let tg = cards::line_galley(ui, &mood_name, title_font, Color32::WHITE, (max_title - 80.0).max(40.0));
    let cg = cards::line_galley(ui, &count, count_font, cards::white_a(220), 120.0);
    let total = tg.size().x + 8.0 + cg.size().x;
    let x0 = head.center().x - total / 2.0;
    let title_rect = Rect::from_min_size(Pos2::new(x0, head.center().y - 14.0), Vec2::new(total, 28.0));
    let tr = ui.interact(title_rect, id.with("title"), Sense::click());
    ui.painter().galley(Pos2::new(x0, head.center().y - tg.size().y / 2.0), tg.clone(), Color32::WHITE);
    ui.painter().galley(
        Pos2::new(x0 + tg.size().x + 8.0, head.center().y - cg.size().y / 2.0 + 2.0),
        cg,
        cards::white_a(220),
    );
    if tr.on_hover_text(t!("cardsui.open_mood")).clicked() {
        app.open_mood(mood_idx);
    }

    // Filter pills.
    let inner = body.shrink2(Vec2::new(16.0, 0.0));
    let kbps = t!("cardsui.filter_kbps", kbps = 128);
    let pills: [(ListFilter, &str, Option<Icon>); 4] = [
        (ListFilter::All, t!("cardsui.filter_all"), None),
        (ListFilter::Favourites, t!("cardsui.filter_favourites"), Some(Icon::HeartFilled)),
        (ListFilter::FourPlus, "4+", Some(Icon::Star)),
        (ListFilter::Kbps128, &kbps, None),
    ];
    let mut x = inner.left();
    let py = body.top() + 16.0;
    for (f, label, ic) in pills {
        let w = cards::pill_width(ui, label, ic);
        let r = Rect::from_min_size(Pos2::new(x, py), Vec2::new(w, PILL_H));
        x += w + 8.0;
        let style = if app.list_filter == f { PillStyle::active(AMBER) } else { PillStyle::normal(pal) };
        if cards::pill_at(ui, r, id.with(("filter", label)), label, ic, style).clicked() {
            app.list_filter = f;
        }
    }

    // The list.
    let list = Rect::from_min_max(Pos2::new(inner.left(), py + PILL_H + 10.0), Pos2::new(inner.right(), body.bottom() - 12.0));
    let filter = app.list_filter;
    let shown: Vec<Station> = all.iter().filter(|s| filter.accepts(app, s)).cloned().collect();
    let pending = app.mood_pending.contains(&mood_idx);
    ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
        egui::ScrollArea::vertical()
            .id_salt(("player-list", mood_idx))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if shown.is_empty() {
                    let note = if pending {
                        t!("cardsui.loading")
                    } else if !all.is_empty() {
                        t!("cardsui.filter_empty")
                    } else {
                        t!("cardsui.list_empty")
                    };
                    cards::list_note(ui, pal, note);
                }
                let opts = cards::RowOpts { rank: None, play_button: false };
                for st in &shown {
                    cards::station_row(ui, app, pal, st, &opts);
                }
                ui.add_space(18.0);
            });
    });
    let fade = Rect::from_min_max(Pos2::new(list.left(), list.bottom() - 22.0), list.max);
    cards::fade(ui.painter(), fade, pal.card);
}
