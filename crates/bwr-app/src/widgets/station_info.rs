//! Right-hand panel showing details about the current station.

use eframe::egui::{self, Ui};

use crate::app::BigWaveApp;

pub fn panel(ui: &mut Ui, app: &mut BigWaveApp) {
    let Some(station) = app.current.clone() else {
        return;
    };

    ui.add_space(8.0);
    ui.label(egui::RichText::new(t!("info.now_playing")).small().weak());
    ui.add_space(6.0);

    // Station artwork.
    if let Some(tex) = app.favicons.get(&station.uuid) {
        let size = ui.available_width().min(160.0);
        let tex = tex.clone();
        ui.add(egui::Image::new((tex.id(), egui::vec2(size, size))).corner_radius(8));
        ui.add_space(8.0);
    }

    ui.label(egui::RichText::new(crate::rtl::visual(&station.name)).strong().size(17.0));
    if !station.country.is_empty() {
        ui.label(egui::RichText::new(format!("📍 {}", crate::rtl::visual(&station.country))).size(13.0));
    }
    if !station.language.is_empty() {
        ui.label(egui::RichText::new(format!("🗣 {}", crate::rtl::visual(&station.language))).size(13.0));
    }

    // Your rating — click a star to set it, click the current level again to clear.
    ui.add_space(4.0);
    let rating = app.ratings.get(&station.uuid);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t!("info.your_rating")).small().weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut new_rating: Option<u8> = None;
            // right_to_left places the FIRST widget at the right edge, so draw
            // 5 → 1 to keep the conventional ★★★★☆ left-to-right order.
            for star in (1..=5u8).rev() {
                let filled = rating >= star;
                let color = if filled {
                    crate::theme::accent_color(app.settings.accent)
                } else {
                    ui.visuals().weak_text_color()
                };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(if filled { "★" } else { "☆" })
                                .size(15.0)
                                .color(color),
                        )
                        .frame(false),
                    )
                    .on_hover_text(tn!("info.rate", star))
                    .clicked()
                {
                    new_rating = Some(if rating == star { 0 } else { star });
                }
            }
            if let Some(r) = new_rating {
                app.ratings.set(&station.uuid, r);
                if r > 0 {
                    // Cache the record so the Rated list can show it offline.
                    app.ratings.record(&station);
                }
            }
        });
    });

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    // Stream properties.
    property(ui, t!("info.format"), &station.codec.to_uppercase());
    if station.bitrate > 0 {
        property(ui, t!("info.bitrate"), &format!("{} kbps", station.bitrate));
    }
    property(ui, t!("info.votes"), &thousands(station.votes));
    property(ui, t!("info.clicks"), &thousands(station.click_count));

    // Tags as clickable chips: each one fills the app with similar stations.
    let tag_list: Vec<String> = station
        .tags
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .take(10)
        .collect();
    if !tag_list.is_empty() {
        ui.label(egui::RichText::new(t!("info.tags")).weak());
        ui.add_space(2.0);
        let mut picked: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            for tag in &tag_list {
                if ui
                    .add(egui::Button::new(crate::rtl::visual_string(format!("#{tag}"))).small())
                    .on_hover_text(t!("info.tag_tip", tag = tag))
                    .clicked()
                {
                    picked = Some(tag.clone());
                }
            }
        });
        if let Some(tag) = picked {
            let from = station.name.clone();
            app.more_like_this(&tag, &from);
        }
        ui.add_space(6.0);
    }
    ui.add_space(6.0);

    if ui
        .button(egui::RichText::new(format!("✨  {}", t!("info.more_like"))).size(14.0))
        .on_hover_text(t!("info.more_like_tip"))
        .clicked()
    {
        app.more_like_station(&station);
    }
    ui.add_space(4.0);

    // Links.
    if !station.homepage.trim().is_empty() {
        ui.hyperlink_to(format!("🌐 {}", t!("info.website")), &station.homepage);
        ui.add_space(2.0);
    }
    if ui.button(format!("📋 {}", t!("info.copy_url"))).clicked() {
        ui.ctx().copy_text(station.stream_url.clone());
    }
    // Surprise me.
    if ui
        .button(egui::RichText::new(format!("🎲  {}", t!("info.surprise"))).size(14.0))
        .on_hover_text(t!("info.surprise_tip"))
        .clicked()
    {
        app.surprise();
    }

    ui.add_space(10.0);
}

fn property(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(egui::RichText::new(label).weak()).selectable(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(value).strong()).selectable(false));
        });
    });
}

/// 307150 -> "307,150" (the separator follows the UI language: "307.150").
fn thousands(n: u32) -> String {
    let sep = t!("format.thousands_sep");
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push_str(sep);
        }
        out.push(c);
    }
    out
}
