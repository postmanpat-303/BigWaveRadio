//! Right-hand panel showing details about the current station.

use eframe::egui::{self, Ui};

use crate::app::BigWaveApp;

pub fn panel(ui: &mut Ui, app: &mut BigWaveApp) {
    let Some(station) = app.current.clone() else {
        return;
    };

    ui.add_space(8.0);
    ui.label(egui::RichText::new("Now playing").small().weak());
    ui.add_space(6.0);

    // Station artwork.
    if let Some(tex) = app.favicons.get(&station.uuid) {
        let size = ui.available_width().min(160.0);
        let tex = tex.clone();
        ui.add(egui::Image::new((tex.id(), egui::vec2(size, size))).corner_radius(8));
        ui.add_space(8.0);
    }

    ui.label(egui::RichText::new(&station.name).strong().size(17.0));
    if !station.country.is_empty() {
        ui.label(egui::RichText::new(format!("📍 {}", station.country)).size(13.0));
    }
    if !station.language.is_empty() {
        ui.label(egui::RichText::new(format!("🗣 {}", station.language)).size(13.0));
    }

    // Your rating — click a star to set it, click the current level again to clear.
    ui.add_space(4.0);
    let rating = app.ratings.get(&station.uuid);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("YOUR RATING").small().weak());
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
                    .on_hover_text(format!("Rate {star} star{}", if star > 1 { "s" } else { "" }))
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
    property(ui, "Format", &station.codec.to_uppercase());
    if station.bitrate > 0 {
        property(ui, "Bitrate", &format!("{} kbps", station.bitrate));
    }
    property(ui, "Votes", &thousands(station.votes));
    property(ui, "Clicks", &thousands(station.click_count));

    // Tags as clickable chips: each one fills the app with similar stations.
    let tag_list: Vec<String> = station
        .tags
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .take(10)
        .collect();
    if !tag_list.is_empty() {
        ui.label(egui::RichText::new("Tags").weak());
        ui.add_space(2.0);
        let mut picked: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            for tag in &tag_list {
                if ui
                    .add(egui::Button::new(format!("#{tag}")).small())
                    .on_hover_text(format!("More stations tagged #{tag}"))
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
        .button(egui::RichText::new("✨  More like this").size(14.0))
        .on_hover_text("Stations sharing this one's tags, tuned by your star ratings")
        .clicked()
    {
        app.more_like_station(&station);
    }
    ui.add_space(4.0);

    // Links.
    if !station.homepage.trim().is_empty() {
        ui.hyperlink_to("🌐 Station website", &station.homepage);
        ui.add_space(2.0);
    }
    if ui.button("📋 Copy stream URL").clicked() {
        ui.ctx().copy_text(station.stream_url.clone());
    }
    // Surprise me.
    if ui
        .button(egui::RichText::new("🎲  Surprise me").size(14.0))
        .on_hover_text("Play a well-rated or popular station from your moods")
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

/// 307150 -> "307,150"
fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
