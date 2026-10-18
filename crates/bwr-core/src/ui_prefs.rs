//! UI preferences the old C# app never had, stored in `ui.json` so that
//! `settings.json` stays exactly as the C# app wrote it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths::data_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    /// UI language: "system" (follow the OS locale), "en", "pt-BR" or "it".
    pub language: String,
    #[serde(skip)]
    path: PathBuf,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self { language: "system".into(), path: PathBuf::new() }
    }
}

impl UiPrefs {
    pub fn path() -> PathBuf {
        data_dir().join("ui.json")
    }

    pub fn load() -> Self {
        Self::load_from(Self::path())
    }

    pub fn load_from(path: PathBuf) -> Self {
        let mut s: Self = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        s.path = path;
        s
    }

    pub fn save(&self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&self.path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_system_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("bwr-ui-prefs-{}", std::process::id()));
        let path = dir.join("ui.json");
        let _ = std::fs::remove_file(&path);
        let mut p = UiPrefs::load_from(path.clone());
        assert_eq!(p.language, "system");
        p.language = "pt-BR".into();
        p.save();
        assert_eq!(UiPrefs::load_from(path.clone()).language, "pt-BR");
        // Unknown fields from a newer version are ignored, missing ones defaulted.
        std::fs::write(&path, r#"{"future": 1}"#).unwrap();
        assert_eq!(UiPrefs::load_from(path).language, "system");
        let _ = std::fs::remove_dir_all(dir);
    }
}
