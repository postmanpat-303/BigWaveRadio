//! The left navigation of the card look: brand row, pages, plugin pages,
//! then Plugins / Settings / About and the language picker at the bottom.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::app::{BigWaveApp, NetJob, Page};
use crate::widgets::cards::{self, Icon, Pal};

pub const WIDTH: f32 = 212.0;
const ITEM_H: f32 = 36.0;

/// Switch page with the side effects each page needs on entry.
pub fn go(app: &mut BigWaveApp, page: Page) {
    if app.page == page {
        return;
    }
    app.page = page;
    app.browse_selection = None;
    match page {
        Page::Player => {
            let idx = app.dial_mood;
            app.tune_mood(idx);
        }
        Page::Discover => {
            app.reload_current_list();
            app.prefetch_moods();
        }
        Page::Library | Page::Settings | Page::Stars | Page::About => app.stations.clear(),
        Page::Plugins | Page::PluginPage(_) => {}
        Page::Mood(i) => app.open_mood(i),
        Page::Search => {
            // Tags back the idle "pick a vibe" chips — fetch on first visit
            // rather than at startup so they never block playback actions.
            if app.tags.is_empty() {
                app.loading = true;
                let _ = app.job_tx.send(NetJob::Tags);
            }
        }
        Page::Tags => {
            app.stations.clear();
            if app.tags.is_empty() {
                app.loading = true;
                let _ = app.job_tx.send(NetJob::Tags);
            }
        }
        Page::Countries => {
            app.stations.clear();
            if app.countries.is_empty() {
                app.loading = true;
                let _ = app.job_tx.send(NetJob::Countries);
            }
        }
    }
}

fn item(ui: &mut Ui, pal: &Pal, ic: Icon, label: &str, selected: bool, hint: Option<&str>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ITEM_H), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, CornerRadius::same(8), pal.nav_active);
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(8), pal.nav_active.gamma_multiply(0.6));
    }
    let col = if selected { pal.text } else { pal.sub };
    let icol = if selected { pal.primary } else { pal.sub };
    cards::icon(p, ic, Pos2::new(rect.left() + 20.0, rect.center().y), 17.0, icol);
    let mut max_w = rect.width() - 44.0;
    if let Some(h) = hint {
        p.text(Pos2::new(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, h, FontId::proportional(11.0), pal.weak);
        max_w -= cards::text_width(ui, h, FontId::proportional(11.0)) + 8.0;
    }
    let g = cards::line_galley(ui, label, FontId::proportional(14.5), col, max_w);
    ui.painter().galley(Pos2::new(rect.left() + 38.0, rect.center().y - g.size().y / 2.0), g, col);
    resp
}

fn nav_page(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, page: Page, ic: Icon, label: &str) -> egui::Response {
    let resp = item(ui, pal, ic, label, app.page == page, None);
    if resp.clicked() {
        go(app, page);
    }
    resp
}

