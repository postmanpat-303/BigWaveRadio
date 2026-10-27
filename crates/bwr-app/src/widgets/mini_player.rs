//! The compact strip (bottom of every page except Now Playing) and the
//! always-on-top mini player, shown as a second native window (egui
//! viewport). Where the platform can't open one, it falls back to a
//! floating window inside the main one.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2, ViewportCommand, ViewportId};

use crate::app::{BigWaveApp, Page};
use crate::pages::player;
use crate::widgets::cards::{self, Icon, Pal};
use crate::widgets::{mood_rail, side_nav};

pub const STRIP_H: f32 = 58.0;
const MINI_SIZE: [f32; 2] = [420.0, 210.0];

fn logo(ui: &Ui, app: &BigWaveApp, rect: Rect) {
    ui.painter().image(
        app.about_logo_icon.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
}

fn now_text(app: &BigWaveApp) -> Option<String> {
    app.song_title.as_ref().filter(|s| !s.is_empty()).map(|s| t!("cardsui.now_line", song = s))
}

/// Compact strip: accent bar, logo, name / now playing, prev-play-next,
/// then mini-player and expand buttons.
pub fn strip(ui: &mut Ui, app: &mut BigWaveApp) {
    let pal = Pal::new(ui.ctx(), app);
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, pal.card);
    ui.painter().hline(full.x_range(), full.top() + 0.5, Stroke::new(1.0_f32, pal.line));
    ui.painter().rect_filled(Rect::from_min_size(full.min, Vec2::new(6.0, full.height())), 0.0, pal.primary);
    let id = ui.id().with("strip");

    let l = Rect::from_center_size(Pos2::new(full.left() + 34.0, full.center().y), Vec2::splat(36.0));
    logo(ui, app, l);

    // Right side: mini + expand.
    let ex_c = Pos2::new(full.right() - 26.0, full.center().y);
    if cards::round_button(ui, ex_c, 15.0, Icon::Expand, None, &pal, true, id.with("expand"), t!("cardsui.expand")) {
        side_nav::go(app, Page::Player);
    }
    let mini_c = Pos2::new(ex_c.x - 40.0, full.center().y);
    if cards::round_button(ui, mini_c, 15.0, Icon::Mini, None, &pal, true, id.with("mini"), t!("cardsui.mini_player")) {
        app.mini_open = !app.mini_open;
    }
    // Transport (small), with the volume slider capped in width.
    let t_w = 300.0_f32.min((full.width() - 300.0).max(140.0));
    let row = Rect::from_min_max(Pos2::new(mini_c.x - 30.0 - t_w, full.top()), Pos2::new(mini_c.x - 30.0, full.bottom()));
    player::transport(ui, app, &pal, row, id.with("t"), false);

    let text_l = l.right() + 12.0;
    let max_w = (row.left() - 20.0 - text_l).max(40.0);
    let name = app.current.as_ref().map(|s| s.name.trim().to_string()).unwrap_or_else(|| t!("player.pick").to_string());
    let g = cards::line_galley(ui, &name, FontId::proportional(14.5), pal.text, max_w);
    let sub = now_text(app).or_else(|| app.current.as_ref().map(cards::meta_line)).unwrap_or_default();
    let sg = cards::line_galley(ui, &sub, FontId::proportional(11.5), pal.now, max_w);
    let top = full.center().y - (g.size().y + sg.size().y + 2.0) / 2.0;
    let tr = Rect::from_min_size(Pos2::new(text_l, top), Vec2::new(max_w, g.size().y + sg.size().y));
    ui.painter().galley(Pos2::new(text_l, top), g, pal.text);
    ui.painter().galley(Pos2::new(text_l, top + 18.0), sg, pal.now);
    if ui.interact(tr, id.with("open"), Sense::click()).on_hover_text(t!("cardsui.expand")).clicked() {
        side_nav::go(app, Page::Player);
    }
}

enum MiniAction {
    None,
    Expand,
    Close,
}

