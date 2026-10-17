//! Same rules as `bwr_core::paths::data_dir` (kept in sync; see that file):
//! `LOCALAPPDATA` if set (always on Windows), else `~/Library/Application Support`
//! on macOS, else the XDG data dir, then `BigWaveRadio`.

use std::path::PathBuf;

pub fn app_data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            #[cfg(target_os = "macos")]
            {
                dirs::data_dir()
            }
            #[cfg(not(target_os = "macos"))]
            {
                if cfg!(windows) {
                    None
                } else {
                    dirs::data_local_dir()
                }
            }
        })
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("BigWaveRadio")
}
