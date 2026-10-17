//! Music-first discovery: mood → tag mappings (editable `moods.json`), the
//! talk-station filter, result ranking and de-duplication, "More like this"
//! scoring and the separate discovery state file (`discovery.json`).
//!
//! `settings.json` stays byte-compatible with the old C# app: everything new
//! lives in `moods.json` (user-editable config) and `discovery.json` (state).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::models::Station;

pub(crate) fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("BigWaveRadio")
}

// ---------------------------------------------------------------------------
// Mood config
// ---------------------------------------------------------------------------

/// One mood chip and the radio-browser tags it covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mood {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: String,
    /// radio-browser tags (matched as substrings server-side by /bytag).
    pub tags: Vec<String>,
    /// Drop results carrying a tag that contains any of these (e.g. trance's
    /// "progressive" also matches progressive rock/metal).
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Mood {
    /// Keep a station found via this mood's tags unless an excluded genre
    /// shows up in its tags.
    pub fn accepts(&self, s: &Station) -> bool {
        let tags = tag_list(&s.tags);
        !self.exclude.iter().any(|x| {
            let x = x.to_lowercase();
            tags.iter().any(|t| t.contains(&x))
        })
    }
}

/// The editable discovery config, stored as `moods.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoodConfig {
    pub moods: Vec<Mood>,
    /// Tags that mark a station as talk/speech.
    pub talk_tags: Vec<String>,
    /// Whole words in a station *name* that mark it as talk.
    pub talk_name_words: Vec<String>,
    /// Tags that clearly mean music (in addition to every mood tag); a
    /// station with at least as many music tags as talk tags is kept.
    pub music_tags: Vec<String>,
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

impl Default for MoodConfig {
    fn default() -> Self {
        let mood = |id: &str, label: &str, icon: &str, tags: &[&str]| Mood {
            id: id.into(),
            label: label.into(),
            icon: icon.into(),
            tags: strs(tags),
            exclude: Vec::new(),
        };
        let rockish = strs(&["metal", "rock", "punk", "prog rock", "jazz fusion"]);
        Self {
            moods: vec![
                Mood { exclude: strs(&["metal", "punk"]), ..mood("dance", "Dance", "🎧", &[
                    "dance", "edm", "house", "eurodance", "club", "deep house",
                    "electro", "dance hits", "hands up", "electronic",
                ]) },
                Mood {
                    exclude: rockish.clone(),
                    ..mood("trance", "Trance", "🌀", &[
                        "trance", "progressive", "uplifting", "psytrance", "psy-trance",
                        "vocal trance", "goa", "hard trance",
                    ])
                },
                mood("chill", "Chill", "🌊", &[
                    "chillout", "chill", "lounge", "ambient", "downtempo", "chillhop",
                    "lofi", "relax", "balearic",
                ]),
                Mood { exclude: strs(&["metal", "punk", "hard rock"]), ..mood("easy", "Easy", "☕", &[
                    "easy listening", "easy", "soft rock", "smooth jazz",
                    "adult contemporary", "soft hits", "smooth", "love songs",
                ]) },
                mood("80s", "80s", "📼", &[
                    "80s", "80's", "eighties", "1980s", "new wave", "synthpop",
                    "synth pop", "italo disco",
                ]),
                mood("90s", "90s", "💿", &[
                    "90s", "90's", "nineties", "1990s", "eurodance", "90s dance",
                ]),
            ],
            talk_tags: strs(&[
                "news", "talk", "talk radio", "news talk", "talk & speech", "speech",
                "sport", "sports", "football", "soccer", "cricket", "podcast", "podcasts",
                "religion", "religious", "christian", "catholic", "bible", "church",
                "sermon", "preaching", "islam", "islamic", "quran", "koran", "spiritual",
                "comedy", "politics", "political", "spoken word", "spoken", "audiobook",
                "audiobooks", "drama", "radio drama", "information", "public affairs",
                "weather", "traffic", "business news", "finance", "debate", "educational",
                "info", "infos", "nachrichten", "noticias", "notizie", "nieuws", "actualités",
                "actualites", "wiadomości", "haberler",
            ]),
            talk_name_words: strs(&[
                "news", "talk", "talksport", "sport", "sports", "podcast", "church",
                "bible", "christian", "catholic", "gospel", "quran", "sermon", "ministry",
                "comedy", "politics", "lbc", "espn", "parliament", "maria", "info", "infos",
                "nachrichten", "noticias", "notizie", "nieuws", "actualités", "haber",
            ]),
            music_tags: strs(&[
                "music", "pop", "hits", "top 40", "top40", "rock", "jazz", "classical",
                "techno", "disco", "rnb", "hip hop", "hiphop", "soul", "funk", "oldies",
                "country", "reggae", "latin", "indie", "alternative", "metal", "blues",
            ]),
        }
    }
}

impl MoodConfig {
    pub fn path() -> PathBuf {
        data_dir().join("moods.json")
    }

