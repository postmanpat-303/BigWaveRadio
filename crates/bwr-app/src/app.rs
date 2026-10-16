//! Application state and the eframe::App implementation.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bwr_core::models::{Country, Station, Tag};
use bwr_core::{Library, RadioBrowserClient, Settings};
use bwr_player::{Player, PlayerEvent};
use bwr_spectrum::BANDS;
use eframe::egui::{self, Context};

use crate::pages;
use crate::theme;
use crate::widgets::now_playing_bar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Discover,
    Search,
    Library,
    Tags,
    Countries,
    Settings,
    Plugins,
    About,
    /// A page added by a plugin (index into `plugins.pages`).
    PluginPage(usize),
    /// Mood chip results (index into `moods.moods`).
    Mood(usize),
    /// Your Stars: every rated station, across all categories.
    Stars,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverTab {
    Trending,
    Popular,
    Recent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryTab {
    Favourites,
    Recent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowseSel {
    Tag(String),
    Country(String),
}

#[derive(Debug)]
pub enum NetJob {
    Discover(DiscoverTab),
    Search(String),
    Tags,
    Countries,
    StationsFor(BrowseSel),
    /// Stations sharing a tag, excluding one uuid (station-info tag chips).
    MoreLike { tag: String, exclude: String },
    /// ✨ More like this: stations sharing the seed's tags, weighted by the
    /// user's ratings.
    MoreLikeStation {
        seed: Station,
        tags: Vec<String>,
        profile: std::collections::HashMap<String, f64>,
        stars: std::collections::HashMap<String, u8>,
    },
    /// Stations for a mood chip (all of the mood's tags, ranked).
    Mood { idx: usize, mood: bwr_core::Mood },
    /// 🎲 Surprise me within the moods: maybe a well-rated favourite, else
    /// a popular working station from a random mood tag.
    Surprise {
        tags: Vec<String>,
        rated_good: Vec<Station>,
        stars: std::collections::HashMap<String, u8>,
        cfg: Box<bwr_core::MoodConfig>,
    },
    Click(String),
    /// Fetch full records for rated uuids (the Rated smart list).
    Rated(Vec<String>),
    Favicon(String, String),
}

pub enum NetResult {
    Stations(Vec<Station>),
    /// Result of a "More like this" request, with the tag it was for.
    MoreLike(String, Vec<Station>),
    /// Ranked stations for mood chip `idx`.
    Mood(usize, Vec<Station>),
    Surprise(Station),
    /// Full records for rated uuids, cached into the ratings store.
    Rated(Vec<Station>),
    Tags(Vec<Tag>),
    Countries(Vec<Country>),
    Favicon(String, Vec<u8>),
    Failed(String),
}

/// Quality filter shared with the network worker: minimum bitrate + codec.
pub struct StreamFilter {
    pub min_bitrate: AtomicU32,
    pub codec: std::sync::Mutex<String>,
}

impl StreamFilter {
    fn snapshot(&self) -> (u32, String) {
        (
            self.min_bitrate.load(Ordering::Relaxed),
            self.codec.lock().map(|c| c.clone()).unwrap_or_default(),
        )
    }
}

pub struct BigWaveApp {
    pub settings: Settings,
    pub library: Library,
    pub ratings: bwr_core::Ratings,
    /// Mood chips + talk filter config (editable moods.json).
    pub moods: bwr_core::MoodConfig,
    /// Show-talk toggle + Recently played (discovery.json; settings.json is
    /// left exactly as the C# app wrote it).
    pub discovery: bwr_core::DiscoveryState,
    /// Your Stars level filter: 0 = all rated, sorted by stars.
    pub stars_level: u8,
    /// The rail's Advanced (format / bitrate) section is expanded.
    pub advanced_open: bool,
    /// Rated uuids already requested from the network (dedupe fetches).
    pub rated_inflight: std::collections::HashSet<String>,
    pub player: Player,

    pub page: Page,
    pub discover_tab: DiscoverTab,
    pub library_tab: LibraryTab,
    pub stations: Vec<Station>,
    pub current: Option<Station>,
    pub status: String,
    /// ICY "Artist - Song" from the live stream, when the station provides it.
    pub song_title: Option<String>,
    pub is_playing: bool,
    pub loading: bool,

    pub tags: Vec<Tag>,
    pub countries: Vec<Country>,
    pub browse_selection: Option<BrowseSel>,

    // Search
    pub query: String,
    pub search_active: bool,
    /// A `NetJob::Search` is in flight; its result (not a Discover/browse
    /// list that happened to arrive while the Search page is open) is what
    /// flips `search_active` on.
    pub search_pending: bool,
    pub last_keystroke: Option<Instant>,
    pub last_searched: String,
    /// "More like this" context: the station name the current search list relates to.
    pub similar_label: Option<String>,
    /// Tag word cloud shown on the idle search page (layout cache).
    pub word_cloud: crate::widgets::word_cloud::WordCloud,

    // Networking
    pub job_tx: mpsc::Sender<NetJob>,
    result_rx: mpsc::Receiver<NetResult>,

    // Visualiser
    pub bands: [f32; BANDS],

    // Station artwork shown in the right-hand info panel, keyed by station uuid.
    pub favicons: std::collections::HashMap<String, egui::TextureHandle>,
    favicon_pending: std::collections::HashSet<String>,
    pub about_logo: egui::TextureHandle,
    /// Square brand icon for the About page in light theme, where the full
    /// logo's silver "RADIO" text would wash out on the light card.
    pub about_logo_icon: egui::TextureHandle,
    pub world_map: egui::TextureHandle,
    pub plugins: crate::plugin_bridge::PluginBridge,

    // Windows integration
    #[cfg(windows)]
    hotkey_rx: mpsc::Receiver<crate::hotkeys::MediaKey>,
    #[cfg(windows)]
    tray: Option<crate::tray::Tray>,
    /// Taskbar thumbnail / media-flyout play controls. Initialised lazily
    /// once the real HWND is known; `smtc_failed` stops retry loops.
    #[cfg(windows)]
    smtc: Option<crate::smtc::Smtc>,
    #[cfg(windows)]
    smtc_failed: bool,
    /// Window currently hidden to the tray.
    #[cfg(windows)]
    hidden_to_tray: bool,
    /// On-screen position to return to when the window comes back.
    #[cfg(windows)]
    restore_pos: Option<(i32, i32)>,
    /// Main window handle, resolved from egui each frame (never guessed by
    /// title — other windows on the machine can share our title).
    #[cfg(windows)]
    hwnd: Option<windows::Win32::Foundation::HWND>,
    /// Tray "Quit" was chosen: the next close request must be honoured.
    #[cfg(windows)]
    quitting: bool,

    ctx: Context,
    filter: Arc<StreamFilter>,
    first_frame: bool,
}

impl BigWaveApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        // Decode the About-page logo here (same path as the window icon, which
        // is proven to work) rather than via include_image!, which depends on
        // optional egui image loaders.
        let about_logo = {
            let png = include_bytes!("../../../assets/bigwave-logo.png");
            let img = image::load_from_memory(png)
                .expect("bundled logo decodes")
                .into_rgba8();
            let (w, h) = img.dimensions();
            cc.egui_ctx.load_texture(
                "about-logo",
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img),
                egui::TextureOptions::LINEAR,
            )
        };
        let about_logo_icon = {
            let png = include_bytes!("../../../assets/bigwave-icon.png");
            let img = image::load_from_memory(png)
                .expect("bundled icon decodes")
                .into_rgba8();
            let (w, h) = img.dimensions();
            cc.egui_ctx.load_texture(
                "about-logo-icon",
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img),
                egui::TextureOptions::LINEAR,
            )
        };
        // Flat world landmask (equirectangular, white-on-transparent) tinted per theme.
        let world_map = {
            let png = include_bytes!("../../../assets/world-map.png");
            let img = image::load_from_memory(png)
                .expect("bundled world map decodes")
                .into_rgba8();
            let (w, h) = img.dimensions();
            cc.egui_ctx.load_texture(
                "world-map",
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img),
                egui::TextureOptions::LINEAR,
            )
        };
        let client = Arc::new(match RadioBrowserClient::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("radio-browser client init failed: {e}");
                // Fall back to retrying until a mirror answers.
                fallback_client()
            }
        });
        let (job_tx, job_rx) = mpsc::channel::<NetJob>();
        let (result_tx, result_rx) = mpsc::channel::<NetResult>();
        let filter = Arc::new(StreamFilter {
            min_bitrate: AtomicU32::new(settings.min_bitrate),
            codec: std::sync::Mutex::new(settings.codec.clone()),
        });
        let filter_worker = filter.clone();
        // Dedicated client for station artwork (favicons) fetched from arbitrary hosts.
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .user_agent("BigWaveRadio/0.1")
            .build()
            .expect("favicon http client builds");

        std::thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                match job {
                    NetJob::Discover(tab) => {
                        let (min, codec) = filter_worker.snapshot();
                        let res = match tab {
                            DiscoverTab::Trending => client.trending(min, &codec),
                            DiscoverTab::Popular => client.popular(min, &codec),
                            DiscoverTab::Recent => client.recently_added(min, &codec),
                        };
                        send_result(&result_tx, res);
                    }
                    NetJob::Search(ref q) => {
                        let (min, codec) = filter_worker.snapshot();
                        let res = client.search(q, min, &codec).map(bwr_core::discovery::rank_and_dedupe);
                        send_result(&result_tx, res);
                    }
                    NetJob::Tags => match client.tags() {
                        Ok(tags) => {
                            let _ = result_tx.send(NetResult::Tags(tags));
                        }
                        Err(e) => {
                            let _ = result_tx.send(NetResult::Failed(e.to_string()));
                        }
                    },
                    NetJob::Countries => match client.countries() {
                        Ok(countries) => {
                            let _ = result_tx.send(NetResult::Countries(countries));
                        }
                        Err(e) => {
                            let _ = result_tx.send(NetResult::Failed(e.to_string()));
                        }
                    },
                    NetJob::StationsFor(ref sel) => {
                        let (min, codec) = filter_worker.snapshot();
                        let res = match sel {
                            BrowseSel::Tag(t) => client.stations_by_tag(t, min, &codec),
                            BrowseSel::Country(c) => client.stations_by_country(c, min, &codec),
                        };
                        send_result(&result_tx, res);
                    }
                    NetJob::MoreLike { ref tag, ref exclude } => {
                        let (min, codec) = filter_worker.snapshot();
                        match client.similar(tag, exclude, min, &codec).map(bwr_core::discovery::rank_and_dedupe) {
                            Ok(stations) => {
                                let _ = result_tx.send(NetResult::MoreLike(tag.clone(), stations));
                            }
                            Err(e) => {
                                let _ = result_tx.send(NetResult::Failed(e.to_string()));
                            }
                        }
                    }
                    NetJob::MoreLikeStation { ref seed, ref tags, ref profile, ref stars } => {
                        let (min, codec) = filter_worker.snapshot();
                        match client.stations_by_tags(tags, 60, min, &codec) {
                            Ok(found) => {
                                let found: Vec<Station> =
                                    found.into_iter().filter(|s| s.uuid != seed.uuid).collect();
                                let ranked = bwr_core::discovery::rank_by(found, |c| {
                                    bwr_core::discovery::similarity(
                                        seed,
                                        c,
                                        profile,
                                        stars.get(&c.uuid).copied().unwrap_or(0),
                                    )
                                });
                                // Drop the seed's own duplicates (same cleaned name).
                                let key = bwr_core::discovery::dedupe_key(&seed.name);
                                let ranked = ranked
                                    .into_iter()
                                    .filter(|s| bwr_core::discovery::dedupe_key(&s.name) != key)
                                    .take(120)
                                    .collect();
                                let _ = result_tx.send(NetResult::MoreLike(tags.join(", #"), ranked));
                            }
                            Err(e) => {
                                let _ = result_tx.send(NetResult::Failed(e.to_string()));
                            }
                        }
                    }
                    NetJob::Mood { idx, ref mood } => {
                        let (min, codec) = filter_worker.snapshot();
                        match client.stations_by_tags(&mood.tags, 60, min, &codec) {
                            Ok(found) => {
                                let found = found.into_iter().filter(|s| mood.accepts(s)).collect();
                                let mut ranked = bwr_core::discovery::rank_and_dedupe(found);
                                ranked.truncate(200);
                                let _ = result_tx.send(NetResult::Mood(idx, ranked));
                            }
                            Err(e) => {
                                let _ = result_tx.send(NetResult::Failed(e.to_string()));
                            }
                        }
                    }
                    NetJob::Surprise { ref tags, ref rated_good, ref stars, ref cfg } => {
                        let (min, codec) = filter_worker.snapshot();
                        let mut rng = bwr_core::discovery::Rng::seeded();
                        // ~1 in 3: one of your 4–5★ stations.
                        let mut pick = None;
                        if !rated_good.is_empty() && rng.below(3) == 0 {
                            pick = Some(rated_good[rng.below(rated_good.len())].clone());
                        }
                        if pick.is_none() && !tags.is_empty() {
                            // One random mood tag, its most-voted stations.
                            let chosen = vec![tags[rng.below(tags.len())].clone()];
                            if let Ok(found) = client.stations_by_tags(&chosen, 60, min, &codec) {
                                let ranked = bwr_core::discovery::rank_and_dedupe(found);
                                pick = bwr_core::discovery::pick_surprise(
                                    &ranked,
                                    cfg,
                                    |u| stars.get(u).copied().unwrap_or(0),
                                    &mut rng,
                                );
                            }
                        }
                        if pick.is_none() && !rated_good.is_empty() {
                            pick = Some(rated_good[rng.below(rated_good.len())].clone());
                        }
                        match pick {
                            Some(station) => {
                                let _ = result_tx.send(NetResult::Surprise(station));
                            }
                            None => match client.random_station(min, &codec) {
                                Ok(station) => {
                                    let _ = result_tx.send(NetResult::Surprise(station));
                                }
                                Err(e) => {
                                    let _ = result_tx.send(NetResult::Failed(e.to_string()));
                                }
                            },
                        }
                    }
                    NetJob::Click(ref uuid) => client.report_click(uuid),
                    NetJob::Rated(ref uuids) => {
                        match client.stations_by_uuids(uuids) {
                            Ok(stations) => {
                                let _ = result_tx.send(NetResult::Rated(stations));
                            }
                            Err(e) => {
                                let _ = result_tx.send(NetResult::Failed(e.to_string()));
                            }
                        }
                    }
                    NetJob::Favicon(ref uuid, ref url) => {
                        match http.get(url).send() {
                            Ok(resp) => match resp.bytes() {
                                Ok(bytes) if !bytes.is_empty() => {
                                    let _ = result_tx.send(NetResult::Favicon(
                                        uuid.clone(),
                                        bytes.to_vec(),
                                    ));
                                }
                                _ => {}
                            },
                            Err(_) => {}
                        }
                    }
                }
            }
        });

        let player = Player::new();
        player.set_volume(settings.volume);

        #[cfg(windows)]
        let hotkey_rx = crate::hotkeys::spawn();
        #[cfg(windows)]
        let tray = crate::tray::Tray::new();

        let app = Self {
            settings,
            library: Library::load(),
            ratings: bwr_core::Ratings::load(),
            moods: bwr_core::MoodConfig::load(),
            discovery: bwr_core::DiscoveryState::load(),
            stars_level: 0,
            advanced_open: false,
            rated_inflight: std::collections::HashSet::new(),
            player,
            page: Page::Discover,
            discover_tab: DiscoverTab::Trending,
            library_tab: LibraryTab::Favourites,
            stations: Vec::new(),
            current: None,
            status: "Ready".into(),
            song_title: None,
            is_playing: false,
            loading: false,
            tags: Vec::new(),
            countries: Vec::new(),
            browse_selection: None,
            query: String::new(),
            search_active: false,
            search_pending: false,
            last_keystroke: None,
            last_searched: String::new(),
            similar_label: None,
            word_cloud: Default::default(),
            job_tx,
            result_rx,
            bands: [0.0; BANDS],
            favicons: std::collections::HashMap::new(),
            favicon_pending: std::collections::HashSet::new(),
            about_logo,
            about_logo_icon,
            world_map,
            #[cfg(windows)]
            hotkey_rx,
            #[cfg(windows)]
            tray,
            #[cfg(windows)]
            smtc: None,
            #[cfg(windows)]
            smtc_failed: false,
            #[cfg(windows)]
            hidden_to_tray: false,
            #[cfg(windows)]
            restore_pos: None,
            #[cfg(windows)]
            hwnd: None,
            #[cfg(windows)]
            quitting: false,
            ctx: cc.egui_ctx.clone(),
            filter,
            first_frame: true,
            plugins: crate::plugin_bridge::PluginBridge::new(),
        };
        theme::apply(&app.ctx, app.settings.theme, app.settings.accent);
        // Per-session motivational quote in the window title, after the app name.
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "BigWave Radio — {}",
            session_quote()
        )));
        app
    }

    // ---- Public API used by pages/widgets -----------------------------------

    pub fn play_station(&mut self, station: Station) {
        self.status = "Connecting…".into();
        self.song_title = None;
        self.current = Some(station.clone());
        self.player.play(&station.stream_url);
        self.is_playing = true;
        self.discovery.note_played(&station);
        let _ = self.job_tx.send(NetJob::Click(station.uuid.clone()));
        if !station.favicon.trim().is_empty()
            && !self.favicons.contains_key(&station.uuid)
            && !self.favicon_pending.contains(&station.uuid)
        {
            self.favicon_pending.insert(station.uuid.clone());
            let _ = self
                .job_tx
                .send(NetJob::Favicon(station.uuid.clone(), station.favicon.clone()));
        }
    }

    pub fn stop(&mut self) {
        self.player.stop();
        self.is_playing = false;
        self.status = "Stopped".into();
    }

    pub fn toggle_play_pause(&mut self) {
        if self.is_playing {
            self.player.stop();
            self.is_playing = false;
            self.status = "Paused".into();
        } else if let Some(station) = self.current.clone() {
            self.play_station(station);
        }
    }

    pub fn can_go_previous(&self) -> bool {
        self.queue_index().is_some_and(|i| i > 0)
    }

    pub fn can_go_next(&self) -> bool {
        self.queue_index().is_some_and(|i| i + 1 < self.stations.len())
    }

    pub fn previous_station(&mut self) {
        if let Some(i) = self.queue_index() {
            if i > 0 {
                let station = self.stations[i - 1].clone();
                self.play_station(station);
            }
        }
    }

    pub fn next_station(&mut self) {
        if let Some(i) = self.queue_index() {
            if i + 1 < self.stations.len() {
                let station = self.stations[i + 1].clone();
                self.play_station(station);
            }
        }
    }

    pub fn persist_settings(&mut self) {
        self.settings.save();
        self.filter
            .min_bitrate
            .store(self.settings.min_bitrate, Ordering::Relaxed);
        if let Ok(mut codec) = self.filter.codec.lock() {
            *codec = self.settings.codec.clone();
        }
        theme::apply(&self.ctx, self.settings.theme, self.settings.accent);
    }

    /// 🎲 Surprise me: a well-rated or popular station within the moods —
    /// the open mood page's mood, otherwise any of them.
    pub fn surprise(&mut self) {
        self.status = "Finding a station for you…".into();
        let tags = match self.page {
            Page::Mood(i) if i < self.moods.moods.len() => self.moods.moods[i].tags.clone(),
            _ => self.moods.all_mood_tags(),
        };
        let mood_tags: std::collections::HashSet<String> =
            tags.iter().map(|t| t.to_lowercase()).collect();
        let rated_good: Vec<Station> = self
            .ratings
            .rated_stations()
            .into_iter()
            .filter(|(r, s)| {
                *r >= 4
                    && bwr_core::discovery::tag_list(&s.tags)
                        .iter()
                        .any(|t| mood_tags.iter().any(|m| t.contains(m.as_str())))
            })
            .map(|(_, s)| s)
            .collect();
        let _ = self.job_tx.send(NetJob::Surprise {
            tags,
            rated_good,
            stars: self.star_map(),
            cfg: Box::new(self.moods.clone()),
        });
    }

    fn star_map(&self) -> std::collections::HashMap<String, u8> {
        self.ratings
            .rated_stations()
            .into_iter()
            .map(|(r, s)| (s.uuid, r))
            .collect()
    }

    /// ✨ More like this for a whole station: its best tags, weighted by
    /// the user's ratings.
    pub fn more_like_station(&mut self, seed: &Station) {
        let rated = self.ratings.rated_stations();
        let profile = bwr_core::discovery::taste_profile(rated.iter().map(|(r, s)| (*r, s)));
        let tags = bwr_core::discovery::seed_tags(seed, &self.moods, &profile, 4);
        if tags.is_empty() {
            self.status = "This station has no tags to match on".into();
            return;
        }
        self.page = Page::Search;
        self.search_active = true;
        self.search_pending = false;
        self.similar_label = Some(format!("More like {} · #{}", seed.name, tags.join(" #")));
        self.query.clear();
        self.last_searched.clear();
        self.loading = true;
        self.stations.clear();
        let _ = self.job_tx.send(NetJob::MoreLikeStation {
            seed: seed.clone(),
            tags,
            profile,
            stars: self.star_map(),
        });
    }

    /// Open a mood chip's results.
    pub fn open_mood(&mut self, idx: usize) {
        let Some(mood) = self.moods.moods.get(idx) else { return };
        let mood = mood.clone();
        self.page = Page::Mood(idx);
        self.browse_selection = None;
        self.stations.clear();
        self.loading = true;
        let _ = self.job_tx.send(NetJob::Mood { idx, mood });
    }

    /// Talk/news/sport stations are hidden unless "Show talk" is on.
    pub fn hides_talk(&self) -> bool {
        !self.discovery.show_talk && !matches!(self.page, Page::Library | Page::Stars)
    }

    /// ✨ More like this: fill the search page with stations sharing `tag`.
    pub fn more_like_this(&mut self, tag: &str, from: &str) {
        self.page = Page::Search;
        self.search_active = true;
        self.similar_label = Some(format!("More like {from} · #{tag}"));
        self.loading = true;
        let exclude = self
            .current
            .as_ref()
            .map(|s| s.uuid.clone())
            .unwrap_or_default();
        let _ = self.job_tx.send(NetJob::MoreLike {
            tag: tag.to_string(),
            exclude,
        });
    }

    /// Run a search right away (history picks, chips): no debounce.
    pub fn run_search_now(&mut self, q: String) {
        self.query = q.clone();
        self.page = Page::Search;
        self.similar_label = None;
        self.last_searched = q.clone();
        self.last_keystroke = None;
        self.loading = true;
        self.search_pending = true;
        let _ = self.job_tx.send(NetJob::Search(q));
    }

    /// Remember a search query (deduped case-insensitively, newest first).
    pub fn record_search(&mut self, query: &str) {
        let q = query.trim();
        if q.len() < 2 {
            return;
        }
        self.settings.search_history.retain(|h| !h.eq_ignore_ascii_case(q));
        self.settings.search_history.insert(0, q.to_string());
        self.settings
            .search_history
            .truncate(bwr_core::settings::MAX_SEARCH_HISTORY);
        self.settings.save();
    }

    /// Bring the window back from the tray.
    #[cfg(windows)]
    pub fn show_from_tray(&mut self) {
        if let Some(hwnd) = self.hwnd {
            unsafe { show_main_window(hwnd, self.restore_pos) };
        }
        self.restore_pos = None;
        self.hidden_to_tray = false;
    }

    /// Stations to show: talk stations filtered out unless "Show talk" is on
    /// (your own Library and Your Stars lists are never filtered).
    pub fn visible_stations(&self) -> Vec<Station> {
        if !self.hides_talk() {
            return self.stations.clone();
        }
        self.stations
            .iter()
            .filter(|s| !self.moods.is_talk(s))
            .cloned()
            .collect()
    }

    /// How many stations the talk filter is hiding from the current list.
    pub fn hidden_talk_count(&self) -> usize {
        if !self.hides_talk() {
            return 0;
        }
        self.stations.iter().filter(|s| self.moods.is_talk(s)).count()
    }

    /// Re-request the visible list after the bitrate filter changes.
    pub fn reload_current_list(&mut self) {
        self.loading = true;
        let job = match self.page {
            Page::Discover => NetJob::Discover(self.discover_tab),
            Page::Search if self.search_active => NetJob::Search(self.query.clone()),
            Page::Mood(i) => {
                self.open_mood(i);
                return;
            }
            Page::Tags | Page::Countries => {
                if let Some(sel) = self.browse_selection.clone() {
                    NetJob::StationsFor(sel)
                } else {
                    self.loading = false;
                    return;
                }
            }
            Page::Library => {
                self.loading = false;
                return;
            }
            _ => {
                self.loading = false;
                return;
            }
        };
        let _ = self.job_tx.send(job);
    }

    fn queue_index(&self) -> Option<usize> {
        let current = self.current.as_ref()?;
        self.stations.iter().position(|s| s.uuid == current.uuid)
    }

    fn apply_result(&mut self, result: NetResult) {
        self.loading = false;
        match result {
            NetResult::Stations(stations) => {
                // A slow Discover/search/browse answer must not overwrite a
                // page that shows its own list (it used to replace a mood's
                // results with Trending).
                if matches!(
                    self.page,
                    Page::Mood(_) | Page::Stars | Page::Library | Page::Plugins | Page::PluginPage(_) | Page::Settings | Page::About
                ) {
                    self.search_pending = false;
                    return;
                }
                self.stations = stations;
                if self.page == Page::Search && self.search_pending {
                    self.search_active = true;
                    self.similar_label = None;
                }
                self.search_pending = false;
            }
            NetResult::MoreLike(tag, stations) => {
                self.stations = stations;
                // Keep the richer label set by more_like_this() if present.
                if self.similar_label.is_none() {
                    self.similar_label = Some(format!("#{tag}"));
                }
            }
            NetResult::Mood(idx, stations) => {
                // Ignore a late result for a mood the user already left.
                if self.page == Page::Mood(idx) {
                    self.stations = stations;
                }
            }
            NetResult::Surprise(station) => {
                self.play_station(station);
            }
            NetResult::Rated(stations) => {
                for s in &stations {
                    self.ratings.record(s);
                }
                self.rated_inflight.clear();
            }
            NetResult::Tags(tags) => self.tags = tags,
            NetResult::Countries(countries) => {
                // The API orders by station count and includes a few unnamed
                // rows; present named countries only, A to Z.
                let mut countries: Vec<_> = countries
                    .into_iter()
                    .filter(|c| !c.name.trim().is_empty())
                    .collect();
                countries.sort_by(|a, b| {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                });
                self.countries = countries;
            }
            NetResult::Favicon(uuid, bytes) => {
                self.favicon_pending.remove(&uuid);
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let img = img.into_rgba8();
                    let (w, h) = img.dimensions();
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        &img,
                    );
                    let tex = self
                        .ctx
                        .load_texture(format!("favicon-{uuid}"), color, egui::TextureOptions::LINEAR);
                    self.favicons.insert(uuid, tex);
                }
            }
            NetResult::Failed(message) => {
                if self.stations.is_empty() {
                    self.status = format!("Error: {message}");
                } else if self.status.starts_with("Finding a station") {
                    self.status =
                        "No station matched the filters — try loosening them".into();
                }
            }
        }
    }
}

