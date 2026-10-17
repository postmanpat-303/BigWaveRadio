//! Where BigWave Radio keeps its data (settings, library, ratings, moods,
//! discovery, update state, plugins).
//!
//! * Windows: `%LOCALAPPDATA%\BigWaveRadio`, the folder the original C# app used.
//! * macOS: `~/Library/Application Support/BigWaveRadio`.
//! * Linux/other: `$XDG_DATA_HOME/BigWaveRadio` (usually `~/.local/share`).
//!
//! On every platform an explicit `LOCALAPPDATA` environment variable wins, which
//! keeps Windows unchanged and lets tests point the app at a scratch folder.
//! `bwr_plugins::app_data_dir` mirrors this function (that crate doesn't depend
//! on bwr-core); a test in bwr-app checks they agree.

use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    base_dir().join("BigWaveRadio")
}

fn base_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    #[cfg(target_os = "macos")]
    let platform = dirs::data_dir(); // ~/Library/Application Support
    #[cfg(not(target_os = "macos"))]
    let platform = if cfg!(windows) { None } else { dirs::data_local_dir() };
    platform.unwrap_or_else(|| PathBuf::from("."))
}
