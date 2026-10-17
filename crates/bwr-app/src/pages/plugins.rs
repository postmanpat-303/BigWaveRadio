//! Plugins page (manage, approve, configure, import) and plugin-owned pages.

use bwr_plugins::{Permission, PluginStatus, SettingKind, HARD_MAX_FLASH_HZ};
use eframe::egui::{self, Color32, RichText, Ui};

use crate::app::BigWaveApp;
use crate::plugin_bridge::render_widgets;

/// The community plugin hub.
pub const COMMUNITY_URL: &str = "https://github.com/postmanpat-303/BigWaveRadio-Plugins";

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("Plugins");
    let dir = app.plugins.host.dir.clone();
    ui.label(
        RichText::new(format!(
            "Plugins are folders with plugin.toml + main.rhai in {}. Drop a folder there or import a .zip \
             (you can also drag a .zip onto the window). Edits reload automatically.",
            dir.display()
        ))
        .size(13.0)
        .color(secondary_text(ui)),
    );
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        #[cfg(windows)]
        if action_button(ui, "📦", "Import .zip…", Btn::Primary).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Plugin archive", &["zip"]).pick_file() {
                app.plugins.import(&p);
            }
        }
        #[cfg(not(windows))]
        {
            ui.add(egui::TextEdit::singleline(&mut app.plugins.import_path).hint_text("path/to/plugin.zip").desired_width(220.0));
            if action_button(ui, "📦", "Import .zip", Btn::Primary).clicked() {
                let p = std::path::PathBuf::from(app.plugins.import_path.trim());
                app.plugins.import(&p);
            }
        }
        if action_button(ui, "📂", "Open folder", Btn::Secondary).clicked() {
            #[cfg(windows)]
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            #[cfg(not(windows))]
            let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
        }
        if action_button(ui, "⟳", "Reload all", Btn::Secondary).clicked() {
            app.plugins.host.scan();
        }
        ui.add_space(4.0);
        let browse = action_button(ui, "🌐", "Browse community plugins", Btn::Link)
            .on_hover_text("The community plugin hub on GitHub: download a plugin's .zip there, then import it here.");
        if browse.clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(COMMUNITY_URL));
        }
    });
    ui.add_space(12.0);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        flash_safety(ui, app);
        ui.add_space(8.0);
        let ids: Vec<String> = app.plugins.host.plugins.iter().map(|p| p.id()).collect();
        if ids.is_empty() {
            ui.label(RichText::new("No plugins installed yet.").weak());
        }
        for id in ids {
            plugin_card(ui, app, &id);
            ui.add_space(10.0);
        }
        ui.add_space(6.0);
        let hint = secondary_text(ui);
        ui.label(
            RichText::new("Writing a plugin (or asking an AI to)? See PLUGIN_GUIDE.md in the plugins folder, and test it with: BigWaveRadio.exe --check-plugin <folder>")
                .size(12.5)
                .color(hint),
        );
        ui.add_space(8.0);
    });
}

fn flash_safety(ui: &mut Ui, app: &mut BigWaveApp) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("FLASH SAFETY").small().strong());
        let host = &mut app.plugins.host;
        let mut allow = host.state.allow_fast_flash;
        let mut cap = host.flash_cap();
        ui.label(
            RichText::new(
                "The app limits how often any plugin can flash the screen. 3 per second is the accessibility guideline (WCAG).",
            )
            .small(),
        );
        let mut changed = ui.checkbox(&mut allow, "Allow faster flashing (up to 12 per second). I accept the seizure risk.").changed();
        let max = if allow { HARD_MAX_FLASH_HZ } else { bwr_plugins::DEFAULT_FLASH_HZ };
        changed |= ui.add(egui::Slider::new(&mut cap, 0.5..=max).suffix(" Hz").text("Flash cap")).changed();
        if changed {
            host.set_flash_safety(allow, cap);
        }
    });
}

fn status_badge(ui: &mut Ui, s: &PluginStatus) {
    let (text, color) = match s {
        PluginStatus::Running => ("ENABLED", Color32::from_rgb(70, 190, 110)),
        PluginStatus::Disabled => ("DISABLED", Color32::from_rgb(130, 130, 140)),
        PluginStatus::NeedsApproval => ("NEEDS APPROVAL", Color32::from_rgb(235, 175, 40)),
        PluginStatus::Error(_) => ("ERROR", Color32::from_rgb(230, 70, 70)),
    };
    egui::Frame::new()
        .fill(color.gamma_multiply(0.22))
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(7, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(10.5).strong());
        });
}

