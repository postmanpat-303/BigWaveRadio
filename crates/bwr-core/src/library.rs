use std::path::PathBuf;

use crate::models::Station;

fn data_dir() -> PathBuf {
    // %LOCALAPPDATA%\BigWaveRadio on Windows, same location the C# app used.
    crate::paths::data_dir()
}

/// The user's saved stations, persisted as library.json in LocalAppData.
#[derive(Debug, Default)]
pub struct Library {
    pub stations: Vec<Station>,
    path: PathBuf,
}

impl Library {
    pub fn load() -> Self {
        let path = data_dir().join("library.json");
        let stations = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { stations, path }
    }

    pub fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.stations) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    pub fn contains(&self, uuid: &str) -> bool {
        self.stations.iter().any(|s| s.uuid == uuid)
    }

    pub fn toggle(&mut self, station: &Station) {
        if let Some(pos) = self.stations.iter().position(|s| s.uuid == station.uuid) {
            self.stations.remove(pos);
        } else {
            self.stations.push(station.clone());
        }
        self.save();
    }
}
