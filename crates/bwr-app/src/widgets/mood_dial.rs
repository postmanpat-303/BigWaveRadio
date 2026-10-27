//! The Mood dial: a painted tuning scale (ticks, track, red needle and a
//! white knob). Dragging moves the needle freely and snaps to the nearest
//! mood on release; clicking a label jumps straight to it.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::widgets::cards::{Pal, NEEDLE};

/// Height the dial needs (caption, ticks, track and labels).
pub const HEIGHT: f32 = 104.0;
const TICKS: usize = 61;

/// The dial positions: the six built-in moods in the dial's order, then
/// any extra moods from moods.json. Talk is never a position. Returns
/// indices into `moods`.
pub fn slots(moods: &[bwr_core::Mood]) -> Vec<usize> {
    const ORDER: [&str; 6] = ["dance", "chill", "trance", "easy", "80s", "90s"];
    let is_talk = |m: &bwr_core::Mood| m.id.eq_ignore_ascii_case("talk");
    let mut out: Vec<usize> = ORDER
        .iter()
        .filter_map(|id| moods.iter().position(|m| m.id.eq_ignore_ascii_case(id)))
        .collect();
    for (i, m) in moods.iter().enumerate() {
        if !out.contains(&i) && !is_talk(m) {
            out.push(i);
        }
    }
    out
}

/// Paint the dial in `rect`; `selected` is a slot index. Returns the slot
/// the user settled on (drag released or label clicked).
pub fn dial(ui: &mut Ui, pal: &Pal, rect: Rect, labels: &[String], selected: usize) -> Option<usize> {
    let n = labels.len().max(1);
    let id = ui.id().with("mood-dial");
    let col_w = rect.width() / n as f32;
    let centre_x = |slot: usize| rect.left() + col_w * (slot as f32 + 0.5);

    let caption_y = rect.top() + 8.0;
    let ticks_top = rect.top() + 26.0;
    let ticks_bottom = rect.top() + 46.0;
    let track_y = rect.top() + 66.0;
    let label_y = rect.top() + 92.0;

    let hit = Rect::from_min_max(Pos2::new(rect.left(), ticks_top - 4.0), Pos2::new(rect.right(), track_y + 12.0));
    let resp = ui.interact(hit, id, Sense::click_and_drag());
    let mut result = None;

    let drag_x: Option<f32> = if resp.dragged() || resp.is_pointer_button_down_on() {
        resp.interact_pointer_pos().map(|p| p.x.clamp(centre_x(0), centre_x(n - 1)))
    } else {
        None
    };
    let snap = |x: f32| (((x - rect.left()) / col_w - 0.5).round().clamp(0.0, (n - 1) as f32)) as usize;
    if resp.drag_stopped() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let s = snap(p.x);
            if s != selected {
                result = Some(s);
            }
        }
    }
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    let target = result.unwrap_or(selected);
    let animated = ui.ctx().animate_value_with_time(id.with("x"), centre_x(target), 0.18);
    let needle_x = drag_x.unwrap_or(animated);

    let p = ui.painter();
    // Caption: "MOOD DIAL" left, hint right.
    p.text(
        Pos2::new(rect.left(), caption_y),
        Align2::LEFT_CENTER,
        t!("dial.caption"),
        FontId::proportional(11.0),
        pal.weak,
    );
    p.text(
        Pos2::new(rect.right(), caption_y),
        Align2::RIGHT_CENTER,
        t!("dial.hint"),
        FontId::proportional(11.0),
        pal.weak,
    );

    // Ticks: 61 across, every 10th long and in the primary colour.
    let tick_col = if pal.dark { Color32::from_rgb(0xc9, 0xcf, 0xd5) } else { pal.sub };
    for i in 0..TICKS {
        let x = rect.left() + rect.width() * i as f32 / (TICKS - 1) as f32;
        let major = i % 10 == 0;
        let top = if major { ticks_top - 2.0 } else { ticks_top + 6.0 };
        let (w, c) = if major { (1.6_f32, pal.primary) } else { (1.0_f32, tick_col.gamma_multiply(0.8)) };
        p.line_segment([Pos2::new(x, top), Pos2::new(x, ticks_bottom)], Stroke::new(w, c));
    }
    // Track.
    let track_col = if pal.dark { Color32::from_rgb(0x5c, 0x63, 0x6a) } else { pal.pill_border };
    p.line_segment([Pos2::new(rect.left(), track_y), Pos2::new(rect.right(), track_y)], Stroke::new(1.2_f32, track_col));
    // Needle + knob.
    p.line_segment([Pos2::new(needle_x, ticks_top - 4.0), Pos2::new(needle_x, track_y)], Stroke::new(2.0_f32, NEEDLE));
    p.circle_filled(Pos2::new(needle_x, track_y + 1.5), 9.5, Color32::from_black_alpha(60));
    p.circle_filled(Pos2::new(needle_x, track_y), 9.0, Color32::WHITE);
    if !pal.dark {
        p.circle_stroke(Pos2::new(needle_x, track_y), 9.0, Stroke::new(1.0_f32, pal.pill_border));
    }

    // Labels (clickable).
    let live = drag_x.map(snap).unwrap_or(target);
    for (i, label) in labels.iter().enumerate() {
        let c = Pos2::new(centre_x(i), label_y);
        let lr = Rect::from_center_size(c, Vec2::new(col_w, 24.0));
        let lresp = ui.interact(lr, id.with(("label", i)), Sense::click());
        let on = i == live;
        let font = FontId::proportional(if on { 14.5 } else { 14.0 });
        let color = if on {
            pal.now
        } else if lresp.hovered() {
            pal.text
        } else {
            pal.sub
        };
        let g = crate::widgets::cards::line_galley(ui, label, font, color, col_w - 4.0);
        let pos = Pos2::new(c.x - g.size().x / 2.0, c.y - g.size().y / 2.0);
        ui.painter().galley(pos, g.clone(), color);
        if on {
            // Faux bold: a second pass offset by half a pixel.
            ui.painter().galley(pos + Vec2::new(0.5, 0.0), g, color);
        }
        if lresp.clicked() && i != selected {
            result = Some(i);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dial_order_matches_the_design_and_skips_talk() {
        let cfg = bwr_core::MoodConfig::default();
        let ids: Vec<&str> = slots(&cfg.moods).iter().map(|&i| cfg.moods[i].id.as_str()).collect();
        assert_eq!(ids, ["dance", "chill", "trance", "easy", "80s", "90s"]);
        let mut moods = cfg.moods.clone();
        let mut talk = moods[0].clone();
        talk.id = "talk".into();
        moods.push(talk);
        assert_eq!(slots(&moods).len(), 6);
    }
}
