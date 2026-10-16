use serde::{Deserialize, Serialize};

/// A radio station as returned by the radio-browser.info API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Station {
    #[serde(rename = "stationuuid")]
    pub uuid: String,
    pub name: String,
    /// Resolved, playable stream URL.
    #[serde(rename = "url_resolved")]
    pub stream_url: String,
    pub homepage: String,
    pub favicon: String,
    pub codec: String,
    pub bitrate: u32,
    pub votes: u32,
    #[serde(rename = "clickcount")]
    pub click_count: u32,
    pub country: String,
    pub language: String,
    pub tags: String,
    /// 1 when radio-browser's last stream check succeeded. Defaults to 1 for
    /// records saved by older versions (and the C# app), which never stored it.
    #[serde(rename = "lastcheckok", default = "one")]
    pub last_check_ok: u8,
}

fn one() -> u8 {
    1
}

impl Station {
    /// "Country · Language" subtitle line.
    pub fn meta_line(&self) -> String {
        [self.country.trim(), self.language.trim()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// "MP3 · 128 kbps · 42 votes" quality line.
    pub fn quality_line(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.codec.trim().is_empty() {
            parts.push(self.codec.trim().to_uppercase());
        }
        if self.bitrate > 0 {
            parts.push(format!("{} kbps", self.bitrate));
        }
        if self.votes > 0 {
            parts.push(format!("{} votes", self.votes));
        }
        parts.join(" · ")
    }

    /// Meta and quality merged into one compact card subtitle.
    pub fn summary_line(&self) -> String {
        let meta = self.meta_line();
        let quality = self.quality_line();
        match (meta.is_empty(), quality.is_empty()) {
            (true, true) => String::new(),
            (true, false) => quality,
            (false, true) => meta,
            (false, false) => format!("{meta} · {quality}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tag {
    pub name: String,
    #[serde(rename = "stationcount")]
    pub station_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Country {
    pub name: String,
    #[serde(rename = "stationcount")]
    pub station_count: u32,
}
