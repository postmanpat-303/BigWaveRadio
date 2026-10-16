//! Plugins page (manage, approve, configure, import) and plugin-owned pages.

use bwr_plugins::{Permission, PluginStatus, SettingKind, HARD_MAX_FLASH_HZ};
use eframe::egui::{self, Color32, RichText, Ui};

use crate::app::BigWaveApp;
use crate::plugin_bridge::render_widgets;

pub fn ui(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading("Plugins");
    let dir = app.plugins.host.dir.clone();
    ui.label(
        RichText::new(format!(
            "Plugins are folders with plugin.toml + main.rhai in {}. Drop a folder there or import a .zip \
             (you can also drag a .zip onto the window). Edits reload automatically.",
            dir.display()
        ))
        .small()
        .weak(),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        #[cfg(windows)]
        if ui.button("📦  Import .zip…").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Plugin archive", &["zip"]).pick_file() {
                app.plugins.import(&p);
            }
        }
        #[cfg(not(windows))]
        {
            ui.add(egui::TextEdit::singleline(&mut app.plugins.import_path).hint_text("path/to/plugin.zip").desired_width(220.0));
            if ui.button("📦  Import .zip").clicked() {
                let p = std::path::PathBuf::from(app.plugins.import_path.trim());
                app.plugins.import(&p);
            }
        }
        if ui.button("📂  Open folder").clicked() {
            #[cfg(windows)]
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            #[cfg(not(windows))]
            let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
        }
        if ui.button("⟳  Reload all").clicked() {
            app.plugins.host.scan();
        }
    });
    ui.add_space(8.0);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        flash_safety(ui, app);
        ui.add_space(8.0);
        let ids: Vec<String> = app.plugins.host.plugins.iter().map(|p| p.id()).collect();
        if ids.is_empty() {
            ui.label(RichText::new("No plugins installed yet.").weak());
        }
        for id in ids {
            plugin_card(ui, app, &id);
            ui.add_space(8.0);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new("Writing a plugin (or asking an AI to)? See PLUGIN_GUIDE.md in the plugins folder, and test it with: BigWaveRadio.exe --check-plugin <folder>")
                .small()
                .weak(),
        );
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
        PluginStatus::Running => ("running", Color32::from_rgb(80, 200, 120)),
        PluginStatus::Disabled => ("disabled", Color32::GRAY),
        PluginStatus::NeedsApproval => ("needs approval", Color32::from_rgb(240, 190, 60)),
        PluginStatus::Error(_) => ("error", Color32::from_rgb(240, 80, 80)),
    };
    ui.label(RichText::new(text).color(color).small());
}

fn plugin_card(ui: &mut Ui, app: &mut BigWaveApp, id: &str) {
    let Some(p) = app.plugins.host.get(id) else { return };
    let status = p.status.clone();
    let manifest = p.manifest.clone();
    let log: Vec<String> = p.log.iter().rev().take(20).cloned().collect();
    let name = p.name();

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(&name).strong().size(15.0));
            if let Some(m) = &manifest {
                ui.label(RichText::new(format!("v{}{}", m.version, if m.author.is_empty() { String::new() } else { format!(" by {}", m.author) })).small().weak());
            }
            status_badge(ui, &status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("⟳ Reload").clicked() {
                    app.plugins.host.reload(id);
                }
                match &status {
                    PluginStatus::Running => {
                        if ui.small_button("Disable").clicked() {
                            app.plugins.host.set_enabled(id, false);
                        }
                    }
                    PluginStatus::Disabled => {
                        if ui.small_button("Enable").clicked() {
                            app.plugins.host.approve(id);
                        }
                    }
                    PluginStatus::NeedsApproval => {
                        if ui.small_button("Allow & enable").clicked() {
                            app.plugins.host.approve(id);
                        }
                    }
                    PluginStatus::Error(_) => {}
                }
            });
        });
        if let Some(m) = &manifest {
            if !m.description.is_empty() {
                ui.label(RichText::new(&m.description).small());
            }
            let perms: Vec<String> = m.permission_set().into_iter().map(|p: Permission| p.describe().to_string()).collect();
            if !perms.is_empty() {
                let prefix = if status == PluginStatus::NeedsApproval { "Wants to: " } else { "Can: " };
                ui.label(RichText::new(format!("{prefix}{}", perms.join(", "))).small().weak());
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