/// Famous life quotes for the window title — one per session, picked at
/// random. Kept short so the title bar and taskbar stay readable.
const QUOTES: &[&str] = &[
    "The unexamined life is not worth living. — Socrates",
    "Not all those who wander are lost. — J.R.R. Tolkien",
    "In the middle of difficulty lies opportunity. — Albert Einstein",
    "Whatever you are, be a good one. — Abraham Lincoln",
    "It does not matter how slowly you go, as long as you do not stop. — Confucius",
    "The best way to predict the future is to invent it. — Alan Kay",
    "He who has a why to live can bear almost any how. — Friedrich Nietzsche",
    "Strive not to be a success, but rather to be of value. — Albert Einstein",
    "Happiness depends upon ourselves. — Aristotle",
    "Dream big and dare to fail. — Norman Vaughan",
    "Turn your wounds into wisdom. — Oprah Winfrey",
    "Well begun is half done. — Aristotle",
    "The only true wisdom is in knowing you know nothing. — Socrates",
    "Life shrinks or expands in proportion to one's courage. — Anaïs Nin",
];

/// Pick the session's quote. Seeded from the wall clock — no rand dependency,
/// and a fresh pick on every launch.
fn session_quote() -> &'static str {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0);
    QUOTES[(nanos % QUOTES.len() as u64) as usize]
}

