//! Glue between `bwr-plugins` and the egui app. app.rs only calls
//! `note_player_event`, `pre_frame` and `post_frame`; everything else
//! (event forwarding, key capture, overlay + warning screen, fullscreen,
//! painting draw commands, rendering plugin widgets) lives here.

use std::collections::HashMap;
use std::time::Instant;

use bwr_player::PlayerEvent;
use bwr_plugins::{
    AudioFrame, DrawCmd, OverlayPhase, PageSpec, PluginHost, Request, Rgba, StationInfo, UiResult, Widget,
};
use bwr_spectrum::BeatTracker;
use eframe::egui::{self, Color32, Context, Pos2, Rect, Stroke};

use crate::app::{BigWaveApp, Page};

pub struct PluginBridge {
    pub host: PluginHost,
    beat: BeatTracker,
    last_frame: Instant,
    last_station: Option<String>,
    fullscreen: bool,
    toasts: Vec<(String, Instant)>,
    /// Pages contributed by running plugins, indexed by `Page::PluginPage(i)`.
    pub pages: Vec<(String, PageSpec)>,
    pub import_path: String,
}

impl PluginBridge {
    pub fn new() -> Self {
        let dir = bwr_plugins::default_plugins_dir();
        bwr_plugins::install_bundled(&dir);
        let host = PluginHost::new(dir);
        let pages = host.pages();
        Self {
            host,
            beat: BeatTracker::new(),
            last_frame: Instant::now(),
            last_station: None,
            fullscreen: false,
            toasts: Vec::new(),
            pages,
            import_path: String::new(),
        }
    }

    pub fn note_player_event(&mut self, ev: &PlayerEvent) {
        match ev {
            PlayerEvent::Connecting => self.host.player("connecting"),
            PlayerEvent::Playing => self.host.player("playing"),
            PlayerEvent::Stopped => {
                self.beat.reset();
                self.host.player("stopped")
            }
            PlayerEvent::Failed(_) => self.host.player("failed"),
            PlayerEvent::Meta(t) => self.host.meta(t),
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toasts.push((msg.into(), Instant::now()));
    }

    pub fn import(&mut self, path: &std::path::Path) {
        match bwr_plugins::import_zip(path, &self.host.dir) {
            Ok(id) => {
                self.host.scan();
                self.toast(format!("Imported plugin \"{id}\". Enable it on the Plugins page."));
            }
            Err(e) => self.toast(format!("Import failed: {e}")),
        }
    }
}

/// Before layout: hot reload, events, audio, key capture, plugin requests.
pub fn pre_frame(app: &mut BigWaveApp, ctx: &Context) {
    let b = &mut app.plugins;
    b.host.poll_changes();

    // .zip files dropped onto the window.
    let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
    for p in dropped {
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
            b.import(&p);
        }
    }

    let url = app.current.as_ref().map(|s| s.stream_url.clone());
    if url != b.last_station {
        b.last_station = url;
        if let Some(s) = &app.current {
            b.host.station_changed(&StationInfo {
                name: s.name.clone(),
                url: s.stream_url.clone(),
                tags: s.tags.clone(),
                country: s.country.clone(),
                codec: s.codec.clone(),
                bitrate: s.bitrate,
            });
        }
    }

    let dt = b.last_frame.elapsed().as_secs_f64().min(0.25);
    b.last_frame = Instant::now();
    let (level, peak) = app.player.spectrum.level_peak();
    let features = b.beat.update(b.host.now(), &app.bands, level, peak);
    b.host.frame(dt, &AudioFrame { features, bands: app.bands, playing: app.is_playing });

    // While an overlay is open, the host owns the keyboard. Esc is handled
    // by the host itself, so it always exits even if the plugin misbehaves.
    if b.host.overlay.is_some() {
        let keys = ctx.input_mut(|i| {
            let mut keys = Vec::new();
            i.events.retain(|e| match e {
                egui::Event::Key { key, pressed, repeat, .. } => {
                    if *pressed && !*repeat {
                        keys.push(key.name().to_string());
                    }
                    false
                }
                egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Copy | egui::Event::Cut => false,
                _ => true,
            });
            keys
        });
        for k in keys {
            b.host.key(&k);
        }
    }

