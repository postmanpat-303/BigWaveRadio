//! Weighted word cloud in the shape of a cloud.
//!
//! Tags from radio-browser, sized by station count and placed along a
//! squashed Archimedean spiral so the whole cluster reads as a cloud. Words
//! float slowly, glitch letter-by-letter on hover (the hit rect is inflated
//! so the word stays clickable while it glitches), and faint musical
//! notation drifts upward behind them. Clicking a word searches that tag.

use bwr_core::Tag;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::BigWaveApp;
use crate::theme;

const MAX_WORDS: usize = 44;
const NOTES: &[&str] = &["♪", "♫", "♩", "♬", "𝄞"];
const NOTE_COUNT: usize = 16;
/// Whitespace inside each word's box; also the glitch amplitude budget.
const PAD: f32 = 8.0;

#[derive(Clone)]
struct Placed {
    name: String,
    font: FontId,
    /// Base rect (before the floating offset); includes PAD whitespace.
    rect: Rect,
    phase: f32,
    /// 0..1 weight, drives colour intensity.
    norm: f32,
}

/// Spiral placement cache — recomputed only when the tags or width change,
/// so the cloud floats without reshuffling.
#[derive(Default)]
pub struct WordCloud {
    width: f32,
    sig: u64,
    words: Vec<Placed>,
}

pub fn cloud(ui: &mut Ui, app: &mut BigWaveApp) {
    let t = ui.ctx().input(|i| i.time) as f32;
    let width = ui.available_width();
    let height = ui.available_height().clamp(240.0, 620.0);
    let area = Rect::from_min_size(ui.cursor().min, Vec2::new(width, height));
    ui.allocate_rect(area, Sense::hover());

    let dark = theme::is_dark(ui.ctx(), app.settings.theme);
    let accent = theme::accent_color(app.settings.accent);
    let weak = ui.visuals().weak_text_color();
    let strong = if dark {
        Color32::from_gray(235)
    } else {
        Color32::from_gray(25)
    };

    let painter = ui.painter_at(area);

    // --- musical notation drifting up behind the words ---------------------
    for i in 0..NOTE_COUNT {
        let speed = 12.0 + (i % 5) as f32 * 4.5;
        let phase = ((i * 61) % 97) as f32 / 97.0;
        let travel = area.height() + 40.0;
        let p = ((t * speed + phase * travel) % travel) / travel; // 0..1 rising
        let y = area.bottom() + 20.0 - p * travel;
        let x = area.left()
            + (((i * 137) % 100) as f32 / 100.0) * area.width()
            + (t * 0.6 + i as f32).sin() * 9.0;
        let fade = (p * std::f32::consts::PI).sin().powf(1.4);
        let size = 13.0 + (i % 4) as f32 * 2.5;
        let alpha = (fade * if dark { 34.0 } else { 44.0 }) as u8;
        let col = Color32::from_rgba_unmultiplied(weak.r(), weak.g(), weak.b(), alpha);
        painter.text(
            Pos2::new(x, y),
            Align2::CENTER_CENTER,
            NOTES[i % NOTES.len()],
            FontId::proportional(size),
            col,
        );
    }

    // --- layout (cached) ----------------------------------------------------
    let sig = signature(&app.tags);
    if app.word_cloud.sig != sig || (app.word_cloud.width - width).abs() > 8.0 {
        let words = ui.fonts(|fonts| layout(&app.tags, width, height, fonts));
        app.word_cloud.words = words;
        app.word_cloud.sig = sig;
        app.word_cloud.width = width;
    }
    let words = app.word_cloud.words.clone();

    // --- words: float + hover glitch + click --------------------------------
    let mut picked: Option<String> = None;
    for (wi, w) in words.iter().enumerate() {
        let float = Vec2::new(
            (t * 0.35 + w.phase).cos() * 1.7,
            (t * 0.5 + w.phase).sin() * 2.7,
        );
        let base = w.rect.translate(float);
        let response = ui.interact(base, ui.id().with(("cloud_word", wi)), Sense::click());

        let color = if response.hovered() {
            accent
        } else {
            // Weighted grey→strong ramp; the cloud darkens toward its base.
            let base_col = Color32::from_gray(if dark { 150 } else { 96 });
            base_col.lerp_to_gamma(strong, w.norm * 0.75)
        };

        let text_left = base.left() + PAD;
        let cy = base.center().y;
        if response.hovered() {
            glitch_letters(&painter, w, text_left, cy, wi, t, color, accent);
        } else {
            painter.text(
                Pos2::new(text_left, cy),
                Align2::LEFT_CENTER,
                w.name.clone(),
                w.font.clone(),
                color,
            );
        }

        if response.clicked() {
            picked = Some(w.name.clone());
        }
    }

    if let Some(name) = picked {
        app.query = name.clone();
        app.last_keystroke = Some(std::time::Instant::now());
    }
}