fn send_result(tx: &mpsc::Sender<NetResult>, res: bwr_core::client::Result<Vec<Station>>) {
    match res {
        Ok(stations) => {
            let _ = tx.send(NetResult::Stations(stations));
        }
        Err(e) => {
            let _ = tx.send(NetResult::Failed(e.to_string()));
        }
    }
}

/// Paint the native title bar dark grey with light text via the Windows 11
/// DWM caption-colour attributes. Best effort: older Windows keeps whatever
/// theme it had.
#[cfg(windows)]
fn dark_caption(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR};
    /// COLORREF packs 0x00BBGGRR.
    const fn colorref(r: u8, g: u8, b: u8) -> u32 {
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
    }
    unsafe {
        // A hair darker than the app's dark chrome fill (30,31,37).
        let caption = colorref(20, 21, 26);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &caption as *const u32 as *const _,
            4,
        );
        let text = colorref(238, 238, 242);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            &text as *const u32 as *const _,
            4,
        );
    }
}

/// Hide WITHOUT ShowWindow(SW_HIDE): a fully hidden window stops receiving
/// paint events and the egui/winit event loop goes idle, which freezes tray/// menu commands, the Ctrl+Alt+B restore hotkey and SMTC. Moving the window
/// far offscreen keeps every subsystem alive; the taskbar button is removed
/// separately. Returns the on-screen position for show_main_window.
#[cfg(windows)]
unsafe fn hide_main_window(hwnd: windows::Win32::Foundation::HWND) -> Option<(i32, i32)> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER, SWP_NOSIZE,
    };
    let mut rect = RECT::default();
    let pos = if GetWindowRect(hwnd, &mut rect).is_ok() {
        Some((rect.left, rect.top))
    } else {
        None
    };
    let _ = SetWindowPos(
        hwnd,
        None,
        -32000,
        -32000,
        0,
        0,
        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    taskbar_tab(hwnd, false);
    pos
}