    let requests = b.host.take_requests();
    b.pages = b.host.pages();
    for (id, r) in requests {
        match r {
            Request::Toast(msg) => app.plugins.toast(format!("{id}: {msg}")),
            Request::PlayerStop => app.stop(),
            Request::PlayerToggle => app.toggle_play_pause(),
            Request::PlayerNext => app.next_station(),
            Request::PlayerPrevious => app.previous_station(),
            Request::SetVolume(v) => {
                app.player.set_volume(v);
                app.settings.volume = v;
            }
            Request::OpenPage(pid) => {
                if let Some(i) = app.plugins.pages.iter().position(|(pl, pg)| *pl == id && pg.id == pid) {
                    app.page = Page::PluginPage(i);
                }
            }
            Request::OpenOverlay { .. } | Request::CloseOverlay => {}
        }
    }
    if let Page::PluginPage(i) = app.page {
        if i >= app.plugins.pages.len() {
            app.page = Page::Plugins;
        }
    }

    // Keep the window's fullscreen state in step with the overlay session.
    let b = &mut app.plugins;
    let want = b.host.overlay.as_ref().is_some_and(|s| s.fullscreen);
    if want != b.fullscreen {
        b.fullscreen = want;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(want));
    }
}

/// After layout: overlay (warning screen or plugin drawing) and toasts.
pub fn post_frame(app: &mut BigWaveApp, ctx: &Context) {
    let b = &mut app.plugins;
    if let Some(session) = b.host.overlay.clone() {
        let rect = ctx.screen_rect();
        let name = b.host.get(&session.plugin_id).map(|p| p.name()).unwrap_or_default();
        let cmds = if session.phase == OverlayPhase::Running { b.host.overlay_commands() } else { Vec::new() };
        let cap = b.host.flash_cap();
        let mut accept = false;
        let mut cancel = false;
        let mut clicked = false;
        egui::Area::new(egui::Id::new("plugin-overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let resp = ui.allocate_rect(rect, egui::Sense::click());
                match session.phase {
                    OverlayPhase::Warning => {
                        ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(8, 8, 8));
                        let inner = Rect::from_center_size(rect.center(), egui::vec2(560.0_f32.min(rect.width() - 40.0), 320.0));
                        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner), |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(egui::RichText::new("⚠ Photosensitivity warning").size(28.0).color(Color32::from_rgb(255, 200, 40)));
                                ui.add_space(12.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "\"{name}\" shows flashing lights. Flashing can trigger seizures in people with \
                                         photosensitive epilepsy. Stop immediately if you feel unwell.\n\n\
                                         Flashes are limited to {cap} per second. Esc always exits."
                                    ))
                                    .size(16.0)
                                    .color(Color32::from_gray(220)),
                                );
                                ui.add_space(18.0);
                                ui.horizontal(|ui| {
                                    ui.add_space((ui.available_width() - 330.0).max(0.0) / 2.0);
                                    if ui.button(egui::RichText::new("I understand, start (Enter)").size(16.0)).clicked() {
                                        accept = true;
                                    }
                                    if ui.button(egui::RichText::new("Cancel (Esc)").size(16.0)).clicked() {
                                        cancel = true;
                                    }
                                });
                            });
                        });
                    }
                    OverlayPhase::Running => {
                        paint(ui.painter(), rect, &cmds);
                        clicked = resp.clicked() || resp.secondary_clicked();
                    }
                }
            });
        if accept {
            b.host.accept_warning();
        }
        if cancel {
            b.host.end_overlay();
        }
        if clicked {
            b.host.key("MouseClick");
        }
        ctx.request_repaint();
    } else if !b.host.plugins.is_empty() {
        // Plugins animate (on_tick); keep frames coming at a modest rate.
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }

    // Toasts (bottom-right).
    b.toasts.retain(|(_, t)| t.elapsed().as_secs_f32() < 5.0);
    if !b.toasts.is_empty() {
        egui::Area::new(egui::Id::new("plugin-toasts"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -110.0))
            .show(ctx, |ui| {
                for (msg, _) in &b.toasts {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(360.0);
                        ui.label(msg);
                    });
                }
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

fn c32(c: Rgba) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Paint plugin draw commands into `rect` (coordinates are 0..1 fractions).
pub fn paint(painter: &egui::Painter, rect: Rect, cmds: &[DrawCmd]) {
    let painter = painter.with_clip_rect(rect);
    let (w, h) = (rect.width(), rect.height());
    let at = |x: f32, y: f32| Pos2::new(rect.min.x + x * w, rect.min.y + y * h);
    for c in cmds {
        match c {
            DrawCmd::Fill(col) => {
                painter.rect_filled(rect, 0.0, c32(*col));
            }
            DrawCmd::Rect { x, y, w: rw, h: rh, color } => {
                painter.rect_filled(Rect::from_min_max(at(*x, *y), at(x + rw, y + rh)), 0.0, c32(*color));
            }
            DrawCmd::Circle { x, y, r, color, stroke } => match stroke {
                None => {
                    painter.circle_filled(at(*x, *y), r * h, c32(*color));
                }
                Some(sw) => {
                    painter.circle_stroke(at(*x, *y), r * h, Stroke::new(*sw, c32(*color)));
                }
            },
            DrawCmd::Line { x1, y1, x2, y2, width, color } => {
                painter.line_segment([at(*x1, *y1), at(*x2, *y2)], Stroke::new(*width, c32(*color)));
            }
            DrawCmd::Text { x, y, text, size, color, centered } => {
                let align = if *centered { egui::Align2::CENTER_CENTER } else { egui::Align2::LEFT_TOP };
                painter.text(at(*x, *y), align, text, egui::FontId::proportional(*size), c32(*color));
            }
            DrawCmd::Paths { cx, cy, paths, width, color } => {
                let centre = at(*cx, *cy);
                for p in paths {
                    let pts: Vec<Pos2> = p.iter().map(|(px, py)| centre + egui::vec2(px * h, py * h)).collect();
                    painter.add(egui::Shape::line(pts, Stroke::new(*width, c32(*color))));
                }
            }
        }
    }
}

/// Render plugin widgets; returns interactions keyed by widget index.
pub fn render_widgets(ui: &mut egui::Ui, salt: &str, widgets: &[Widget]) -> HashMap<usize, UiResult> {
    let mut out = HashMap::new();
    for (i, w) in widgets.iter().enumerate() {
        match w {
            Widget::Heading(t) => {
                ui.heading(t);
            }
            Widget::Label(t) => {
                ui.add(egui::Label::new(t.as_str()).wrap());
            }
            Widget::Small(t) => {
                ui.add(egui::Label::new(egui::RichText::new(t.as_str()).small().weak()).wrap());
            }
            Widget::Separator => {
                ui.separator();
            }
            Widget::Space(px) => ui.add_space(*px),
            Widget::Button(t) => {
                if ui.button(t.as_str()).clicked() {
                    out.insert(i, UiResult::Clicked);
                }
            }
            Widget::Checkbox { label, value } => {
                let mut v = *value;
                if ui.checkbox(&mut v, label.as_str()).changed() {
                    out.insert(i, UiResult::Bool(v));
                }
            }
            Widget::Slider { label, value, min, max } => {
                let mut v = *value;
                if ui.add(egui::Slider::new(&mut v, *min..=*max).text(label.as_str())).changed() {
                    out.insert(i, UiResult::Float(v));
                }
            }
            Widget::Choice { label, value, options } => {
                ui.horizontal(|ui| {
                    ui.label(label.as_str());
                    egui::ComboBox::from_id_salt((salt, i)).selected_text(value.as_str()).show_ui(ui, |ui| {
                        for o in options {
                            if ui.selectable_label(o == value, o.as_str()).clicked() && o != value {
                                out.insert(i, UiResult::Text(o.clone()));
                            }
                        }
                    });
                });
            }
            Widget::Canvas { height, cmds } => {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), *height), egui::Sense::hover());
                paint(ui.painter(), rect, cmds);
            }
        }
    }
    out
}
