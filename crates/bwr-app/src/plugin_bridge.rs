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

    /// The first hotkey hint for a plugin, e.g. "Ctrl+F,U".
    pub fn hotkey_hint(&self, plugin_id: &str) -> Option<String> {
        self.host.hotkeys().into_iter().find(|(id, _)| id == plugin_id).map(|(_, h)| h.keys)
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
    if std::env::var_os("BWR_FAKE_AUDIO").is_some() && !app.is_playing {
        // Developer aid: a synthetic 124 BPM groove for testing visualisers.
        let t = b.host.now();
        let (features, bands) = fake_audio(t);
        b.host.frame(dt, &AudioFrame { features, bands, playing: true });
    } else {
        let features = b.beat.update(b.host.now(), &app.bands, level, peak);
        b.host.frame(dt, &AudioFrame { features, bands: app.bands, playing: app.is_playing });
    }

    // While an overlay is open, the host owns the keyboard. Esc is handled
    // by the host itself, so it always exits even if the plugin misbehaves.
    if b.host.overlay.is_some() {
        let keys = ctx.input_mut(|i| {
            let mut keys = Vec::new();
            i.events.retain(|e| match e {
                egui::Event::Key { key, pressed, repeat, .. } => {
                    if *pressed && !*repeat {
                        keys.push(plugin_key_name(*key).to_string());
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

    } else {
        // App-wide plugin hotkeys (e.g. Ctrl+F,U). Matching keys are
        // swallowed so they never reach text fields or other shortcuts.
        let host = &mut b.host;
        if !host.hotkeys().is_empty() {
            ctx.input_mut(|i| {
                i.events.retain(|e| match e {
                    egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => {
                        let mods = bwr_plugins::Mods { ctrl: modifiers.command || modifiers.ctrl, shift: modifiers.shift, alt: modifiers.alt };
                        !host.hotkey_key(plugin_key_name(*key), mods)
                    }
                    _ => true,
                })
            });
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
        b.host.limiter.aspect = rect.width() / rect.height().max(1.0);
        let cmds = if session.phase == OverlayPhase::Running { b.host.overlay_commands() } else { Vec::new() };
        let cap = b.host.flash_cap();
        let mut accept = false;
        let mut cancel = false;
        let mut clicked = false;
        // Debug is egui's top layer: nothing (not even tooltips) shows over it.
        egui::Area::new(egui::Id::new("plugin-overlay"))
            .order(egui::Order::Debug)
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
        fps_log();
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

/// Key names as documented for plugins (egui says "Right", plugins get
/// "ArrowRight").
fn plugin_key_name(key: egui::Key) -> &'static str {
    match key {
        egui::Key::ArrowUp => "ArrowUp",
        egui::Key::ArrowDown => "ArrowDown",
        egui::Key::ArrowLeft => "ArrowLeft",
        egui::Key::ArrowRight => "ArrowRight",
        k => k.name(),
    }
}

/// Synthetic audio (BWR_FAKE_AUDIO=1): 124 BPM kick, moving mids/highs.
fn fake_audio(t: f64) -> (bwr_spectrum::AudioFeatures, [f32; bwr_spectrum::BANDS]) {
    let beat_len = 60.0 / 124.0;
    let phase = (t % beat_len) / beat_len;
    let kick = (-phase * 6.0).exp() as f32;
    let mut bands = [0.0f32; bwr_spectrum::BANDS];
    for (i, b) in bands.iter_mut().enumerate() {
        let x = i as f32 / bwr_spectrum::BANDS as f32;
        let wob = (0.5 + 0.5 * ((t as f32) * (1.3 + x * 3.0) + x * 9.0).sin()) * 0.6;
        *b = ((1.0 - x).powi(2) * kick * 0.9 + wob * (0.4 + 0.6 * x) * 0.7).clamp(0.0, 1.0);
    }
    let bass = bands[..4].iter().sum::<f32>() / 4.0;
    let mid = bands[8..20].iter().sum::<f32>() / 12.0;
    let treble = bands[20..].iter().sum::<f32>() / 12.0;
    let beat = {
        thread_local! { static LAST: std::cell::Cell<i64> = const { std::cell::Cell::new(-1) }; }
        let n = (t / beat_len) as i64;
        LAST.with(|l| { let b = l.get() != n; l.set(n); b })
    };
    let f = bwr_spectrum::AudioFeatures { level: 0.3 + bass * 0.4, peak: 0.8, bass, mid, treble, beat, bpm: 124.0, quiet: false };
    (f, bands)
}

/// With BWR_FPS=1 set, print overlay frames per second to stderr.
fn fps_log() {
    thread_local! { static FPS: std::cell::Cell<(u32, Option<Instant>)> = const { std::cell::Cell::new((0, None)) }; }
    if std::env::var_os("BWR_FPS").is_none() {
        return;
    }
    FPS.with(|c| {
        let (n, t) = c.get();
        let t = t.unwrap_or_else(Instant::now);
        let el = t.elapsed().as_secs_f32();
        if el >= 2.0 {
            eprintln!("[fps] {:.1}", (n + 1) as f32 / el);
            c.set((0, Some(Instant::now())));
        } else {
            c.set((n + 1, Some(t)));
        }
    });
}

fn c32(c: Rgba) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

thread_local! {
    /// One engine + texture per `draw.visual` id (each keeps its own feedback).
    static VISUALS: std::cell::RefCell<HashMap<String, (bwr_plugins::VisualEngine, Option<egui::TextureHandle>, Instant)>> =
        std::cell::RefCell::new(HashMap::new());
}

fn paint_visual(painter: &egui::Painter, area: Rect, p: &bwr_plugins::VisualParams) {
    let [rx, ry, rw, rh] = p.rect;
    let rect = Rect::from_min_size(
        area.min + egui::vec2(rx * area.width(), ry * area.height()),
        egui::vec2(rw.max(0.0) * area.width(), rh.max(0.0) * area.height()),
    );
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let aspect = rect.width() / rect.height().max(1.0);
    let (w, h) = bwr_plugins::VisualEngine::size_for(aspect, p.quality);
    VISUALS.with(|v| {
        let mut map = v.borrow_mut();
        // Forget engines nobody has drawn for a while.
        map.retain(|_, e| e.2.elapsed().as_secs() < 10);
        let e = map.entry(p.id.clone()).or_insert_with(|| (bwr_plugins::VisualEngine::new(w, h), None, Instant::now()));
        if e.0.w != w || e.0.h != h {
            e.0 = bwr_plugins::VisualEngine::new(w, h);
        }
        e.2 = Instant::now();
        let px = e.0.render(p);
        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], px);
        let opts = egui::TextureOptions::LINEAR;
        match &mut e.1 {
            Some(t) => t.set(img, opts),
            None => e.1 = Some(painter.ctx().load_texture(format!("visual-{}", p.id), img, opts)),
        }
        if let Some(t) = &e.1 {
            painter.image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
    });
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
            DrawCmd::Rings { cx, cy, radius, width, colors } => {
                let centre = at(*cx, *cy);
                let n = ((radius / width).ceil() as usize).min(bwr_plugins::MAX_RINGS);
                for k in 0..n {
                    let col = colors[k % colors.len()];
                    if col.0[3] == 0 {
                        continue;
                    }
                    let outer = radius - k as f32 * width;
                    let w = width.min(outer);
                    painter.circle_stroke(centre, (outer - w / 2.0) * h, Stroke::new(w * h, c32(col)));
                }
            }
            DrawCmd::Paths { cx, cy, paths, width, color } => {
                let centre = at(*cx, *cy);
                for p in paths {
                    let pts: Vec<Pos2> = p.iter().map(|(px, py)| centre + egui::vec2(px * h, py * h)).collect();
                    painter.add(egui::Shape::line(pts, Stroke::new(*width, c32(*color))));
                }
            }
                        DrawCmd::Visual(p) => paint_visual(&painter, rect, p),
            DrawCmd::Shape { cx, cy, paths, width, color, fill } => {
                let centre = at(*cx, *cy);
                let to = |(px, py): &(f32, f32)| centre + egui::vec2(px * h, py * h);
                if let Some(f) = fill.filter(|f| f.0[3] > 0) {
                    for p in paths.iter().filter(|p| bwr_plugins::is_closed(p)) {
                        let mut mesh = egui::Mesh::default();
                        for v in p {
                            mesh.colored_vertex(to(v), c32(f));
                        }
                        for [a, b, c] in bwr_plugins::triangulate(p) {
                            mesh.add_triangle(a as u32, b as u32, c as u32);
                        }
                        painter.add(egui::Shape::mesh(mesh));
                    }
                }
                let w = width * h;
                if w > 0.0 && color.0[3] > 0 {
                    let col = c32(*color);
                    for p in paths {
                        let pts: Vec<Pos2> = p.iter().map(to).collect();
                        // Round joins and caps so thick strokes look solid.
                        for q in &pts {
                            painter.circle_filled(*q, w / 2.0, col);
                        }
                        painter.add(egui::Shape::line(pts, Stroke::new(w, col)));
                    }
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
