//! Opt-in file logging for diagnosing issues on user machines.
//!
//! Enabled by setting the `BWR_LOG` environment variable; lines are appended
//! to `%LOCALAPPDATA%\BigWaveRadio\bwr.log`. No-op when unset, so the normal
//! app experience is unchanged.

#![cfg(windows)]

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;

static ENABLED: OnceLock<bool> = OnceLock::new();

pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("BWR_LOG").is_some())
}

pub fn log(line: &str) {
    if !enabled() {
        return;
    }
    let path = dirs_data().join("bwr.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{ts}] {line}");
    }
}

fn dirs_data() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("BigWaveRadio")
}
