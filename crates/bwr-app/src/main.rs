// Release builds are GUI-subsystem: no console window behind the app.
// Debug builds keep the console for diagnostics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
#[cfg(windows)]
mod backdrop;
#[cfg(windows)]
mod diag;
#[cfg(windows)]
mod hotkeys;
mod pages;
#[cfg(windows)]
mod smtc;
mod theme;
#[cfg(windows)]
mod tray;
mod widgets;

use app::BigWaveApp;

fn main() -> eframe::Result<()> {
    #[cfg(debug_assertions)]
    eprintln!("[startup] settings: {:?}", bwr_core::Settings::load());
    eframe::run_native(
        "BigWave Radio",
        make_options(),
        Box::new(|cc| Ok(Box::new(BigWaveApp::new(cc)))),
    )
}

fn make_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BigWave Radio")
            .with_app_id("bigwave.radio")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([760.0, 480.0])
            .with_icon(window_icon()),
        ..Default::default()
    }
}

fn window_icon() -> egui::IconData {
    let png = include_bytes!("../../../assets/bigwave-logo.png");
    let image = image::load_from_memory(png)
        .expect("bundled logo decodes")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}