    /// Load `moods.json`; write the defaults on first run so the user has a
    /// file to edit. A file that fails to parse is left alone (defaults are
    /// used for this session) so a typo never wipes the user's edits.
    pub fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                eprintln!("moods.json invalid ({e}); using defaults");
                Self::default()
            }),
            Err(_) => {
                let cfg = Self::default();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                    let _ = std::fs::write(&path, json);
                }
                cfg
            }
        }
    }

    pub fn mood(&self, id: &str) -> Option<&Mood> {
        self.moods.iter().find(|m| m.id == id)
    }

    /// Every tag of every mood, deduped (used for "Surprise me").
    pub fn all_mood_tags(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.moods
            .iter()
            .flat_map(|m| m.tags.iter())
            .filter(|t| seen.insert(t.to_lowercase()))
            .cloned()
            .collect()
    }

    /// Is this a talk/speech station? Tag hits must outnumber music tags
    /// (so "dance, hits, news" stays), or the name contains a talk word and
    /// the tags don't clearly say music.
    pub fn is_talk(&self, s: &Station) -> bool {
        let tags = tag_list(&s.tags);
        let talk: HashSet<String> = self.talk_tags.iter().map(|t| t.to_lowercase()).collect();
        let mut music: HashSet<String> = self.music_tags.iter().map(|t| t.to_lowercase()).collect();
        for m in &self.moods {
            music.extend(m.tags.iter().map(|t| t.to_lowercase()));
        }
        let talk_hits = tags.iter().filter(|t| talk.contains(*t)).count();
        let music_hits = tags.iter().filter(|t| music.contains(*t)).count();
        if talk_hits > 0 && talk_hits >= music_hits {
            return true;
        }
        let words = name_words(&s.name);
        let name_hit = self
            .talk_name_words
            .iter()
            .any(|w| words.iter().any(|n| n == &w.to_lowercase()));
        name_hit && music_hits < 2
    }
}

/// Lower-cased, trimmed, non-empty tags of a station.
pub fn tag_list(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

fn name_words(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Ranking + de-duplication
// ---------------------------------------------------------------------------

/// Music-first quality score: working stream dominates, then popularity
/// (votes, clicks, log-scaled) and bitrate.
pub fn quality_score(s: &Station) -> f64 {
    let ok = if s.last_check_ok != 0 { 100.0 } else { 0.0 };
    let votes = (s.votes as f64 + 1.0).ln();
    let clicks = (s.click_count as f64 + 1.0).ln() * 0.8;
    let br = s.bitrate.min(320) as f64 / 320.0 * 2.0;
    ok + votes + clicks + br
}

/// Identity used to spot near-identical stations: the name without
/// punctuation, bitrate/codec/quality markers.
pub fn dedupe_key(name: &str) -> String {
    const NOISE: &[&str] = &[
        "mp3", "aac", "aacplus", "aac+", "ogg", "opus", "flac", "hq", "hd", "lq", "high",
        "low", "quality", "stream", "kbps", "kbit", "k", "64", "96", "128", "160", "192",
        "256", "320", "48", "32",
    ];
    let lower = name.to_lowercase();
    let words: Vec<String> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '+')
        .filter(|w| !w.is_empty())
        .filter(|w| {
            let bare = w.trim_end_matches("kbps").trim_end_matches('k');
            !NOISE.contains(w) && !(bare.len() >= 2 && bare.chars().all(|c| c.is_ascii_digit()) && NOISE.contains(&bare))
        })
        .map(|w| w.to_string())
        .collect();
    if words.is_empty() {
        lower.trim().to_string()
    } else {
        words.join(" ")
    }
}

fn url_key(url: &str) -> String {
    let u = url.trim().to_lowercase();
    let u = u.split_once("://").map(|(_, r)| r.to_string()).unwrap_or(u);
    let u = u.split(['?', '#']).next().unwrap_or("").trim_end_matches('/').to_string();
    u
}

/// Drop near-duplicates (same cleaned name or same stream URL), keeping the
/// best-scoring one, then sort best first. Stable for equal scores.
pub fn rank_and_dedupe(stations: Vec<Station>) -> Vec<Station> {
    rank_by(stations, |_| 0.0)
}

/// Like `rank_and_dedupe` but adds a caller-supplied bonus to each score.
pub fn rank_by(stations: Vec<Station>, bonus: impl Fn(&Station) -> f64) -> Vec<Station> {
    let mut scored: Vec<(f64, usize, Station)> = stations
        .into_iter()
        .enumerate()
        .map(|(i, s)| (quality_score(&s) + bonus(&s), i, s))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    let mut names = HashSet::new();
    let mut urls = HashSet::new();
    let mut uuids = HashSet::new();
    let mut out = Vec::new();
    for (_, _, s) in scored {
        let nk = dedupe_key(&s.name);
        let uk = url_key(&s.stream_url);
        if !uuids.insert(s.uuid.clone()) || !names.insert(nk) {
            continue;
        }
        if !uk.is_empty() && !urls.insert(uk) {
            continue;
        }
        out.push(s);
    }
    out
}

// ---------------------------------------------------------------------------
// More like this / taste profile
// ---------------------------------------------------------------------------

/// Tag → preference weight learned from the user's star ratings: each rated
/// station adds (stars − 2.5) to each of its tags, so 4–5★ pull a tag up and
/// 1–2★ push it down.
pub fn taste_profile<'a>(rated: impl IntoIterator<Item = (u8, &'a Station)>) -> HashMap<String, f64> {
    let mut prof: HashMap<String, f64> = HashMap::new();
    for (stars, s) in rated {
        let w = stars as f64 - 2.5;
        for t in tag_list(&s.tags) {
            *prof.entry(t).or_default() += w;
        }
    }
    prof
}

