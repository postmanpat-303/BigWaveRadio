//! Browse by tag or country, then the stations under the selection.

use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::app::{BigWaveApp, BrowseSel, NetJob, Page};
use crate::pages::search::chip;
use crate::theme;
use crate::widgets::station_card::station_grid;

pub fn tags_page(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading(t!("browse.tags_title"));
    ui.label(t!("browse.tags_hint"));
    ui.add_space(8.0);
    if app.browse_selection.is_none() {
        if app.tags.is_empty() && !app.loading {
            app.loading = true;
            let _ = app.job_tx.send(NetJob::Tags);
        }
        ui.horizontal_wrapped(|ui| {
            for tag in app.tags.clone().iter().take(100) {
                if chip(ui, app, &format!("#{}  ·  {}", tag.name, tag.station_count)) {
                    app.browse_selection = Some(BrowseSel::Tag(tag.name.clone()));
                    app.loading = true;
                    let _ = app.job_tx.send(NetJob::StationsFor(BrowseSel::Tag(tag.name.clone())));
                }
            }
        });
    } else {
        selection_header(ui, app);
        station_grid(ui, app);
    }
}

pub fn countries_page(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.heading(t!("browse.countries_title"));
    ui.label(t!("browse.countries_hint"));
    ui.add_space(8.0);
    if app.browse_selection.is_none() {
        if app.countries.is_empty() && !app.loading {
            app.loading = true;
            let _ = app.job_tx.send(NetJob::Countries);
        }
        world_map(ui, app);
        ui.add_space(4.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for country in app.countries.clone().iter().take(250) {
                    if chip(ui, app, &format!("{}  ·  {}", country.name, country.station_count)) {
                        app.browse_selection = Some(BrowseSel::Country(country.name.clone()));
                        app.loading = true;
                        let _ = app
                            .job_tx
                            .send(NetJob::StationsFor(BrowseSel::Country(country.name.clone())));
                    }
                }
            });
        });
    } else {
        selection_header(ui, app);
        station_grid(ui, app);
    }
}

fn world_map(ui: &mut Ui, app: &mut BigWaveApp) {
    if app.countries.is_empty() {
        return;
    }
    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    let accent = theme::accent_color(app.settings.accent);
    let avail = ui.available_width();
    let map_h = (avail * 0.5).clamp(230.0, 440.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(avail, map_h), Sense::click());
    let painter = ui.painter_at(rect);

    // Ocean wash + landmass (landmask texture is white-on-transparent, tinted here).
    painter.rect_filled(
        rect,
        CornerRadius::same(10),
        if dark {
            egui::Color32::from_white_alpha(4)
        } else {
            egui::Color32::from_black_alpha(5)
        },
    );
    let land_tint = if dark {
        egui::Color32::from_gray(84)
    } else {
        egui::Color32::from_gray(205)
    };
    painter.image(
        app.world_map.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        land_tint,
    );

    // Graticule: flat-map feel.
    let grid = if dark {
        egui::Color32::from_white_alpha(9)
    } else {
        egui::Color32::from_black_alpha(12)
    };
    let project = |lon: f32, lat: f32| {
        Pos2::new(
            rect.left() + (lon + 180.0) / 360.0 * rect.width(),
            rect.top() + (90.0 - lat) / 180.0 * rect.height(),
        )
    };
    for lon in (-150..=150).step_by(60) {
        let a = project(lon as f32, 0.0);
        painter.line_segment(
            [Pos2::new(a.x, rect.top()), Pos2::new(a.x, rect.bottom())],
            Stroke::new(1.0, grid),
        );
    }
    for lat in (-60..=60).step_by(30) {
        let a = project(0.0, lat as f32);
        let strong = lat == 0;
        painter.line_segment(
            [Pos2::new(rect.left(), a.y), Pos2::new(rect.right(), a.y)],
            Stroke::new(
                1.0,
                if strong {
                    grid.linear_multiply(2.2)
                } else {
                    grid
                },
            ),
        );
    }
    painter.rect(
        rect,
        CornerRadius::same(10),
        egui::Color32::TRANSPARENT,
        Stroke::new(
            1.0,
            if dark {
                egui::Color32::from_white_alpha(22)
            } else {
                egui::Color32::from_black_alpha(28)
            },
        ),
        StrokeKind::Outside,
    );

    // Station-count bubbles at country centroids.
    let max_count = app
        .countries
        .iter()
        .map(|c| c.station_count)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut bubbles: Vec<(Pos2, f32, u32, String)> = Vec::new();
    for country in &app.countries {
        let Some(iso) = country.iso.as_deref() else { continue };
        let Some((lon, lat)) = bwr_core::geo::centroid(iso) else {
            continue;
        };
        let w = country.station_count as f32 / max_count as f32;
        let r = 2.0 + 9.0 * w.sqrt();
        bubbles.push((project(lon, lat), r, country.station_count, country.name.clone()));
    }

    // Hit-test largest first so overlapping bubbles pick the more significant one.
    let hover = response.hover_pos().and_then(|p| {
        let mut order: Vec<usize> = (0..bubbles.len()).collect();
        order.sort_by(|&a, &b| bubbles[b].1.total_cmp(&bubbles[a].1));
        order
            .into_iter()
            .find(|&i| (p - bubbles[i].0).length() <= bubbles[i].1.max(5.0))
    });

    for (i, (center, r, count, _)) in bubbles.iter().enumerate() {
        let w = *count as f32 / max_count as f32;
        let fill = if Some(i) == hover {
            accent
        } else {
            accent.gamma_multiply(0.25 + 0.55 * w.sqrt())
        };
        painter.circle_filled(*center, *r, fill);
        painter.circle_stroke(
            *center,
            *r,
            Stroke::new(
                1.0,
                if Some(i) == hover {
                    egui::Color32::WHITE
                } else {
                    accent.linear_multiply(0.6)
                },
            ),
        );
    }

    if let Some(i) = hover {
        let (_, _, count, name) = &bubbles[i];
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        egui::show_tooltip_at_pointer(
            ui.ctx(),
            egui::LayerId::new(egui::Order::Tooltip, ui.id().with("map-tip")),
            ui.id().with("map-tip"),
            |ui| {
                ui.label(tn!("browse.map_tip", *count, name = name));
            },
        );
        if response.clicked() {
            app.browse_selection = Some(BrowseSel::Country(name.clone()));
            app.loading = true;
            let _ = app
                .job_tx
                .send(NetJob::StationsFor(BrowseSel::Country(name.clone())));
        }
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(t!("browse.map_hint"))
            .small()
            .weak(),
    );
}

fn selection_header(ui: &mut Ui, app: &mut BigWaveApp) {
    ui.horizontal(|ui| {
        if ui.button(format!("← {}", t!("common.back"))).clicked() {
            app.browse_selection = None;
            app.stations.clear();
            app.loading = false;
        }
        let title = match app.browse_selection {
            Some(BrowseSel::Tag(ref t)) => format!("#{t}"),
            Some(BrowseSel::Country(ref c)) => c.clone(),
            None => String::new(),
        };
        ui.heading(title);
    });
    ui.add_space(6.0);
    let _ = Page::Tags; // keep import used if pages are trimmed
}
