//! BigWave Radio plugin host.
//!
//! Plugins are folders containing `plugin.toml` + `main.rhai`, living in
//! `%LOCALAPPDATA%\BigWaveRadio\plugins\<id>\` on Windows and
//! `~/Library/Application Support/BigWaveRadio/plugins/<id>/` on macOS. Scripts never touch egui:
//! they return declarative drawing commands ([`DrawCmd`]) and simple widget
//! descriptions ([`Widget`]) which the app renders. See `docs/PLUGIN_GUIDE.md`.

mod api;
mod bundled;
mod check;
mod draw;
pub mod visual;
mod host;
mod hotkeys;
mod manifest;
mod paths;
mod safety;
mod ui;
mod zipimport;

use std::path::PathBuf;

pub use api::Request;
pub use bundled::{install_bundled, BUNDLED};
pub use check::{check_plugin, CheckReport};
pub use visual::{VisualEngine, VisualParams, SCENES};
pub use draw::{is_closed, parse_svg_path, poly_area, triangulate, DrawCmd, Rgba, MAX_RINGS};
pub use host::{migrate_state, AudioFrame, HostState, OverlayPhase, OverlaySession, Plugin, PluginHost, PluginStatus, StationInfo};
pub use hotkeys::{parse_chord, ChordMatcher, ChordResult, HotkeySpec, Mods, Step, CHORD_TIMEOUT};
pub use manifest::{card_tint, Manifest, PageSpec, Permission, SettingKind, SettingSpec};
pub use safety::{frame_luminance, FlashLimiter, FLASH_DELTA};
pub use ui::{UiResult, Widget};
pub use zipimport::import_zip;
pub use paths::app_data_dir;

/// Version of the script API this host implements (`api = 1` in plugin.toml).
pub const API_VERSION: u32 = 1;
/// Default host-enforced flash cap (WCAG 2.3.1 "three flashes" guidance).
pub const DEFAULT_FLASH_HZ: f64 = 3.0;
/// Absolute ceiling, even with the user's opt-in.
pub const HARD_MAX_FLASH_HZ: f64 = 12.0;

/// `<data dir>/plugins`: `%LOCALAPPDATA%\BigWaveRadio\plugins` on Windows,
/// `~/Library/Application Support/BigWaveRadio/plugins` on macOS.
pub fn default_plugins_dir() -> PathBuf {
    app_data_dir().join("plugins")
}