/// Draw a word letter by letter, each glyph jittered independently; a few
/// glyphs flash in the accent colour for the glitch read. Advances are
/// estimated (no per-glyph galley) — imperceptible in a glitch frame.
fn glitch_letters(
    painter: &egui::Painter,
    w: &Placed,
    start_x: f32,
    cy: f32,
    wi: usize,
    t: f32,
    color: Color32,
    accent: Color32,
) {
    let tick = (t / 0.09) as i64;
    let size = w.font.size;
    let mut x = start_x;
    for (ci, ch) in w.name.chars().enumerate() {
        let jx = jitter(tick, (wi * 31 + ci * 7) as i64) * 3.2;
        let jy = jitter(tick + 7777, (wi * 17 + ci * 13) as i64) * 3.2;
        // Occasionally a glyph shears sideways harder, RGB-split style.
        let shear = if jitter(tick, (ci * 97 + wi) as i64) > 0.93 { 4.0 } else { 0.0 };
        let col = if jitter(tick + 333, (wi * 7 + ci) as i64) > 0.88 {
            accent
        } else {
            color
        };
        painter.text(
            Pos2::new(x + jx + shear, cy + jy),
            Align2::LEFT_CENTER,
            ch.to_string(),
            w.font.clone(),
            col,
        );
        x += estimate_advance(ch, size);
    }
}

/// Rough per-glyph advance for the glitch path.
fn estimate_advance(ch: char, size: f32) -> f32 {
    let f = match ch {
        'i' | 'l' | 'j' | 't' | 'f' | 'I' | ' ' => 0.32,
        'm' | 'w' | 'M' | 'W' => 0.78,
        c if c.is_uppercase() => 0.62,
        _ => 0.5,
    };
    size * f
}

/// Deterministic -1..1 hash for a (tick, salt) pair.
fn jitter(tick: i64, salt: i64) -> f32 {
    let mut h = (tick as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (salt as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    (h % 2000) as f32 / 1000.0 - 1.0
}

/// Content signature so the layout refreshes when the tag list changes.
fn signature(tags: &[Tag]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for t in tags.iter().take(64) {
        for b in t.name.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
        h ^= t.station_count as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

/// Largest-first placement along a vertically-squashed Archimedean spiral:
/// big words land near the middle and the silhouette reads as a cloud.
fn layout(tags: &[Tag], width: f32, height: f32, fonts: &egui::text::Fonts) -> Vec<Placed> {
    let mut tags: Vec<&Tag> = tags
        .iter()
        .filter(|t| !t.name.trim().is_empty() && t.name.chars().count() <= 22)
        .collect();
    tags.sort_by(|a, b| b.station_count.cmp(&a.station_count));
    tags.truncate(MAX_WORDS);

    let max_c = tags
        .iter()
        .map(|t| t.station_count)
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let min_c = tags.iter().map(|t| t.station_count).min().unwrap_or(0) as f32;

    let mut placed: Vec<Placed> = Vec::new();
    let mut theta = 0.0f32;
    let center = Pos2::new(width / 2.0, height / 2.0);

    for (i, tag) in tags.iter().enumerate() {
        let norm = if max_c > min_c {
            (tag.station_count as f32 - min_c) / (max_c - min_c)
        } else {
            0.5
        };
        let size = 12.0 + 20.0 * norm.powf(0.6);
        let font = FontId::proportional(size);
        let name = tag.name.trim().to_string();
        let text_size = fonts
            .layout_no_wrap(name.clone(), font.clone(), Color32::WHITE)
            .size();
        let box_size = text_size + Vec2::splat(PAD * 2.0);

        let mut rect = None;
        for _ in 0..1200 {
            let r = 10.0 + 8.5 * theta;
            let pos = Pos2::new(
                center.x + r * theta.cos(),
                center.y + 0.58 * r * theta.sin(),
            );
            let candidate = Rect::from_center_size(pos, box_size);
            let inside = candidate.min.x > 2.0
                && candidate.min.y > 2.0
                && candidate.max.x < width - 2.0
                && candidate.max.y < height - 2.0;
            if inside && !placed.iter().any(|p: &Placed| p.rect.intersects(candidate)) {
                rect = Some(candidate);
                break;
            }
            theta += 0.36;
        }
        let rect = rect.unwrap_or_else(|| {
            let r = 10.0 + 8.5 * theta;
            Rect::from_center_size(
                Pos2::new(center.x + r * theta.cos(), center.y + 0.58 * r * theta.sin()),
                box_size,
            )
        });
        placed.push(Placed {
            name,
            font,
            rect,
            phase: i as f32 * 1.618,
            norm,
        });
    }
    placed
}