/// Card colours: a subtle tint of the plugin's colour over the panel.
fn card_colors(ui: &Ui, id: &str, accent: &str) -> (Color32, Color32, Color32) {
    let t = bwr_plugins::card_tint(id, accent);
    let tint = Color32::from_rgb(t.0[0], t.0[1], t.0[2]);
    let base = ui.visuals().panel_fill;
    let mix = |a: Color32, b: Color32, k: f32| {
        let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
        Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
    };
    let k = if ui.visuals().dark_mode { 0.10 } else { 0.14 };
    (mix(base, tint, k), mix(base, tint, 0.45), tint)
}

fn plugin_card(ui: &mut Ui, app: &mut BigWaveApp, id: &str) {
    let Some(p) = app.plugins.host.get(id) else { return };
    let status = p.status.clone();
    let manifest = p.manifest.clone();
    let log: Vec<String> = p.log.iter().rev().take(20).cloned().collect();
    let name = p.name();

    let accent = manifest.as_ref().map(|m| m.accent.clone()).unwrap_or_default();
    let (fill, border, tint) = card_colors(ui, id, &accent);
    let card = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0_f32, border))
        .corner_radius(CARD_RADIUS)
        .inner_margin(egui::Margin { left: 18, right: 12, top: 10, bottom: 10 })
        .outer_margin(egui::Margin::symmetric(2, 0))
        .show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 5.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new(&name).strong().size(16.0));
            if let Some(m) = &manifest {
                ui.label(RichText::new(format!("v{}", m.version)).small().monospace());
                if !m.author.is_empty() {
                    ui.label(RichText::new(format!("by {}", m.author)).small().weak());
                }
            }
            status_badge(ui, &status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if action_button(ui, "⟳", "Reload", Btn::Small).clicked() {
                    app.plugins.host.reload(id);
                }
                match &status {
                    PluginStatus::Running => {
                        if action_button(ui, "⏸", "Disable", Btn::Small).clicked() {
                            app.plugins.host.set_enabled(id, false);
                        }
                    }
                    PluginStatus::Disabled => {
                        if action_button(ui, "▶", "Enable", Btn::SmallPrimary).clicked() {
                            app.plugins.host.approve(id);
                        }
                    }
                    PluginStatus::NeedsApproval => {
                        if action_button(ui, "✔", "Allow & enable", Btn::SmallPrimary).clicked() {
                            app.plugins.host.approve(id);
                        }
                    }
                    PluginStatus::Error(_) => {}
                }
            });
        });
        ui.add_space(2.0);
        if let Some(m) = &manifest {
            if !m.description.is_empty() {
                ui.label(RichText::new(&m.description).size(14.0));
            }
            let perms: Vec<String> = m.permission_set().into_iter().map(|p: Permission| p.describe().to_string()).collect();
            if !perms.is_empty() {
                let prefix = if status == PluginStatus::NeedsApproval { "Wants to: " } else { "Can: " };
                ui.label(RichText::new(format!("{prefix}{}", perms.join(", "))).size(12.5).color(secondary_text(ui)));
            }
        }
        if let PluginStatus::Error(e) = &status {
            ui.label(RichText::new(e).color(Color32::from_rgb(240, 100, 100)).monospace().small());
        }

        if status == PluginStatus::Running {
            if let Some(m) = manifest.as_ref().filter(|m| !m.settings.is_empty()) {
                egui::CollapsingHeader::new("Settings").id_salt(("plugin-settings", id)).show(ui, |ui| {
                    for (key, spec) in &m.settings {
                        setting_row(ui, app, id, key, spec);
                    }
                });
            }
            if m_has(&manifest, Permission::Panel) {
                ui.separator();
                let widgets = app.plugins.host.panel(id);
                let results = render_widgets(ui, &format!("panel-{id}"), &widgets);
                app.plugins.host.set_ui_results(id, "panel", results);
            }
        }
        if !log.is_empty() {
            egui::CollapsingHeader::new(format!("Log ({})", log.len())).id_salt(("plugin-log", id)).show(ui, |ui| {
                for l in &log {
                    let color = if l.starts_with("error") { Color32::from_rgb(240, 100, 100) } else { Color32::GRAY };
                    ui.label(RichText::new(l).monospace().small().color(color));
                }
            });
        }
    });
    // Accent bar: painted AFTER layout from the card's real rect, clipped to
    // the card's rounded shape so it can never spill past the card.
    let (shape, clip) = accent_bar(card.response.rect, ACCENT_BAR_W);
    ui.painter()
        .with_clip_rect(clip.intersect(ui.clip_rect()))
        .rect_filled(shape, CARD_RADIUS, tint);
}

