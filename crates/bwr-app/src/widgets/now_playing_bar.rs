//! The bottom transport bar: station info (left), a centred transport
//! cluster with explicit labels (middle), volume + save (right).
//!
//! The transport buttons are custom-painted (same family as the tab pills
//! and chips) so their outlines and states are fully controlled: the primary
//! play/pause is a large solid accent circle; secondary buttons are outlined
//! with small captions so it's always obvious what each one does.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::app::BigWaveApp;
use crate::theme;

pub fn bar(ui: &mut Ui, app: &mut BigWaveApp) {
    let bar = ui.max_rect();
    let painter = ui.painter().clone();

    // Chrome fill first so everything else paints on top of it.
    painter.rect_filled(
        bar,
        0.0,
        theme::chrome_fill(ui.ctx(), app.settings.theme),
    );

    // Separator across the top of the bar.
    painter.hline(
        bar.x_range(),
        bar.top(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );

    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    let accent = theme::accent_color(app.settings.accent);
    let strong = if dark { Color32::from_gray(240) } else { Color32::from_gray(20) };
    let weak = ui.visuals().weak_text_color();

    // Transport cluster geometry (needed up front to clip the info text and
    // to paint the cluster backdrop before the buttons).
    let btn_h = 40.0;
    let y = bar.top() + 10.0;
    let cluster_w = 44.0 + 8.0 + 44.0 + 8.0 + 60.0 + 8.0 + 44.0 + 8.0 + 44.0; // 268
    let start = bar.center().x - cluster_w / 2.0;

    // Opaque backdrop behind the whole cluster so the (clipped) station text
    // can never mix with the buttons; reads as a soft "faded" plate.
    let backdrop = Rect::from_min_size(
        Pos2::new(start - 12.0, bar.top() + 2.0),
        Vec2::new(cluster_w + 24.0, bar.height() - 4.0),
    );

    // --- Left: station + status (painted; no interaction needed) -------------
    // Clipped to the area left of the cluster so long names never run under
    // the buttons.
    let clip_right = (start - 14.0).max(bar.left() + 150.0);
    let left_painter = painter.with_clip_rect(Rect::from_min_max(
        Pos2::new(bar.left(), bar.top()),
        Pos2::new(clip_right, bar.bottom()),
    ));
    let name = app
        .current
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Pick a station to start listening".to_string());
    let status: String = app.status.chars().take(80).collect();
    left_painter.text(
        Pos2::new(bar.left() + 14.0, bar.top() + 12.0),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(15.0),
        strong,
    );
    // ICY song title (accent) takes the line under the name when available;
    // the status line slides down to make room.
    let (line2, line3) = if let Some(title) = &app.song_title {
        let song: String = format!("♪  {}", title).chars().take(80).collect();
        (Some(song), 56.0)
    } else {
        (None, 36.0)
    };
    if let Some(song) = line2 {
        left_painter.text(
            Pos2::new(bar.left() + 14.0, bar.top() + 36.0),
            Align2::LEFT_TOP,
            song,
            FontId::proportional(12.0),
            accent,
        );
    }
    left_painter.text(
        Pos2::new(bar.left() + 14.0, bar.top() + line3),
        Align2::LEFT_TOP,
        status,
        FontId::proportional(12.0),
        weak,
    );

    // --- Right: save star + volume (interactive widgets, right-aligned) ------
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add_space(12.0);
        let saved = app
            .current
            .as_ref()
            .is_some_and(|c| app.library.contains(&c.uuid));
        // Frameless: only the star itself lights up in the accent colour —
        // no filled box around it.
        let star_color = if saved {
            accent
        } else {
            ui.visuals().weak_text_color()
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(if saved { "★" } else { "☆" })
                        .size(17.0)
                        .color(star_color),
                )
                .frame(false)
                .min_size(Vec2::new(42.0, 40.0)),
            )
            .on_hover_text(if saved {
                "Remove from My Library"
            } else {
                "Save to My Library"
            })
            .clicked()
        {
            if let Some(station) = app.current.clone() {
                app.library.toggle(&station);
            }
        }
        ui.add_space(10.0);
        let mut vol = app.player.volume();
        if ui
            .add_sized(
                [130.0, 0.0],
                egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false),
            )
            .changed()
        {
            app.player.set_volume(vol);
            app.settings.volume = vol;
        }
    });

    // Cluster backdrop (after the right-side widgets, before the buttons).
    painter.rect(
        backdrop,
        CornerRadius::same(12),
        ui.visuals().window_fill(),
        Stroke::NONE,
        egui::StrokeKind::Inside,
    );

    // --- Centre: transport cluster -------------------------------------------
    // Visual order: DICE | PREV | PLAY/PAUSE | STOP | NEXT, symmetric around centre.
    let mut x = start;
    let dice_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(44.0, btn_h));
    x += 44.0 + 8.0;
    let prev_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(44.0, btn_h));
    x += 44.0 + 8.0;
    let play_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(60.0, btn_h));
    x += 60.0 + 8.0;
    let stop_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(44.0, btn_h));
    x += 44.0 + 8.0;
    let next_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(44.0, btn_h));

    let has_station = app.current.is_some();

    if transport_btn(
        ui,
        dice_rect,
        "🎲",
        "SURPRISE",
        accent,
        dark,
        true,
        false,
    ) {
        app.surprise();
    }

    if transport_btn(
        ui,
        prev_rect,
        "⏮",
        "PREV",
        accent,
        dark,
        app.can_go_previous(),
        false,
    ) {
        app.previous_station();
    }
    if transport_btn(
        ui,
        stop_rect,
        "⏹",
        "STOP",
        accent,
        dark,
        has_station,
        false,
    ) {
        app.stop();
    }
    let play_label = if app.is_playing { "PAUSE" } else { "PLAY" };
    let play_glyph = if app.is_playing { "⏸" } else { "▶" };
    if play_btn(ui, play_rect, play_glyph, play_label, accent, dark, has_station) {
        app.toggle_play_pause();
    }
    if transport_btn(
        ui,
        next_rect,
        "⏭",
        "NEXT",
        accent,
        dark,
        app.can_go_next(),
        false,
    ) {
        app.next_station();
    }
}