/// The mini player's content filling `ui`.
fn contents(ui: &mut Ui, app: &mut BigWaveApp, native: bool) -> MiniAction {
    let pal = Pal::new(ui.ctx(), app);
    let full = ui.max_rect();
    let id = ui.id().with("mini");
    let (head, body) = cards::card_frame(ui.painter(), &pal, full, pal.primary, 42.0);
    let mut action = MiniAction::None;

    // Header: logo · "BigWave · Chill", expand, close. Drag moves the window.
    let hr = ui.interact(head, id.with("head"), Sense::click_and_drag());
    if native && hr.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if hr.double_clicked() {
        action = MiniAction::Expand;
    }
    hr.on_hover_text(t!("cardsui.mini_hint"));
    logo(ui, app, Rect::from_center_size(Pos2::new(head.left() + 25.0, head.center().y), Vec2::splat(22.0)));
    let mood = app.moods.moods.get(app.dial_mood).map(mood_rail::mood_label).unwrap_or_default();
    let title = format!("BigWave · {mood}");
    let g = cards::line_galley(ui, &title, FontId::proportional(15.0), Color32::WHITE, head.width() - 130.0);
    ui.painter().galley(Pos2::new(head.left() + 44.0, head.center().y - g.size().y / 2.0), g, Color32::WHITE);
    if cards::header_icon(ui, Pos2::new(head.right() - 22.0, head.center().y), Icon::Close, id.with("close"), t!("cardsui.close")) {
        action = MiniAction::Close;
    }
    if cards::header_icon(ui, Pos2::new(head.right() - 56.0, head.center().y), Icon::Expand, id.with("expand"), t!("cardsui.expand")) {
        action = MiniAction::Expand;
    }

    // Body: spectrum lines, readout, meta, now line, stars.
    let lines = Rect::from_min_max(Pos2::new(body.left(), body.top() + 10.0), Pos2::new(body.right(), body.top() + 110.0));
    let levels: Vec<f32> = if app.is_playing { app.bands.to_vec() } else { vec![0.12; 8] };
    cards::spectrum_lines(ui.painter(), lines, pal.primary, &levels, 6.0, false);
    let current = app.current.clone();
    let x = body.left() + 18.0;
    let stars_w = 86.0;
    let name = current.as_ref().map(|s| s.name.trim().to_string()).unwrap_or_else(|| "BigWave Radio".to_string());
    let ng = player::fit_readout(ui, &name, 30.0, 18.0, body.width() - 36.0 - stars_w, pal.text);
    let mut y = body.top() + 14.0;
    ui.painter().galley(Pos2::new(x, y), ng.clone(), pal.text);
    y += ng.size().y + 4.0;
    let meta = current.as_ref().map(cards::meta_line).unwrap_or_default();
    let mg = cards::line_galley(ui, &meta, FontId::proportional(12.5), pal.sub, body.width() - 36.0);
    ui.painter().galley(Pos2::new(x, y), mg, pal.sub);
    y += 20.0;
    let (now, col) = match now_text(app) {
        Some(s) => (s, pal.now),
        None if current.is_none() => (t!("player.pick").to_string(), pal.weak),
        None => (app.status.clone(), pal.weak),
    };
    let wg = cards::line_galley(ui, &now, FontId::proportional(12.5), col, body.width() - 36.0);
    ui.painter().galley(Pos2::new(x, y), wg, col);
    if let Some(st) = &current {
        let r = Rect::from_min_size(Pos2::new(body.right() - 18.0 - stars_w, body.top() + 18.0), Vec2::new(stars_w, 18.0));
        if let Some(n) = cards::rating_input(ui, &pal, r, app.ratings.get(&st.uuid), id.with("stars")) {
            cards::set_rating(app, st, n);
        }
    }

    // Controls: transport + volume, heart at the end.
    let row = Rect::from_min_max(Pos2::new(x, body.bottom() - 52.0), Pos2::new(body.right() - 50.0, body.bottom() - 8.0));
    player::transport(ui, app, &pal, row, id.with("t"), false);
    if let Some(st) = &current {
        let hc = Pos2::new(body.right() - 26.0, row.center().y);
        let saved = app.library.contains(&st.uuid);
        let hr = ui.interact(Rect::from_center_size(hc, Vec2::splat(30.0)), id.with("heart"), Sense::click());
        let ic = if saved { Icon::HeartFilled } else { Icon::Heart };
        cards::icon(ui.painter(), ic, hc, 20.0, if saved { pal.primary } else { pal.sub });
        let tip = if saved { t!("player.remove_library") } else { t!("player.save_library") };
        if hr.on_hover_text(tip).clicked() {
            app.library.toggle(st);
        }
    }
    action
}

/// Show the mini player when it's open. Call once per frame from the root.
pub fn show(ctx: &egui::Context, app: &mut BigWaveApp) {
    if !app.mini_open {
        return;
    }
    let vid = ViewportId::from_hash_of("bwr-mini");
    let builder = egui::ViewportBuilder::default()
        .with_title("BigWave Radio")
        .with_inner_size(MINI_SIZE)
        .with_min_inner_size(MINI_SIZE)
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(false)
        .with_always_on_top();
    let action = ctx.show_viewport_immediate(vid, builder, |ctx, class| {
        if class == egui::ViewportClass::Embedded {
            // No native windows here: float it inside the main window.
            let mut action = MiniAction::None;
            egui::Window::new("BigWave Radio")
                .id(egui::Id::new("bwr-mini-embedded"))
                .title_bar(false)
                .resizable(false)
                .frame(egui::Frame::NONE)
                .anchor(Align2::RIGHT_BOTTOM, [-24.0, -24.0])
                .fixed_size(MINI_SIZE)
                .show(ctx, |ui| {
                    let (r, _) = ui.allocate_exact_size(MINI_SIZE.into(), Sense::hover());
                    ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
                        action = contents(ui, app, false);
                    });
                });
            return action;
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            return MiniAction::Close;
        }
        let pal = Pal::new(ctx, app);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(pal.bg))
            .show(ctx, |ui| contents(ui, app, true))
            .inner
    });
    match action {
        MiniAction::None => {}
        MiniAction::Close => app.mini_open = false,
        MiniAction::Expand => {
            app.mini_open = false;
            if app.page != Page::Player {
                side_nav::go(app, Page::Player);
            }
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
        }
    }
}

