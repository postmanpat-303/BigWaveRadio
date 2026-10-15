//! Per-station user ratings (0–5 stars), persisted as ratings.json in the
//! same data directory as the library and settings.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct Ratings {
    map: HashMap<String, u8>,
    path: PathBuf,
}

fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("BigWaveRadio")
}

impl Ratings {
    pub fn load() -> Self {
        let path = data_dir().join("ratings.json");
        let map = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { map, path }
    }

    pub fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.map) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    /// Stars for a station (0 = unrated).
    pub fn get(&self, uuid: &str) -> u8 {
        self.map.get(uuid).copied().unwrap_or(0)
    }

    /// Set the rating; 0 removes the entry. Returns the new value.
    pub fn set(&mut self, uuid: &str, rating: u8) -> u8 {
        let rating = rating.min(5);
        if rating == 0 {
            self.map.remove(uuid);
        } else {
            self.map.insert(uuid.to_string(), rating);
        }
        self.save();
        rating
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}