pub fn show(ui: &mut Ui, app: &mut BigWaveApp) {
    let pal = Pal::new(ui.ctx(), app);
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, pal.nav);
    ui.painter().vline(full.right() - 0.5, full.y_range(), Stroke::new(1.0_f32, pal.nav_border));

    // Brand row.
    ui.add_space(14.0);
    let (brand, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::hover());
    let logo = Rect::from_min_size(Pos2::new(brand.left() + 6.0, brand.top() + 2.0), Vec2::splat(40.0));
    ui.painter().image(
        app.about_logo_icon.id(),
        logo,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.painter().text(
        Pos2::new(logo.right() + 10.0, brand.top() + 13.0),
        Align2::LEFT_CENTER,
        "BigWave Radio",
        FontId::proportional(15.5),
        pal.text,
    );
    ui.painter().text(
        Pos2::new(logo.right() + 10.0, brand.top() + 32.0),
        Align2::LEFT_CENTER,
        format!("v{}", env!("CARGO_PKG_VERSION")),
        FontId::proportional(11.0),
        pal.weak,
    );
    ui.add_space(16.0);

    let bottom_h = 3.0 * (ITEM_H + 2.0) + 44.0;
    egui::ScrollArea::vertical()
        .id_salt("nav-scroll")
        .auto_shrink([false, false])
        .max_height((ui.available_height() - bottom_h).max(72.0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            nav_page(ui, app, &pal, Page::Player, Icon::Radio, t!("cardsui.now_playing"));
            nav_page(ui, app, &pal, Page::Discover, Icon::Home, t!("nav.discover"));
            nav_page(ui, app, &pal, Page::Search, Icon::Search, t!("nav.search"));
            nav_page(ui, app, &pal, Page::Library, Icon::Heart, t!("nav.library"));
            let rated: usize = (1..=5).map(|r| app.ratings.count(r)).sum();
            let count = (rated > 0).then(|| rated.to_string());
            let resp = item(ui, &pal, Icon::Star, t!("stars.title"), app.page == Page::Stars, count.as_deref());
            if resp.on_hover_text(t!("rail.stars_tip")).clicked() {
                go(app, Page::Stars);
            }
            nav_page(ui, app, &pal, Page::Tags, Icon::Tag, t!("nav.tags"));
            nav_page(ui, app, &pal, Page::Countries, Icon::Globe, t!("nav.countries"));

            if !app.plugins.pages.is_empty() {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(t!("nav.plugins").to_uppercase()).size(11.0).color(pal.weak));
                });
                ui.add_space(4.0);
            }
            for (i, (pid, pg)) in app.plugins.pages.clone().into_iter().enumerate() {
                let ic = match pid.as_str() {
                    "visions" => Icon::Eye,
                    "strobe" => Icon::Bolt,
                    _ => Icon::Plug,
                };
                let hint = app.plugins.hotkey_hint(&pid);
                let title = pg.title_for(crate::i18n::current().code()).to_string();
                let resp = item(ui, &pal, ic, &title, app.page == Page::PluginPage(i), hint.as_deref());
                if resp.clicked() {
                    go(app, Page::PluginPage(i));
                }
                // Double-click a plugin in the side menu to start it
                // (prefers an action named "start", else its first action).
                if resp.double_clicked() {
                    let actions: Vec<String> = app
                        .plugins
                        .host
                        .hotkeys()
                        .into_iter()
                        .filter(|(id, _)| id == &pid)
                        .map(|(_, h)| h.action)
                        .collect();
                    let action = actions.iter().find(|a| *a == "start").cloned().or_else(|| actions.first().cloned());
                    if let Some(a) = action {
                        app.plugins.host.trigger_action(&pid, &a);
                    }
                }
                resp.on_hover_text(t!("nav.plugin_tip", title = title));
            }
        });

    // Bottom block, anchored to the panel's bottom edge.
    let rest = ui.available_rect_before_wrap();
    let block = Rect::from_min_max(Pos2::new(rest.left(), (rest.bottom() - bottom_h).max(rest.top())), rest.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(block), |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        nav_page(ui, app, &pal, Page::Plugins, Icon::Plug, t!("nav.plugins"));
        nav_page(ui, app, &pal, Page::Settings, Icon::Gear, t!("nav.settings"));
        nav_page(ui, app, &pal, Page::About, Icon::Info, t!("nav.about"));
        ui.add_space(6.0);
        language_picker(ui, app, &pal);
    });
}

/// The language picker (same choices as Settings → Language).
fn language_picker(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal) {
    use crate::i18n::{Choice, Lang};
    let current = Choice::parse(&app.ui_prefs.language);
    let system = crate::i18n::system_lang();
    let name = |c: Choice| match c {
        Choice::System => t!("settings.language_system", lang = system.native_name()),
        Choice::Fixed(l) => l.native_name().to_string(),
    };
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let (r, _) = ui.allocate_exact_size(Vec2::new(18.0, 24.0), Sense::hover());
        cards::icon(ui.painter(), Icon::Globe2, r.center(), 15.0, pal.sub);
        let mut picked = None;
        egui::ComboBox::from_id_salt("nav-language")
            .width(ui.available_width() - 12.0)
            .selected_text(egui::RichText::new(name(current)).size(12.5).color(pal.sub))
            .show_ui(ui, |ui| {
                for c in std::iter::once(Choice::System).chain(Lang::ALL.map(Choice::Fixed)) {
                    if ui.selectable_label(c == current, name(c)).clicked() {
                        picked = Some(c);
                    }
                }
            })
            .response
            .on_hover_text(t!("settings.language"));
        if let Some(c) = picked {
            app.set_language(c);
        }
    });
}