const CARD_RADIUS: f32 = 10.0;
const ACCENT_BAR_W: f32 = 5.0;

/// Geometry of a card's left accent bar: paint `shape` (the card's own
/// rounded rect, inset by the 1px border) clipped to `clip` (its left
/// `width` pixels). The bar therefore has exactly the card's height and its
/// left corners follow the card's rounding.
pub(crate) fn accent_bar(card: egui::Rect, width: f32) -> (egui::Rect, egui::Rect) {
    let shape = card.shrink(1.0);
    let clip = egui::Rect::from_min_max(shape.min, egui::pos2((shape.left() + width).min(shape.right()), shape.bottom()));
    (shape, clip)
}

/// Readable secondary text: the normal text colour at reduced strength
/// (egui's "weak" is too faint on the tinted cards).
fn secondary_text(ui: &Ui) -> Color32 {
    let base = ui.visuals().text_color();
    let bg = ui.visuals().panel_fill;
    let mix = |a: u8, b: u8| ((a as f32) * 0.72 + (b as f32) * 0.28).round() as u8;
    Color32::from_rgb(mix(base.r(), bg.r()), mix(base.g(), bg.g()), mix(base.b(), bg.b()))
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Btn {
    Primary,
    Secondary,
    Link,
    Small,
    SmallPrimary,
    /// Big accent-filled call-to-action (About page donate button).
    Large,
}

/// A framed button with padding, an icon and a hover state.
pub(crate) fn action_button(ui: &mut Ui, icon: &str, label: &str, kind: Btn) -> egui::Response {
    use egui::{Align2, CornerRadius, FontId, Sense, Stroke, StrokeKind, Vec2};
    let accent = ui.visuals().selection.bg_fill;
    let small = matches!(kind, Btn::Small | Btn::SmallPrimary);
    let large = kind == Btn::Large;
    let font = FontId::proportional(if small { 13.0 } else if large { 18.0 } else { 14.0 });
    let text = if icon.is_empty() { label.to_string() } else { format!("{icon}  {label}") };
    let galley = ui.fonts(|f| f.layout_no_wrap(text, font, Color32::WHITE));
    let pad = if small {
        Vec2::new(10.0, 4.0)
    } else if large {
        Vec2::new(24.0, 11.0)
    } else {
        Vec2::new(14.0, 7.0)
    };
    let size = galley.size() + pad * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let pressed = resp.is_pointer_button_down_on();
    let dark = ui.visuals().dark_mode;
    let neutral = if dark { Color32::from_gray(52) } else { Color32::from_gray(228) };
    let neutral_hover = if dark { Color32::from_gray(66) } else { Color32::from_gray(212) };
    let (fill, stroke, text_col) = match kind {
        Btn::Primary | Btn::SmallPrimary | Btn::Large => (
            if pressed { accent.gamma_multiply(0.8) } else if hovered { accent.gamma_multiply(1.15) } else { accent },
            Stroke::new(1.0_f32, accent),
            Color32::WHITE,
        ),
        Btn::Secondary | Btn::Small => (
            if hovered { neutral_hover } else { neutral },
            Stroke::new(1.0_f32, ui.visuals().widgets.inactive.bg_stroke.color.gamma_multiply(1.5)),
            ui.visuals().strong_text_color(),
        ),
        Btn::Link => (
            if hovered { accent.gamma_multiply(0.15) } else { Color32::TRANSPARENT },
            Stroke::new(1.0_f32, accent.gamma_multiply(if hovered { 1.0 } else { 0.6 })),
            if dark { accent.gamma_multiply(1.4) } else { accent },
        ),
    };
    let r = CornerRadius::same(if small { 6 } else if large { 12 } else { 8 });
    ui.painter().rect(rect, r, fill, stroke, StrokeKind::Inside);
    ui.painter().galley_with_override_text_color(
        Align2::CENTER_CENTER.align_size_within_rect(galley.size(), rect).min,
        galley,
        text_col,
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::accent_bar;
    use eframe::egui::{pos2, Rect};

    #[test]
    fn accent_bar_stays_inside_its_card() {
        let card = Rect::from_min_max(pos2(100.0, 200.0), pos2(700.0, 330.0));
        let (shape, clip) = accent_bar(card, 5.0);
        // Same height as the card, never taller (the old bug used the
        // remaining page height).
        assert!(card.contains_rect(shape) && card.contains_rect(clip));
        assert_eq!(clip.top(), card.top() + 1.0);
        assert_eq!(clip.bottom(), card.bottom() - 1.0);
        assert_eq!(clip.left(), shape.left());
        assert_eq!(clip.width(), 5.0);
        // Paint = shape ∩ clip: the left strip of the card's own rounded rect.
        let painted = shape.intersect(clip);
        assert_eq!(painted, clip);
    }

    #[test]
    fn accent_bar_on_tiny_card_is_clamped() {
        let card = Rect::from_min_max(pos2(0.0, 0.0), pos2(4.0, 20.0));
        let (shape, clip) = accent_bar(card, 5.0);
        assert!(clip.right() <= shape.right());
        assert!(card.contains_rect(clip));
    }
}

fn m_has(m: &Option<bwr_plugins::Manifest>, p: Permission) -> bool {
    m.as_ref().is_some_and(|m| m.permission_set().contains(&p))
}

fn setting_row(ui: &mut Ui, app: &mut BigWaveApp, id: &str, key: &str, spec: &bwr_plugins::SettingSpec) {
    let host = &mut app.plugins.host;
    let Some(cur) = host.setting(id, key) else { return };
    let label = spec.label.clone().unwrap_or_else(|| key.to_string());
    let new: Option<serde_json::Value> = match spec.kind {
        SettingKind::Float => {
            let mut v = cur.as_float().unwrap_or(0.0);
            let (lo, hi) = (spec.min.unwrap_or(0.0), spec.max.unwrap_or(100.0));
            ui.add(egui::Slider::new(&mut v, lo..=hi).text(&label)).changed().then(|| serde_json::json!(v))
        }
        SettingKind::Int => {
            let mut v = cur.as_int().unwrap_or(0);
            let (lo, hi) = (spec.min.unwrap_or(0.0) as i64, spec.max.unwrap_or(100.0) as i64);
            ui.add(egui::Slider::new(&mut v, lo..=hi).text(&label)).changed().then(|| serde_json::json!(v))
        }
        SettingKind::Bool => {
            let mut v = cur.as_bool().unwrap_or(false);
            ui.checkbox(&mut v, &label).changed().then(|| serde_json::json!(v))
        }
        SettingKind::String | SettingKind::Color => {
            let mut v = cur.into_string().unwrap_or_default();
            let changed = ui
                .horizontal(|ui| {
                    ui.label(&label);
                    let r = ui.text_edit_singleline(&mut v);
                    r.lost_focus() || (r.changed() && spec.kind == SettingKind::String)
                })
                .inner;
            let ok = spec.kind != SettingKind::Color || bwr_plugins::Rgba::parse(&v).is_some();
            (changed && ok).then(|| serde_json::json!(v))
        }
        SettingKind::Choice => {
            let v = cur.into_string().unwrap_or_default();
            let mut picked = None;
            ui.horizontal(|ui| {
                ui.label(&label);
                egui::ComboBox::from_id_salt(("plugin-setting", id, key)).selected_text(&v).show_ui(ui, |ui| {
                    for o in &spec.options {
                        if ui.selectable_label(*o == v, o).clicked() {
                            picked = Some(serde_json::json!(o));
                        }
                    }
                });
            });
            picked
        }
    };
    if let Some(v) = new {
        host.set_setting(id, key, v);
    }
}

/// A whole nav page contributed by a plugin (`[[pages]]` + `on_page`).
pub fn plugin_page(ui: &mut Ui, app: &mut BigWaveApp, index: usize) {
    let Some((id, page)) = app.plugins.pages.get(index).cloned() else {
        ui.label("This plugin page is no longer available.");
        return;
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let widgets = app.plugins.host.page(&id, &page.id);
        if widgets.is_empty() {
            ui.heading(&page.title);
        }
        let results = render_widgets(ui, &format!("page-{id}-{}", page.id), &widgets);
        app.plugins.host.set_ui_results(&id, &format!("page:{}", page.id), results);
    });
}
