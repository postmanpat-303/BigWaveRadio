//! Left-rail discovery block: big mood chips (from moods.json), Your Stars,
//! the Show talk toggle and an Advanced section (format + bitrate).

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};

use crate::app::{BigWaveApp, Page};
use crate::theme;

const CHIP_H: f32 = 38.0;
const GAP: f32 = 6.0;

/// Height the rail block needs, so the nav above can scroll instead of
/// pushing it off-screen.
pub fn reserved_height(app: &BigWaveApp) -> f32 {
    let rows = app.moods.moods.len().div_ceil(2) as f32 + 1.0; // + Your Stars
    let chips = 18.0 + rows * (CHIP_H + GAP);
    let talk = 30.0;
    let adv = if app.advanced_open { 26.0 + 110.0 } else { 26.0 };
    chips + talk + adv + 16.0
}

/// A mood's label in the UI language. The built-in moods are translated
/// while their label is still the default; labels edited in moods.json
/// (or moods the user added) are shown exactly as written.
pub fn mood_label(mood: &bwr_core::Mood) -> String {
    let translated = match (mood.id.as_str(), mood.label.as_str()) {
        ("dance", "Dance") => t!("mood.dance"),
        ("trance", "Trance") => t!("mood.trance"),
        ("chill", "Chill") => t!("mood.chill"),
        ("easy", "Easy") => t!("mood.easy"),
        ("80s", "80s") => t!("mood.80s"),
        ("90s", "90s") => t!("mood.90s"),
        _ => return mood.label.clone(),
    };
    translated.to_string()
}

pub fn rail(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.label(egui::RichText::new(t!("rail.moods")).small().weak());
    ui.add_space(4.0);
    let w = ui.available_width();
    let half = (w - GAP) / 2.0;

    let moods = app.moods.moods.clone();
    for (row, pair) in moods.chunks(2).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for (j, mood) in pair.iter().enumerate() {
                let idx = row * 2 + j;
                let selected = app.page == Page::Mood(idx);
                let name = mood_label(mood);
                let label = format!("{} {}", mood.icon, name);
                let hover = format!("{}: #{}", name, mood.tags.join(" #"));
                if mood_chip(ui, app, &label, None, half, selected, idx).on_hover_text(hover).clicked() {
                    app.open_mood(idx);
                }
            }
        });
        ui.add_space(GAP);
    }
    let rated: usize = (1..=5).map(|r| app.ratings.count(r)).sum();
    let selected = app.page == Page::Stars;
    let count = (rated > 0).then(|| rated.to_string());
    if mood_chip(ui, app, &format!("★ {}", t!("stars.title")), count.as_deref(), w, selected, 99)
        .on_hover_text(t!("rail.stars_tip"))
        .clicked()
    {
        app.page = Page::Stars;
        app.browse_selection = None;
        app.stations.clear();
    }
    ui.add_space(8.0);

    let mut show = app.discovery.show_talk;
    if ui
        .checkbox(&mut show, t!("rail.show_talk"))
        .on_hover_text(t!("rail.show_talk_tip"))
        .changed()
    {
        app.discovery.show_talk = show;
        app.discovery.save();
    }
    ui.add_space(4.0);

    let arrow = if app.advanced_open { "−" } else { "+" };
    if ui
        .add(egui::Label::new(egui::RichText::new(format!("{arrow} {}", t!("rail.advanced"))).small().weak()).sense(Sense::click()))
        .on_hover_text(t!("rail.advanced_tip"))
        .clicked()
    {
        app.advanced_open = !app.advanced_open;
    }
    if app.advanced_open {
        advanced(ui, app);
    }
}

fn advanced(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(t!("rail.format")).small().weak());
    egui::ComboBox::from_id_salt("nav-codec")
        .width(ui.available_width() - 4.0)
        .selected_text(if app.settings.codec.is_empty() {
            t!("filter.any_format").to_string()
        } else {
            app.settings.codec.to_uppercase()
        })
        .show_ui(ui, |ui| {
            for c in ["", "mp3", "aac", "ogg", "flac", "opus"] {
                let label = if c.is_empty() { t!("filter.any_format").to_string() } else { c.to_uppercase() };
                if ui.selectable_label(app.settings.codec == c, label).clicked() {
                    app.settings.codec = c.to_string();
                    app.persist_settings();
                    app.reload_current_list();
                }
            }
        });
    ui.add_space(4.0);
    ui.label(egui::RichText::new(t!("rail.min_bitrate")).small().weak());
    egui::ComboBox::from_id_salt("nav-bitrate")
        .width(ui.available_width() - 4.0)
        .selected_text(match app.settings.min_bitrate {
            0 => t!("filter.any_bitrate").to_string(),
            b => format!("{b} kbps+"),
        })
        .show_ui(ui, |ui| {
            for b in [0u32, 64, 128, 192, 256, 320] {
                let label = if b == 0 { t!("filter.any_bitrate").to_string() } else { t!("filter.kbps_or_more", kbps = b) };
                if ui.selectable_label(app.settings.min_bitrate == b, label).clicked() {
                    app.settings.min_bitrate = b;
                    app.persist_settings();
                    app.reload_current_list();
                }
            }
        });
}

/// A chunky rounded chip with a per-mood tint; filled with the accent when
/// its page is open.
fn mood_chip(
    ui: &mut Ui,
    app: &BigWaveApp,
    label: &str,
    badge: Option<&str>,
    width: f32,
    selected: bool,
    salt: usize,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, CHIP_H), Sense::click());
    let accent = theme::accent_color(app.settings.accent);
    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    // Each mood gets its own hue so the chips read at a glance.
    let hues = [
        Color32::from_rgb(0xff, 0x3c, 0xac), // dance
        Color32::from_rgb(0x7b, 0x2f, 0xf7), // trance
        Color32::from_rgb(0x1f, 0xb5, 0xc9), // chill
        Color32::from_rgb(0xe0, 0xa1, 0x2a), // easy
        Color32::from_rgb(0xff, 0x6a, 0x3d), // 80s
        Color32::from_rgb(0x3a, 0xa0, 0xff), // 90s
    ];
    let tint = if salt == 99 {
        Color32::from_rgb(0xff, 0xc8, 0x2e)
    } else {
        hues[salt % hues.len()]
    };
    let painter = ui.painter();
    let fill = if selected {
        accent.gamma_multiply(0.85)
    } else if response.hovered() {
        tint.gamma_multiply(0.30)
    } else {
        tint.gamma_multiply(if dark { 0.16 } else { 0.20 })
    };
    painter.rect(
        rect,
        CornerRadius::same(10),
        fill,
        Stroke::new(1.0_f32, if selected { accent } else { tint.gamma_multiply(0.7) }),
        egui::StrokeKind::Inside,
    );
    let text_col = if selected {
        Color32::WHITE
    } else if dark {
        Color32::from_gray(235)
    } else {
        Color32::from_gray(25)
    };
    painter.text(
        Pos2::new(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.5),
        text_col,
    );
    if let Some(b) = badge {
        let r = Rect::from_center_size(Pos2::new(rect.right() - 20.0, rect.center().y), Vec2::new(28.0, 18.0));
        painter.rect_filled(r, CornerRadius::same(9), Color32::from_black_alpha(70));
        painter.text(r.center(), Align2::CENTER_CENTER, b, FontId::proportional(11.0), Color32::WHITE);
    }
    response
}
