//! Per-station user ratings (0–5 stars), persisted as ratings.json in the
//! same data directory as the library and settings.
//!
//! The file holds two maps: `ratings` (uuid → stars) and `stations` (uuid →
//! station record, cached so the Rated list renders without a network
//! round-trip). Records missing from `stations` are fetched from
//! radio-browser when the Rated list opens. Files written by v0.4 (a flat
//! uuid → stars map) still load; those entries simply have no cached record
//! until they are re-fetched.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::models::Station;

#[derive(Debug, Default)]
pub struct Ratings {
    map: HashMap<String, u8>,
    stations: HashMap<String, Station>,
    path: PathBuf,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum File {
    /// v0.4 layout: flat uuid → stars.
    Old(HashMap<String, u8>),
    New {
        ratings: HashMap<String, u8>,
        #[serde(default)]
        stations: HashMap<String, Station>,
    },
}

#[derive(Serialize)]
struct FileOut<'a> {
    ratings: &'a HashMap<String, u8>,
    stations: &'a HashMap<String, Station>,
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
        let parsed = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<File>(&text).ok());
        let (map, stations) = match parsed {
            Some(File::Old(m)) => (m, HashMap::new()),
            Some(File::New { ratings, stations }) => (ratings, stations),
            None => (HashMap::new(), HashMap::new()),
        };
        Self { map, stations, path }
    }

    pub fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let out = FileOut { ratings: &self.map, stations: &self.stations };
        if let Ok(json) = serde_json::to_string_pretty(&out) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    /// Stars for a station (0 = unrated).
    pub fn get(&self, uuid: &str) -> u8 {
        self.map.get(uuid).copied().unwrap_or(0)
    }

    /// Set the rating; 0 removes the entry and its cached record.
    /// Returns the new value.
    pub fn set(&mut self, uuid: &str, rating: u8) -> u8 {
        let rating = rating.min(5);
        if rating == 0 {
            self.map.remove(uuid);
            self.stations.remove(uuid);
        } else {
            self.map.insert(uuid.to_string(), rating);
        }
        self.save();
        rating
    }

    /// Cache a station record for a rated uuid (called when rating from the
    /// info panel and when fetch results arrive).
    pub fn record(&mut self, station: &Station) {
        if self.map.contains_key(&station.uuid) {
            self.stations.insert(station.uuid.clone(), station.clone());
            self.save();
        }
    }

    /// Rated stations with their stars, best first, name A→Z within a level.
    /// Entries without a cached record are skipped (they are being fetched).
    pub fn rated_stations(&self) -> Vec<(u8, Station)> {
        let mut out: Vec<(u8, Station)> = self
            .map
            .iter()
            .filter_map(|(uuid, &r)| self.stations.get(uuid).map(|s| (r, s.clone())))
            .collect();
        out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase())));
        out
    }

    /// Rated uuids that have no cached station record yet.
    pub fn missing_records(&self) -> Vec<String> {
        self.map
            .keys()
            .filter(|u| !self.stations.contains_key(*u))
            .cloned()
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// How many stations carry exactly `r` stars.
    pub fn count(&self, r: u8) -> usize {
        self.map.values().filter(|&&v| v == r).count()
    }
}
