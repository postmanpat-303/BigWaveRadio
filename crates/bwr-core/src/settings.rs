use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The 8 highlight colours from the WinUI app, as (key, display name, rgb).
pub const ACCENTS: &[(&str, &str, [u8; 3])] = &[
    ("ocean", "Ocean blue", [0x00, 0x78, 0xD4]),
    ("teal", "Teal", [0x03, 0x83, 0x87]),
    ("forest", "Forest green", [0x10, 0x7C, 0x10]),
    ("gold", "Gold", [0xC1, 0x9C, 0x00]),
    ("ember", "Ember orange", [0xCA, 0x50, 0x10]),
    ("ruby", "Ruby red", [0xD1, 0x34, 0x38]),
    ("pink", "Hot pink", [0xE3, 0x00, 0x8C]),
    ("violet", "Violet", [0x74, 0x4D, 0xA9]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    System,
    Ocean,
    Teal,
    Forest,
    Gold,
    Ember,
    Ruby,
    Pink,
    Violet,
}

impl Accent {
    pub fn rgb(self) -> Option<[u8; 3]> {
        match self {
            Accent::System => None,
            Accent::Ocean => Some([0x00, 0x78, 0xD4]),
            Accent::Teal => Some([0x03, 0x83, 0x87]),
            Accent::Forest => Some([0x10, 0x7C, 0x10]),
            Accent::Gold => Some([0xC1, 0x9C, 0x00]),
            Accent::Ember => Some([0xCA, 0x50, 0x10]),
            Accent::Ruby => Some([0xD1, 0x34, 0x38]),
            Accent::Pink => Some([0xE3, 0x00, 0x8C]),
            Accent::Violet => Some([0x74, 0x4D, 0xA9]),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    pub theme: Theme,
    pub accent: Accent,
    /// Minimum stream bitrate in kbps, 0 = any.
    pub min_bitrate: u32,
    /// Minimum user rating (stars), 0 = any.
    pub min_rating: u8,
    /// Stream codec filter, empty = any (MP3, AAC, OGG, FLAC, OPUS…).
    pub codec: String,
    pub volume: f32,
    /// Hide the window to the tray on close instead of exiting.
    pub minimize_to_tray: bool,
    /// Recent search queries, most recent first (capped at MAX_SEARCH_HISTORY).
    #[serde(default)]
    pub search_history: Vec<String>,
}

/// Cap on the number of remembered searches.
pub const MAX_SEARCH_HISTORY: usize = 8;

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            accent: Accent::System,
            min_bitrate: 0,
            min_rating: 0,
            codec: String::new(),
            volume: 0.8,
            minimize_to_tray: false,
            search_history: Vec::new(),
        }
    }
}

fn path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("BigWaveRadio").join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(parent) = path().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path(), json);
        }
    }
}
