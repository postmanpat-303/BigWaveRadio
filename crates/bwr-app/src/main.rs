// Release builds are GUI-subsystem: no console window behind the app.
// Debug builds keep the console for diagnostics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
mod i18n;
mod rtl;
mod app;
#[cfg(windows)]
mod backdrop;
#[cfg(windows)]
mod diag;
#[cfg(windows)]
mod hotkeys;
mod pages;
mod search_history;
mod plugin_bridge;
#[cfg(windows)]
mod smtc;
mod theme;
#[cfg(windows)]
mod tray;
mod update_ui;
mod widgets;

use app::BigWaveApp;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // `--version`: print "BigWave Radio <ver>" and exit (CI smoke test).
    if args.iter().any(|a| a == "--version" || a == "-V") {
        attach_parent_console();
        println!("BigWave Radio {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    // `--check-plugin <dir>`: validate a plugin and exit (no window).
    if let Some(i) = args.iter().position(|a| a == "--check-plugin") {
        std::process::exit(check_plugin_cli(args.get(i + 1)));
    }
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
            // Wayland app_id / X11 WM_CLASS: matches bigwave-radio.desktop on Linux.
            .with_app_id(if cfg!(target_os = "linux") { "bigwave-radio" } else { "bigwave.radio" })
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([760.0, 480.0])
            .with_icon(window_icon()),
        ..Default::default()
    }
}

fn window_icon() -> egui::IconData {
    let png = include_bytes!("../../../assets/bigwave-icon.png");
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

/// Release builds use the GUI subsystem; reattach to the calling console so
/// CLI output is visible when run from a terminal.
fn attach_parent_console() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Prints a report and returns the process exit code (0 = OK, 1 = problems,
/// 2 = usage error).
fn check_plugin_cli(dir: Option<&String>) -> i32 {
    attach_parent_console();
    let Some(dir) = dir else {
        eprintln!("usage: BigWaveRadio --check-plugin <plugin folder>");
        return 2;
    };
    let report = bwr_plugins::check_plugin(std::path::Path::new(dir));
    print!("{report}");
    if report.ok() {
        0
    } else {
        1
    }
}
