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
    if !station.tags.trim().is_empty() {
        let tags = station
            .tags
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .take(10)
            .collect::<Vec<_>>()
            .join(", ");
        ui.label(egui::RichText::new("Tags").weak());
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(egui::RichText::new(tags).strong())
                .wrap()
                .selectable(false),
        );
        ui.add_space(6.0);
    }
    ui.add_space(6.0);

    // Links.
    if !station.homepage.trim().is_empty() {
        ui.hyperlink_to("🌐 Station website", &station.homepage);
        ui.add_space(2.0);
    }
    if ui.button("📋 Copy stream URL").clicked() {
        ui.ctx().copy_text(station.stream_url.clone());
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
