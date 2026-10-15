//! Light/dark visuals plus the 9 accent colours (8 presets + follow system).

use bwr_core::settings::{Accent, Theme};
use eframe::egui::{self, Color32};

pub const SYSTEM_ACCENT_FALLBACK: [u8; 3] = [0x00, 0x78, 0xD4];

/// Resolve the accent to a concrete colour (system accent isn't directly
/// readable without extra crates, so System uses the Windows-blue fallback).
pub fn accent_color(accent: Accent) -> Color32 {
    let rgb = accent.rgb().unwrap_or(SYSTEM_ACCENT_FALLBACK);
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub fn is_dark(ctx: &egui::Context, theme: Theme) -> bool {
    match theme {
        Theme::Dark => true,
        Theme::Light => false,
        Theme::System => ctx
            .system_theme()
            .map(|t| matches!(t, egui::Theme::Dark))
            .unwrap_or(true),
    }
}

/// egui 0.31 tracks a theme *preference* (Dark/Light/System) and re-applies the
/// system theme automatically, so we must set the preference (not just visuals)
/// and then tint both stored styles with the accent.
pub fn apply(ctx: &egui::Context, theme: Theme, accent: Accent) {
    let preference = match theme {
        Theme::Dark => egui::ThemePreference::Dark,
        Theme::Light => egui::ThemePreference::Light,
        Theme::System => egui::ThemePreference::System,
    };
    ctx.set_theme(preference);

    let a = accent_color(accent);
    ctx.all_styles_mut(|style| {
        style.visuals.selection.bg_fill = a;
        style.visuals.hyperlink_color = a;
        style.visuals.widgets.active.bg_fill = a;
        style.visuals.widgets.active.weak_bg_fill = a;
        style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    });
}

/// Fill for the chrome bars (nav, search, player, info panel): on Windows 11
/// a near-solid tinted material colour, elsewhere the plain window fill.
pub fn chrome_fill(ctx: &egui::Context, theme: Theme) -> Color32 {
    if !win11() {
        return ctx.style().visuals.window_fill();
    }
    if is_dark(ctx, theme) {
        Color32::from_rgb(30, 31, 37)
    } else {
        Color32::from_rgb(247, 248, 251)
    }
}

#[cfg(windows)]
fn win11() -> bool {
    crate::backdrop::supported()
}

#[cfg(not(windows))]
fn win11() -> bool {
    false
}

/// Public gate for the painted Windows 11 material.
pub fn win11_material() -> bool {
    win11()
}

/// A slightly softer accent for hover tints / spectrum gradients.
pub fn accent_soft(accent: Accent) -> Color32 {
    let c = accent_color(accent);
    Color32::from_rgba_premultiplied(c.r(), c.g(), c.b(), 28)
}

/// Paint the Mica-style material: a soft vertical gradient, slightly lighter
/// at the top with a whisper of the accent colour — the same visual language
/// as the Windows 11 Mica backdrop, rendered by egui instead of DWM.
/// Painted inside the central panel (behind the station cards) and under the
/// chrome bars via `chrome_fill`; pass the target rect explicitly.
pub fn paint_material(
    ctx: &egui::Context,
    theme: Theme,
    accent: Accent,
    rect: egui::Rect,
) {
    let dark = is_dark(ctx, theme);
    let top = if dark {
        Color32::from_rgb(38, 39, 46)
    } else {
        Color32::from_rgb(250, 250, 253)
    };
    let bottom = if dark {
        Color32::from_rgb(23, 24, 29)
    } else {
        Color32::from_rgb(234, 236, 242)
    };
    // Mica's signature: a faint tinted highlight where the light lands.
    let top = top.lerp_to_gamma(accent_color(accent), if dark { 0.05 } else { 0.06 });
    let mid = top.lerp_to_gamma(bottom, 0.55);

    let mid_y = (rect.top() + rect.bottom()) / 2.0;
    // 6 vertices: top edge, mid line, bottom edge; two quads between them
    // keep the falloff soft (top→mid, mid→bottom).
    let positions = [
        rect.left_top(),
        rect.right_top(),
        egui::pos2(rect.left(), mid_y),
        egui::pos2(rect.right(), mid_y),
        rect.left_bottom(),
        rect.right_bottom(),
    ];
    let colors = [top, top, mid, mid, bottom, bottom];
    let mut mesh = egui::Mesh::default();
    for (pos, color) in positions.iter().zip(colors.iter()) {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *pos,
            uv: egui::epaint::WHITE_UV,
            color: *color,
        });
    }
    mesh.indices = vec![0, 1, 3, 0, 3, 2, 2, 3, 5, 2, 5, 4];

    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.add(egui::Shape::mesh(mesh));
}
