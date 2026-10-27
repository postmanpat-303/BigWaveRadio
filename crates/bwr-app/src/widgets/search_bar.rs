//! Browse view's search bar (with recent searches and 🎲 Surprise me) and
//! the codec / minimum-bitrate filter pills beside it.

use std::time::Instant;

use eframe::egui::{self, Align2, CornerRadius, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::app::{BigWaveApp, Page};
use crate::widgets::cards::{self, Icon, Pal, PillStyle, PILL_H};

const CODECS: [&str; 6] = ["", "mp3", "aac", "ogg", "flac", "opus"];
const BITRATES: [u32; 6] = [0, 64, 128, 192, 256, 320];

/// The rounded search field filling `rect`.
pub fn bar(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, rect: Rect) {
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(12),
        pal.card,
        Stroke::new(1.0_f32, if pal.dark { pal.line.gamma_multiply(0.6) } else { pal.line }),
        egui::StrokeKind::Inside,
    );
    cards::icon(p, Icon::Search, Pos2::new(rect.left() + 22.0, rect.center().y), 17.0, pal.weak);

    // 🎲 at the right end, then a "Ctrl K" hint.
    let dice_c = Pos2::new(rect.right() - 24.0, rect.center().y);
    let dice = ui.interact(Rect::from_center_size(dice_c, Vec2::splat(32.0)), ui.id().with("bar-dice"), Sense::click());
    if dice.hovered() {
        ui.painter().circle_filled(dice_c, 16.0, pal.pill_border.gamma_multiply(0.35));
    }
    cards::icon(ui.painter(), Icon::Dice, dice_c, 18.0, pal.sub);
    if dice.on_hover_text(t!("top.surprise_tip")).clicked() {
        app.surprise();
    }
    let kbd = "Ctrl K";
    let kw = cards::text_width(ui, kbd, FontId::proportional(11.0)) + 12.0;
    let kr = Rect::from_center_size(Pos2::new(dice_c.x - 24.0 - kw / 2.0, rect.center().y), Vec2::new(kw, 20.0));
    ui.painter().rect_stroke(kr, CornerRadius::same(4), Stroke::new(1.0_f32, pal.pill_border), egui::StrokeKind::Inside);
    ui.painter().text(kr.center(), Align2::CENTER_CENTER, kbd, FontId::proportional(11.0), pal.weak);

    let field = Rect::from_min_max(Pos2::new(rect.left() + 40.0, rect.top() + 6.0), Pos2::new(kr.left() - 10.0, rect.bottom() - 6.0));
    let edit_id = ui.id().with("browse-search");
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let response = child.add(
        egui::TextEdit::singleline(&mut app.query)
            .id(edit_id)
            .frame(false)
            .hint_text(t!("top.search_hint"))
            .font(FontId::proportional(15.0))
            .text_color(pal.text)
            .desired_width(field.width()),
    );
    // Ctrl/⌘+K focuses the field (never while a plugin overlay owns the keyboard).
    if app.plugins.host.overlay.is_none() && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::K)) {
        response.request_focus();
    }
    if response.changed() {
        // Typing continues on the Search page (its field takes focus).
        app.page = Page::Search;
        app.last_keystroke = Some(Instant::now());
    }
    history_popup(ui, app, &response);
}

/// Recent searches drop-down under the field.
fn history_popup(ui: &mut Ui, app: &mut BigWaveApp, response: &egui::Response) {
    let popup_id = ui.make_persistent_id("search-history");
    // Focus opens the popup; it then stays open until a pick, typing or a
    // click outside (see search_history.rs).
    let was_open = ui.memory(|mem| mem.is_popup_open(popup_id));
    let show_history = crate::search_history::popup_open(
        was_open,
        response.has_focus(),
        app.query.is_empty(),
        !app.settings.search_history.is_empty(),
    );
    ui.memory_mut(|mem| {
        if show_history {
            mem.open_popup(popup_id);
        } else if mem.is_popup_open(popup_id) {
            mem.close_popup();
        }
    });
    let history = app.settings.search_history.clone();
    egui::popup::popup_below_widget(ui, popup_id, response, egui::popup::PopupCloseBehavior::CloseOnClickOutside, |ui| {
        ui.set_min_width(260.0);
        ui.label(egui::RichText::new(t!("top.recent")).small().weak());
        for q in history {
            let mut remove = false;
            let picked = ui
                .horizontal(|ui| {
                    let picked = ui.selectable_label(false, format!("🕘  {q}")).clicked();
                    if ui
                        .add(egui::Button::new("🗙").small().frame(false))
                        .on_hover_text(t!("top.remove"))
                        .clicked()
                    {
                        remove = true;
                    }
                    picked
                })
                .inner;
            if remove {
                app.settings.search_history.retain(|h| !h.eq_ignore_ascii_case(&q));
                app.settings.save();
            }
            if picked {
                let pick = crate::search_history::pick(&q, &mut app.settings.search_history, bwr_core::settings::MAX_SEARCH_HISTORY);
                app.settings.save();
                app.run_search_now(pick.query);
                ui.memory_mut(|m| m.close_popup());
            }
        }
        if ui.add(egui::Button::new(t!("top.clear_history")).small()).clicked() {
            app.settings.search_history.clear();
            app.settings.save();
            ui.memory_mut(|m| m.close_popup());
        }
    });
}

/// "Any codec ▾" and "≥ 128 kbps ▾" pills, right-aligned ending at
/// `right`, vertically centred on `cy`. Returns the left edge used.
pub fn filter_pills(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, right: f32, cy: f32) -> f32 {
    let codec_label = if app.settings.codec.is_empty() {
        t!("filter.any_format").to_string()
    } else {
        app.settings.codec.to_uppercase()
    };
    let br_label = match app.settings.min_bitrate {
        0 => t!("filter.any_bitrate").to_string(),
        b => t!("filter.kbps_or_more", kbps = b),
    };
    let mut x = right;
    let mut changed = false;
    for (which, label, tip) in [(1, br_label, t!("rail.min_bitrate")), (0, codec_label, t!("rail.format"))] {
        let label = format!("{label} ⏷");
        let w = cards::pill_width(ui, &label, None);
        let r = Rect::from_min_size(Pos2::new(x - w, cy - PILL_H / 2.0), Vec2::new(w, PILL_H + 2.0));
        x -= w + 10.0;
        let style = PillStyle { text: pal.text, ..PillStyle::normal(pal) };
        let resp = cards::pill_at(ui, r, ui.id().with(("filter-pill", which)), &label, None, style);
        let popup_id = ui.id().with(("filter-popup", which));
        if resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup_id));
        }
        let resp = resp.on_hover_text(tip);
        egui::popup::popup_below_widget(ui, popup_id, &resp, egui::popup::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(140.0);
            if which == 0 {
                for c in CODECS {
                    let l = if c.is_empty() { t!("filter.any_format").to_string() } else { c.to_uppercase() };
                    if ui.selectable_label(app.settings.codec == c, l).clicked() && app.settings.codec != c {
                        app.settings.codec = c.to_string();
                        changed = true;
                    }
                }
            } else {
                for b in BITRATES {
                    let l = if b == 0 { t!("filter.any_bitrate").to_string() } else { t!("filter.kbps_or_more", kbps = b) };
                    if ui.selectable_label(app.settings.min_bitrate == b, l).clicked() && app.settings.min_bitrate != b {
                        app.settings.min_bitrate = b;
                        changed = true;
                    }
                }
            }
        });
    }
    if changed {
        app.persist_settings();
        app.filters_changed();
    }
    x + 10.0
}
