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

/// A slightly softer accent for hover tints / spectrum gradients.
pub fn accent_soft(accent: Accent) -> Color32 {
    let c = accent_color(accent);
    Color32::from_rgba_premultiplied(c.r(), c.g(), c.b(), 28)
}