/// Reverse of hide_main_window: taskbar button back, position restored.
#[cfg(windows)]
unsafe fn show_main_window(hwnd: windows::Win32::Foundation::HWND, pos: Option<(i32, i32)>) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, SetWindowPos, ShowWindow, SWP_NOACTIVATE, SWP_NOZORDER, SWP_NOSIZE,
        SW_RESTORE, SW_SHOW,
    };
    taskbar_tab(hwnd, true);
    if let Some((x, y)) = pos {
        let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = ShowWindow(hwnd, SW_RESTORE);
    let _ = SetForegroundWindow(hwnd);
}

/// Add/remove the taskbar button via ITaskbarList (COM).
#[cfg(windows)]
unsafe fn taskbar_tab(hwnd: windows::Win32::Foundation::HWND, add: bool) {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{ITaskbarList, TaskbarList};

    // No-op if this thread already has an apartment; we intentionally never
    // uninit (the UI thread lives for the whole process).
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    if let Ok(tl) = CoCreateInstance::<_, ITaskbarList>(&TaskbarList, None, CLSCTX_ALL) {
        if tl.HrInit().is_ok() {
            let r = if add { tl.AddTab(hwnd) } else { tl.DeleteTab(hwnd) };
            if let Err(e) = r {
                eprintln!("[tray] taskbar {} failed: {e}", if add { "AddTab" } else { "DeleteTab" });
                crate::diag::log(&format!("[tray] taskbar tab failed ({add}): {e}"));
            }
        }
    }
}