/// The seed's most useful tags for a "More like this" query (up to `n`),
/// talk/generic tags skipped, preferring ones the user likes.
pub fn seed_tags(seed: &Station, cfg: &MoodConfig, profile: &HashMap<String, f64>, n: usize) -> Vec<String> {
    let skip: HashSet<String> = cfg.talk_tags.iter().map(|t| t.to_lowercase()).collect();
    let generic = ["music", "radio", "fm", "am", "internet radio", "online", "webradio", "hits"];
    let mut tags: Vec<(usize, String)> = tag_list(&seed.tags)
        .into_iter()
        .enumerate()
        .filter(|(_, t)| !skip.contains(t) && !generic.contains(&t.as_str()) && t.len() > 1)
        .collect();
    tags.sort_by(|a, b| {
        let pa = profile.get(&a.1).copied().unwrap_or(0.0);
        let pb = profile.get(&b.1).copied().unwrap_or(0.0);
        pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
    });
    tags.into_iter().map(|(_, t)| t).take(n).collect()
}

/// Similarity bonus of `cand` to `seed`: shared tags (each worth 3, plus the
/// user's taste weight for that tag), the user's own rating of the candidate,
/// and the taste weight of its other tags.
pub fn similarity(seed: &Station, cand: &Station, profile: &HashMap<String, f64>, cand_stars: u8) -> f64 {
    let seed_tags: HashSet<String> = tag_list(&seed.tags).into_iter().collect();
    let mut score = 0.0;
    for t in tag_list(&cand.tags) {
        let pref = profile.get(&t).copied().unwrap_or(0.0).clamp(-6.0, 6.0);
        if seed_tags.contains(&t) {
            score += 3.0 + pref;
        } else {
            score += pref * 0.3;
        }
    }
    if cand_stars > 0 {
        score += (cand_stars as f64 - 2.5) * 4.0;
    }
    score
}

// ---------------------------------------------------------------------------
// Surprise picking
// ---------------------------------------------------------------------------

/// Tiny xorshift so we need no rand dependency.
pub struct Rng(u64);

impl Rng {
    pub fn seeded() -> Self {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9);
        Self(t | 1)
    }
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

/// Pick a surprise from a ranked candidate list: talk and broken streams are
/// skipped, candidates rated ≤2★ are never picked, and the choice is biased
/// toward the top (the list is already best-first).
pub fn pick_surprise(
    ranked: &[Station],
    cfg: &MoodConfig,
    stars: impl Fn(&str) -> u8,
    rng: &mut Rng,
) -> Option<Station> {
    let pool: Vec<&Station> = ranked
        .iter()
        .filter(|s| s.last_check_ok != 0 && !cfg.is_talk(s))
        .filter(|s| {
            let r = stars(&s.uuid);
            r == 0 || r >= 3
        })
        .take(40)
        .collect();
    if pool.is_empty() {
        return None;
    }
    // Min of two draws biases toward the front of the list.
    let i = rng.below(pool.len()).min(rng.below(pool.len()));
    Some(pool[i].clone())
}

// ---------------------------------------------------------------------------
// Discovery state (discovery.json)
// ---------------------------------------------------------------------------

pub const MAX_RECENT: usize = 30;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryState {
    /// Show talk/news/sport stations (hidden by default).
    pub show_talk: bool,
    /// Recently played, newest first.
    pub recent: Vec<Station>,
    /// Station cards in the roomier pre-0.9.1 size (default: compact).
    pub comfortable_cards: bool,
    #[serde(skip)]
    path: PathBuf,
}

impl DiscoveryState {
    pub fn load() -> Self {
        Self::load_from(data_dir().join("discovery.json"))
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

    /// Move/insert a station at the top of Recently played.
    pub fn note_played(&mut self, s: &Station) {
        self.recent.retain(|r| r.uuid != s.uuid);
        self.recent.insert(0, s.clone());
        self.recent.truncate(MAX_RECENT);
        self.save();
    }
}
