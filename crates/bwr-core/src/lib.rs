//! Core types for the BigWave Radio workspace.

pub mod client;
pub mod discovery;
pub mod geo;
pub mod library;
pub mod models;
pub mod paths;
pub mod ratings;
pub mod settings;
pub mod ui_prefs;
pub mod update;

pub use client::RadioBrowserClient;
pub use client::Ipv4OnlyResolver;
pub use library::Library;
pub use models::{Country, Station, Tag};
pub use ratings::Ratings;
pub use discovery::{DiscoveryState, Mood, MoodConfig};
pub use settings::{Accent, Settings};
pub use ui_prefs::UiPrefs;