/// Outlined secondary transport button with an explicit caption underneath.
fn transport_btn(
    ui: &mut Ui,
    rect: Rect,
    glyph: &str,
    caption: &str,
    accent: Color32,
    dark: bool,
    enabled: bool,
    _selected: bool,
) -> bool {
    let id = ui.id().with(caption);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, id, sense);

    let base_fill = if dark {
        Color32::from_gray(40)
    } else {
        Color32::from_gray(250)
    };
    let (fill, stroke_color, glyph_color) = if !enabled {
        (
            base_fill.gamma_multiply(0.6),
            accent.gamma_multiply(0.25),
            ui.visuals().weak_text_color(),
        )
    } else if response.is_pointer_button_down_on() {
        (accent.gamma_multiply(0.45), accent, Color32::WHITE)
    } else if response.hovered() {
        (accent.gamma_multiply(0.16), accent, if dark { Color32::from_gray(245) } else { Color32::from_gray(15) })
    } else {
        (
            base_fill,
            accent.gamma_multiply(0.6),
            if dark { Color32::from_gray(230) } else { Color32::from_gray(35) },
        )
    };

    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(10),
        fill,
        Stroke::new(1.0_f32, stroke_color),
        egui::StrokeKind::Outside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(16.0),
        glyph_color,
    );
    // Caption under the button.
    painter.text(
        Pos2::new(rect.center().x, rect.bottom() + 9.0),
        Align2::CENTER_CENTER,
        caption,
        FontId::proportional(9.5),
        if enabled { ui.visuals().weak_text_color() } else { ui.visuals().weak_text_color().gamma_multiply(0.6) },
    );

    response.clicked()
}

/// Large primary play/pause button: solid accent pill with a white glyph.
fn play_btn(
    ui: &mut Ui,
    rect: Rect,
    glyph: &str,
    caption: &str,
    accent: Color32,
    dark: bool,
    enabled: bool,
) -> bool {
    let id = ui.id().with("play-pause");
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, id, sense);

    let (fill, glyph_color) = if !enabled {
        (accent.gamma_multiply(0.35), Color32::from_gray(220))
    } else if response.is_pointer_button_down_on() {
        (accent.gamma_multiply(0.8), Color32::WHITE)
    } else if response.hovered() {
        (accent.gamma_multiply(1.12), Color32::WHITE)
    } else {
        (accent, Color32::WHITE)
    };

    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(20),
        fill,
        Stroke::new(1.0_f32, accent.gamma_multiply(0.9)),
        egui::StrokeKind::Outside,
    );
    // Nudge the glyph slightly right so the triangle reads centred.
    let nudge = if glyph == "▶" { 2.0 } else { 0.0 };
    painter.text(
        Pos2::new(rect.center().x + nudge, rect.center().y),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(19.0),
        glyph_color,
    );
    let _ = dark;
    painter.text(
        Pos2::new(rect.center().x, rect.bottom() + 9.0),
        Align2::CENTER_CENTER,
        caption,
        FontId::proportional(9.5),
        if enabled { ui.visuals().weak_text_color() } else { ui.visuals().weak_text_color().gamma_multiply(0.6) },
    );

    response.clicked()
}
