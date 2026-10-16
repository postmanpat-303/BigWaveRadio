//! `plugin.toml` parsing and validation.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::API_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Permission {
    /// Receive `on_audio` (levels, bands, beat, BPM).
    Audio,
    /// Receive `on_station_change`, `on_meta`, `on_player`.
    NowPlaying,
    /// Stop / play-pause / next / previous / volume.
    PlayerControl,
    /// Draw an overlay over the app window (`on_overlay`).
    Overlay,
    /// Make the overlay fullscreen.
    Fullscreen,
    /// Receive key presses while the overlay is open (`on_key`).
    Hotkeys,
    /// Draw a panel on the Plugins page (`on_panel`).
    Panel,
    /// Add whole pages to the side navigation (`on_page`).
    Page,
    /// Persist small key/value data (`ctx.store_get/store_set`).
    Storage,
}

impl Permission {
    pub const ALL: [Permission; 9] = [
        Permission::Audio,
        Permission::NowPlaying,
        Permission::PlayerControl,
        Permission::Overlay,
        Permission::Fullscreen,
        Permission::Hotkeys,
        Permission::Panel,
        Permission::Page,
        Permission::Storage,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Permission::Audio => "audio",
            Permission::NowPlaying => "now_playing",
            Permission::PlayerControl => "player_control",
            Permission::Overlay => "overlay",
            Permission::Fullscreen => "fullscreen",
            Permission::Hotkeys => "hotkeys",
            Permission::Panel => "panel",
            Permission::Page => "page",
            Permission::Storage => "storage",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Permission::Audio => "read audio levels and beats",
            Permission::NowPlaying => "see the station and song playing",
            Permission::PlayerControl => "control playback and volume",
            Permission::Overlay => "draw over the app window",
            Permission::Fullscreen => "go fullscreen",
            Permission::Hotkeys => "use keyboard shortcuts",
            Permission::Panel => "show a panel on the Plugins page",
            Permission::Page => "add pages to the side bar",
            Permission::Storage => "save small amounts of data",
        }
    }

    pub fn parse(s: &str) -> Result<Permission, String> {
        Permission::ALL
            .into_iter()
            .find(|p| p.name() == s)
            .ok_or_else(|| match s {
                "net" | "network" => "permission \"net\" is not available: plugins have no network access in API v1".to_string(),
                "fs" | "filesystem" => "permission \"fs\" is not available: plugins have no file access in API v1".to_string(),
                _ => format!(
                    "unknown permission \"{s}\" (known: {})",
                    Permission::ALL.iter().map(|p| p.name()).collect::<Vec<_>>().join(", ")
                ),
            })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSpec {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingKind {
    Float,
    Int,
    Bool,
    Color,
    String,
    Choice,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingSpec {
    #[serde(rename = "type")]
    pub kind: SettingKind,
    pub default: toml::Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub label: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: u32,
    #[serde(default = "default_runtime")]
    pub runtime: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub pages: Vec<PageSpec>,
    #[serde(default)]
    pub settings: BTreeMap<String, SettingSpec>,
    /// App-wide hotkeys (needs the "hotkeys" permission); fire `on_hotkey`.
    #[serde(default)]
    pub hotkeys: Vec<crate::HotkeySpec>,
}

fn default_runtime() -> String {
    "rhai".into()
}
fn default_entry() -> String {
    "main.rhai".into()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

impl Manifest {
    /// Parse and validate. Returns every problem found, not just the first.
    pub fn parse(src: &str) -> Result<Manifest, Vec<String>> {
        let m: Manifest = toml::from_str(src).map_err(|e| vec![format!("plugin.toml: {e}")])?;
        let errs = m.validate();
        if errs.is_empty() {
            Ok(m)
        } else {
            Err(errs)
        }
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errs = Vec::new();
        if !valid_id(&self.id) {
            errs.push(format!("id \"{}\" must be 1-40 chars of a-z, 0-9, '-' or '_'", self.id));
        }
        if self.name.trim().is_empty() {
            errs.push("name must not be empty".into());
        }
        if self.api == 0 || self.api > API_VERSION {
            errs.push(format!("api = {} is not supported (this host supports api = {API_VERSION})", self.api));
        }
        if self.runtime != "rhai" {
            errs.push(format!("runtime \"{}\" is not supported (only \"rhai\")", self.runtime));
        }
        if self.entry.contains(['/', '\\']) || self.entry.contains("..") || !self.entry.ends_with(".rhai") {
            errs.push(format!("entry \"{}\" must be a .rhai file name in the plugin folder", self.entry));
        }
        for p in &self.permissions {
            if let Err(e) = Permission::parse(p) {
                errs.push(e);
            }
        }
        let perms = self.permission_set();
        if perms.contains(&Permission::Fullscreen) && !perms.contains(&Permission::Overlay) {
            errs.push("permission \"fullscreen\" also needs \"overlay\"".into());
        }
        if !self.pages.is_empty() && !perms.contains(&Permission::Page) {
            errs.push("[[pages]] are declared but the \"page\" permission is missing".into());
        }
        if !self.hotkeys.is_empty() && !perms.contains(&Permission::Hotkeys) {
            errs.push("[[hotkeys]] are declared but the \"hotkeys\" permission is missing".into());
        }
        for h in &self.hotkeys {
            if let Err(e) = crate::parse_chord(&h.keys) {
                errs.push(e);
            }
            if h.action.trim().is_empty() {
                errs.push(format!("hotkey \"{}\" needs an action", h.keys));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for p in &self.pages {
            if !valid_id(&p.id) {
                errs.push(format!("page id \"{}\" must be 1-40 chars of a-z, 0-9, '-' or '_'", p.id));
            }
            if !seen.insert(p.id.clone()) {
                errs.push(format!("duplicate page id \"{}\"", p.id));
            }
            if p.title.trim().is_empty() {
                errs.push(format!("page \"{}\" needs a title", p.id));
            }
        }
        for (key, s) in &self.settings {
            let ok = match s.kind {
                SettingKind::Float => s.default.as_float().is_some() || s.default.as_integer().is_some(),
                SettingKind::Int => s.default.as_integer().is_some(),
                SettingKind::Bool => s.default.as_bool().is_some(),
                SettingKind::Color => s.default.as_str().is_some_and(|c| crate::Rgba::parse(c).is_some()),
                SettingKind::String => s.default.as_str().is_some(),
                SettingKind::Choice => s.default.as_str().is_some_and(|d| s.options.iter().any(|o| o == d)),
            };
            if !ok {
                errs.push(format!("setting \"{key}\": default {} does not match type {:?}", s.default, s.kind));
            }
            if let (Some(lo), Some(hi)) = (s.min, s.max) {
                if lo > hi {
                    errs.push(format!("setting \"{key}\": min > max"));
                }
            }
        }
        errs
    }

    pub fn permission_set(&self) -> std::collections::BTreeSet<Permission> {
        self.permissions.iter().filter_map(|p| Permission::parse(p).ok()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r##"
id = "demo"
name = "Demo"
version = "0.1.0"
api = 1
permissions = ["audio", "overlay", "page"]
[[pages]]
id = "main"
title = "Demo page"
[settings]
speed = { type = "float", default = 2.0, min = 0.5, max = 4.0 }
tint = { type = "color", default = "#ff0000" }
"##;

    #[test]
    fn parses_good_manifest() {
        let m = Manifest::parse(GOOD).unwrap();
        assert_eq!(m.entry, "main.rhai");
        assert_eq!(m.pages.len(), 1);
        assert!(m.permission_set().contains(&Permission::Audio));
    }

    #[test]
    fn rejects_net_and_bad_fields() {
        let bad = GOOD.replace("\"audio\"", "\"net\"").replace("api = 1", "api = 9").replace("\"demo\"", "\"Bad Id\"");
        let errs = Manifest::parse(&bad).unwrap_err();
        let all = errs.join("\n");
        assert!(all.contains("network"), "{all}");
        assert!(all.contains("api = 9"), "{all}");
        assert!(all.contains("Bad Id"), "{all}");
    }

    #[test]
    fn pages_need_permission() {
        let bad = GOOD.replace(", \"page\"", "");
        assert!(Manifest::parse(&bad).unwrap_err().join("").contains("page"));
    }
}
