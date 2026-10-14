//! 32-bar realtime spectrum painter.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke};

use crate::theme;

/// Paint 32 vertical bars rising from the bottom of `rect`.
/// Bars are slightly transparent and coloured as an accent gradient.
pub fn paint(
    ui: &egui::Ui,
    rect: Rect,
    levels: &[f32; bwr_spectrum::BANDS],
    accent: bwr_core::settings::Accent,
) {
    if rect.width() < 10.0 || rect.height() < 4.0 {
        return;
    }
    let painter = ui.painter_at(rect);
    let n = levels.len();
    let gap = 3.0f32;
    let bar_w = ((rect.width() - gap * (n as f32 - 1.0)) / n as f32).max(1.5);
    let base = theme::accent_color(accent);

    for (i, &level) in levels.iter().enumerate() {
        if level <= 0.01 {
            continue;
        }
        let h = (level * rect.height()).clamp(2.0, rect.height());
        let x = rect.left() + i as f32 * (bar_w + gap);
        let t = i as f32 / (n as f32 - 1.0);
        // Gradient across the strip: accent -> softened accent, alpha ~0.35.
        let color = blend(base, Color32::WHITE, t * 0.5).gamma_multiply(0.35);
        let bar = Rect::from_min_size(Pos2::new(x, rect.bottom() - h), egui::vec2(bar_w, h));
        painter.rect_filled(bar, 2.0, color);
    }
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgb(
        (a.r() as f32 * (1.0 - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1.0 - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1.0 - t) + b.b() as f32 * t) as u8,
    )
}

/// Thin baseline stroke used under the now-playing bar (optional).
#[allow(dead_code)]
pub fn divider(painter: &egui::Painter, rect: Rect, color: Color32) {
    painter.hline(
        rect.x_range(),
        rect.top(),
        Stroke::new(1.0_f32, color.gamma_multiply(0.3)),
    );
}