fn fallback_client() -> RadioBrowserClient {
    for attempt in 1..=5 {
        match RadioBrowserClient::new() {
            Ok(c) => return c,
            Err(e) => eprintln!("client init attempt {attempt} failed: {e}"),
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    panic!("no radio-browser mirror reachable");
}

impl eframe::App for BigWaveApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::from_rgba_unmultiplied(12, 12, 12, 180).to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        if self.first_frame {
            self.first_frame = false;
            self.loading = true;
            let _ = self.job_tx.send(NetJob::Discover(self.discover_tab));
        }

        // --- Network results -------------------------------------------------
        while let Ok(result) = self.result_rx.try_recv() {
            self.apply_result(result);
        }

        // --- Debounced live search -------------------------------------------
        if let Some(last) = self.last_keystroke {
            if last.elapsed() >= Duration::from_millis(450) {
                self.last_keystroke = None;
                let q = self.query.trim().to_string();
                if q.len() >= 2 && q != self.last_searched {
                    self.last_searched = q.clone();
                    self.record_search(&q);
                    self.loading = true;
                    self.search_pending = true;
                    let _ = self.job_tx.send(NetJob::Search(q));
                }
            }
        }

        // --- Player events -----------------------------------------------------
        // Events carry the playback generation they belong to; ignore any from
        // a previous station so stale errors/connects never clobber the status.
        let current_gen = self.player.generation();
        for (gen, event) in self.player.poll() {
            if gen != current_gen {
                continue;
            }
            self.plugins.note_player_event(&event);
            match event {
                PlayerEvent::Connecting => self.status = "Connecting…".into(),
                PlayerEvent::Playing => {
                    self.is_playing = true;
                    self.status = "Connected".into();
                }
                PlayerEvent::Stopped => {
                    self.is_playing = false;
                    if self.status != "Paused" {
                        self.status = "Stopped".into();
                    }
                }
                PlayerEvent::Failed(e) => {
                    self.is_playing = false;
                    self.status = format!("Playback error: {e}");
                }
                PlayerEvent::Meta(title) => {
                    self.song_title = Some(title);
                }
            }
        }

        // --- Media keys + tray commands (Windows) ------------------------------
        #[cfg(windows)]
        {
            use crate::hotkeys::MediaKey;
            // SMTC also receives hardware media keys; consuming both would
            // double-toggle. Only fall back to RegisterHotKey when SMTC is
            // unavailable.
            // Drain always: Show must work regardless of SMTC state, and the
            // channel must never grow unboundedly.
            while let Ok(key) = self.hotkey_rx.try_recv() {
                match key {
                    // SMTC also receives hardware media keys; consuming both
                    // would double-toggle, so ignore those when SMTC is up.
                    MediaKey::PlayPause if self.smtc.is_none() => self.toggle_play_pause(),
                    MediaKey::Next if self.smtc.is_none() => self.next_station(),
                    MediaKey::Previous if self.smtc.is_none() => self.previous_station(),
                    MediaKey::Show => {
                        if self.hidden_to_tray {
                            self.show_from_tray();
                        } else if let Some(hwnd) = self.hwnd {
                            unsafe {
                                let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(
                                    hwnd,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Taskbar / media-flyout buttons arrive over a channel.
            let mut smtc_events = Vec::new();
            if let Some(smtc) = &self.smtc {
                while let Some(ev) = smtc.try_recv() {
                    smtc_events.push(ev);
                }
            }
            use crate::smtc::SmtcEvent;
            for ev in smtc_events {
                crate::diag::log(&format!("[smtc] app handling: {ev:?}"));
                match ev {
                    SmtcEvent::Play => {
                        if !self.is_playing {
                            self.toggle_play_pause();
                        }
                    }
                    SmtcEvent::Pause => {
                        if self.is_playing {
                            self.toggle_play_pause();
                        }
                    }
                    SmtcEvent::Next => self.next_station(),
                    SmtcEvent::Previous => self.previous_station(),
                }
            }
            let mut cmds = Vec::new();
            if let Some(tray) = &self.tray {
                while let Some(cmd) = tray.try_recv() {
                    cmds.push(cmd);
                }
            }
            for cmd in cmds {
                use crate::tray::TrayCmd;
                match cmd {
                    TrayCmd::Show => self.show_from_tray(),
                    TrayCmd::PlayPause => self.toggle_play_pause(),
                    TrayCmd::Next => self.next_station(),
                    TrayCmd::Previous => self.previous_station(),
                    TrayCmd::Quit => {
                        self.quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }

            // Resolve our own HWND from egui — never search by window title,
            // other windows on this machine can share it.
            use raw_window_handle::HasWindowHandle;
            if let Ok(handle) = frame.window_handle() {
                if let raw_window_handle::RawWindowHandle::Win32(w) = handle.as_raw() {
                    self.hwnd = Some(windows::Win32::Foundation::HWND(
                        w.hwnd.get() as *mut core::ffi::c_void,
                    ));
                    // Dark grey caption + light title text, so the title bar
                    // matches the app's dark chrome instead of Windows'
                    // default (DWM caption colours, Windows 11).
                    dark_caption(self.hwnd.unwrap());
                }
            }

            // Lazy SMTC init: needs the real HWND, which egui only hands out
            // once the window exists. Try exactly once.
            if self.smtc.is_none() && !self.smtc_failed {
                if let Some(hwnd) = self.hwnd {
                    match crate::smtc::Smtc::new(hwnd) {
                        Ok(s) => {
                            eprintln!("[smtc] taskbar media controls active");
                            crate::diag::log("[smtc] taskbar media controls active");
                            self.smtc = Some(s);
                        }
                        Err(e) => {
                            eprintln!("[smtc] init failed: {e}");
                            crate::diag::log(&format!("[smtc] init failed: {e}"));
                            self.smtc_failed = true;
                        }
                    }
                }
            }

            // Push playback state / metadata to the taskbar controls, but
            // only when something actually changed (guards are in Smtc).
            // Compute everything before taking the mutable borrow.
            use windows::Media::MediaPlaybackStatus;
            let smtc_status = if self.is_playing {
                MediaPlaybackStatus::Playing
            } else if self.status == "Paused" {
                MediaPlaybackStatus::Paused
            } else {
                MediaPlaybackStatus::Stopped
            };
            let smtc_nav = (self.can_go_previous(), self.can_go_next());
            let smtc_meta = self.current.as_ref().map(|st| {
                let artist = self
                    .song_title
                    .clone()
                    .unwrap_or_else(|| "BigWave Radio".into());
                (st.name.clone(), artist)
            });
            if let Some(smtc) = &mut self.smtc {
                smtc.set_status(smtc_status);
                smtc.set_navigation(smtc_nav.0, smtc_nav.1);
                if let Some((title, artist)) = smtc_meta {
                    smtc.set_metadata(&title, &artist);
                }
            }

            // Minimize now behaves normally: the window minimizes to a normal
            // taskbar button, so the thumbnail/SMTC quick controls stay
            // available. Only CLOSE hides to the tray.
            let tray_ok = self.tray.as_ref().is_some_and(|t| t.icon_ok());
            if !self.settings.minimize_to_tray && self.hidden_to_tray {
                // Setting was turned off while hidden: come back.
                self.show_from_tray();
            }

            // Close -> tray (unless Quit came from the tray menu).
            let close_requested =
                ctx.input(|i| i.viewport().close_requested());
            if close_requested && self.settings.minimize_to_tray && tray_ok && !self.quitting {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if let Some(hwnd) = self.hwnd {
                    self.restore_pos = unsafe { hide_main_window(hwnd) };
                }
                self.hidden_to_tray = true;
                // Toast every time: Windows 11 parks new tray icons in the
                // hidden "^" list beside the clock, so the icon is not
                // immediately visible — say exactly where it is and how to
                // get back. Clicking the toast restores the window.
                if let Some(tray) = &self.tray {
                    tray.notify(
                        "BigWave Radio — still playing",
                        "Open the ^ list beside the clock to find the wave icon (drag it out once to keep it there), or press Ctrl+Alt+B to bring the window back.",
                    );
                }
            }

            // While hidden, keep a slow heartbeat repaint going so tray menu
            // commands (Show / Play / Quit) actually get processed.
            if self.hidden_to_tray {
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }

        // --- Spectrum ------------------------------------------------------------
        self.player.spectrum.update();
        self.player.spectrum.snapshot(&mut self.bands);
        crate::plugin_bridge::pre_frame(self, ctx);

        // --- Layout ---------------------------------------------------------------
        egui::TopBottomPanel::top("title-search").show(ctx, |ui| {
            ui.painter().rect_filled(
                ui.max_rect(),
                0.0,
                theme::chrome_fill(ui.ctx(), self.settings.theme),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🔍").size(16.0));
                let width = (ui.available_width() - 56.0)
                    .clamp(200.0, 560.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search stations…")
                        .desired_width(width),
                );
                if response.changed() {
                    self.page = Page::Search;
                    self.last_keystroke = Some(Instant::now());
                }

                // 🎲 Surprise me — random station, honouring the quality filter.
                if ui
                    .button(egui::RichText::new("🎲").size(16.0))
                    .on_hover_text("Surprise me — play a random station")
                    .clicked()
                {
                    self.surprise();
                }

                // Recent searches popup under the field.
                let popup_id = ui.make_persistent_id("search-history");
                // Focus opens the popup; it then stays open until a pick,
                // typing or a click outside (see search_history.rs for the
                // root cause of the old "click does nothing" bug).
                let was_open = ui.memory(|mem| mem.is_popup_open(popup_id));
                let show_history = crate::search_history::popup_open(
                    was_open,
                    response.has_focus(),
                    self.query.is_empty(),
                    !self.settings.search_history.is_empty(),
                );
                ui.memory_mut(|mem| {
                    if show_history {
                        mem.open_popup(popup_id);
                    } else if mem.is_popup_open(popup_id) {
                        mem.close_popup();
                    }
                });
                let history = self.settings.search_history.clone();
                egui::popup::popup_below_widget(
                    ui,
                    popup_id,
                    &response,
                    egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_min_width(240.0);
                        ui.label(egui::RichText::new("RECENT SEARCHES").small().weak());
                        for q in history {
                            let mut remove = false;
                            let picked = ui
                                .horizontal(|ui| {
                                    let picked = ui
                                        .selectable_label(false, format!("🕘  {q}"))
                                        .clicked();
                                    if ui
                                        .add(egui::Button::new("🗙").small().frame(false))
                                        .on_hover_text("Remove from history")
                                        .clicked()
                                    {
                                        remove = true;
                                    }
                                    picked
                                })
                                .inner;
                            if remove {
                                self.settings
                                    .search_history
                                    .retain(|h| !h.eq_ignore_ascii_case(&q));
                                self.settings.save();
                            }
                            if picked {
                                let pick = crate::search_history::pick(
                                    &q,
                                    &mut self.settings.search_history,
                                    bwr_core::settings::MAX_SEARCH_HISTORY,
                                );
                                self.settings.save();
                                self.run_search_now(pick.query);
                                ui.memory_mut(|m| m.close_popup());
                            }
                        }
                        if ui.add(egui::Button::new("Clear history").small()).clicked() {
                            self.settings.search_history.clear();
                            self.settings.save();
                            ui.memory_mut(|m| m.close_popup());
                        }
                    },
                );
            });
            ui.add_space(2.0);
        });

        egui::SidePanel::left("nav")
            .resizable(false)
            .exact_width(188.0)
            .show(ctx, |ui| {
                ui.painter().rect_filled(
                    ui.max_rect(),
                    0.0,
                    theme::chrome_fill(ui.ctx(), self.settings.theme),
                );
                ui.add_space(8.0);
                // Nav scrolls when the window is short so the filter block
                // below (which is bottom-anchored) never gets pushed out.
                let reserve = crate::widgets::mood_rail::reserved_height(self);
                egui::ScrollArea::vertical()
                    .id_salt("nav-scroll")
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - reserve).max(72.0))
                    .show(ui, |ui| {
                        nav_button(ui, self, Page::Discover, "🏠  Discover");
                        nav_button(ui, self, Page::Search, "🔎  Search");
                        nav_button(ui, self, Page::Library, "★  My Library");
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                        nav_button(ui, self, Page::Tags, "🏷  Tags");
                        nav_button(ui, self, Page::Countries, "🗺  Countries");
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                        for (i, (pid, pg)) in self.plugins.pages.clone().into_iter().enumerate() {
                            let icon = if pg.icon.is_empty() { "🔌".to_string() } else { pg.icon };
                            let hint = self.plugins.hotkey_hint(&pid).map(|k| format!("   {k}")).unwrap_or_default();
                            nav_button(ui, self, Page::PluginPage(i), &format!("{icon}  {}{hint}", pg.title));
                        }
                        nav_button(ui, self, Page::Plugins, "🔌  Plugins");
                        nav_button(ui, self, Page::Settings, "⚙  Settings");
                        nav_button(ui, self, Page::About, "ℹ  About");
                    });

                ui.add_space(6.0);
                crate::widgets::mood_rail::rail(ui, self);
            });

        egui::TopBottomPanel::bottom("now-playing")
            .exact_height(92.0)
            .show(ctx, |ui| {
                now_playing_bar::bar(ui, self);
            });

        if self.current.is_some() {
            egui::SidePanel::right("station-info")
                .resizable(false)
                .default_width(250.0)
                .width_range(200.0..=320.0)
                .show(ctx, |ui| {
                    ui.painter().rect_filled(
                        ui.max_rect(),
                        0.0,
                        theme::chrome_fill(ui.ctx(), self.settings.theme),
                    );
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            crate::widgets::station_info::panel(ui, self);
                        });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Mica-style material behind the page content (Windows 11).
            if theme::win11_material() {
                theme::paint_material(ui.ctx(), self.settings.theme, self.settings.accent, ui.max_rect());
            }
            match self.page {
                Page::Discover => pages::discover::ui(ui, self),
                Page::Search => pages::search::ui(ui, self),
                Page::Library => pages::library::ui(ui, self),
                Page::Tags => pages::browse::tags_page(ui, self),
                Page::Countries => pages::browse::countries_page(ui, self),
                Page::Settings => pages::settings::ui(ui, self),
                Page::Plugins => pages::plugins::ui(ui, self),
                Page::About => pages::about::ui(ui, self),
                Page::PluginPage(i) => pages::plugins::plugin_page(ui, self, i),
                Page::Mood(i) => pages::moods::mood_page(ui, self, i),
                Page::Stars => pages::moods::stars_page(ui, self),
            }
        });
        crate::plugin_bridge::post_frame(self, ctx);

        // Repaint scheduling MUST come after the layout: pages and widgets set
        // `loading` / `is_playing` while drawing, and egui is event-driven —
        // without these requests no further frame would ever run.
        if self.is_playing {
            ctx.request_repaint();
        }
        if self.loading {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
        // The idle search page animates the word cloud continuously.
        if matches!(self.page, Page::Search) && !self.search_active {
            ctx.request_repaint();
        }
    }
}

fn nav_button(ui: &mut egui::Ui, app: &mut BigWaveApp, page: Page, label: &str) {
    let selected = app.page == page;    let button = egui::Button::new(egui::RichText::new(label).size(14.0))
        .selected(selected)
        .min_size(egui::vec2(ui.available_width(), 30.0));
    if ui.add(button).clicked() && !selected {
        app.page = page;
        app.browse_selection = None;
        match page {
            Page::Discover => app.reload_current_list(),
            Page::Library | Page::Settings => {
                app.stations.clear();
            }
            Page::Plugins | Page::PluginPage(_) => {}
            Page::Mood(i) => app.open_mood(i),
            Page::Stars => app.stations.clear(),
            Page::Search => {
                // Tags back the idle "pick a vibe" chips — fetch on first visit
                // rather than at startup so they never block playback actions.
                if app.tags.is_empty() {
                    app.loading = true;
                    let _ = app.job_tx.send(NetJob::Tags);
                }
            }
            Page::Tags => {
                app.stations.clear();
                if app.tags.is_empty() {
                    app.loading = true;
                    let _ = app.job_tx.send(NetJob::Tags);
                }
            }
            Page::Countries => {
                app.stations.clear();
                if app.countries.is_empty() {
                    app.loading = true;
                    let _ = app.job_tx.send(NetJob::Countries);
                }
            }
            Page::About => app.stations.clear(),
        }
    }
}
