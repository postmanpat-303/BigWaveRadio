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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverTab {
    Trending,
    Popular,
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
    /// Stations sharing a tag, excluding one uuid (More like this).
    MoreLike { tag: String, exclude: String },
    /// Pick one random station honouring the quality filter (Surprise me).
    Surprise,
    Click(String),
    Favicon(String, String),
}

pub enum NetResult {
    Stations(Vec<Station>),
    /// Result of a "More like this" request, with the tag it was for.
    MoreLike(String, Vec<Station>),
    Surprise(Station),
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
    pub player: Player,

    pub page: Page,
    pub discover_tab: DiscoverTab,
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
    pub last_keystroke: Option<Instant>,
    pub last_searched: String,
    /// "More like this" context: the station name the current search list relates to.
    pub similar_label: Option<String>,

    // Networking
    pub job_tx: mpsc::Sender<NetJob>,
    result_rx: mpsc::Receiver<NetResult>,

    // Visualiser
    pub bands: [f32; BANDS],

    // Station artwork shown in the right-hand info panel, keyed by station uuid.
    pub favicons: std::collections::HashMap<String, egui::TextureHandle>,
    favicon_pending: std::collections::HashSet<String>,
    pub about_logo: egui::TextureHandle,

    // Windows integration
    #[cfg(windows)]
    hotkey_rx: mpsc::Receiver<crate::hotkeys::MediaKey>,
    #[cfg(windows)]
    tray: Option<crate::tray::Tray>,
    /// Window currently hidden to the tray.
    #[cfg(windows)]
    hidden_to_tray: bool,
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
                        let res = client.search(q, min, &codec);
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
                        match client.similar(tag, exclude, min, &codec) {
                            Ok(stations) => {
                                let _ = result_tx.send(NetResult::MoreLike(tag.clone(), stations));
                            }
                            Err(e) => {
                                let _ = result_tx.send(NetResult::Failed(e.to_string()));
                            }
                        }
                    }
                    NetJob::Surprise => {
                        let (min, codec) = filter_worker.snapshot();
                        match client.random_station(min, &codec) {
                            Ok(station) => {
                                let _ = result_tx.send(NetResult::Surprise(station));
                            }
                            Err(e) => {
                                let _ = result_tx.send(NetResult::Failed(e.to_string()));
                            }
                        }
                    }
                    NetJob::Click(ref uuid) => client.report_click(uuid),
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
        let tray = {
            let t = crate::tray::Tray::new();
            if let Some(t) = &t {
                t.ping();
            }
            t
        };

        let app = Self {
            settings,
            library: Library::load(),
            player,
            page: Page::Discover,
            discover_tab: DiscoverTab::Trending,
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
            last_keystroke: None,
            last_searched: String::new(),
            similar_label: None,
            job_tx,
            result_rx,
            bands: [0.0; BANDS],
            favicons: std::collections::HashMap::new(),
            favicon_pending: std::collections::HashSet::new(),
            about_logo,
            #[cfg(windows)]
            hotkey_rx,
            #[cfg(windows)]
            tray,
            #[cfg(windows)]
            hidden_to_tray: false,
            #[cfg(windows)]
            hwnd: None,
            #[cfg(windows)]
            quitting: false,
            ctx: cc.egui_ctx.clone(),
            filter,
            first_frame: true,
        };
        theme::apply(&app.ctx, app.settings.theme, app.settings.accent);
        app
    }

    // ---- Public API used by pages/widgets -----------------------------------

    pub fn play_station(&mut self, station: Station) {
        self.status = "Connecting…".into();
        self.song_title = None;
        self.current = Some(station.clone());
        self.player.play(&station.stream_url);
        self.is_playing = true;
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

    /// 🎲 Surprise me: play one random station honouring the quality filter.
    pub fn surprise(&mut self) {
        self.status = "Finding a station for you…".into();
        let _ = self.job_tx.send(NetJob::Surprise);
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
            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::{
                    SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
                };
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
        }
        self.hidden_to_tray = false;
    }

    /// Re-request the visible list after the bitrate filter changes.
    pub fn reload_current_list(&mut self) {
        self.loading = true;
        let job = match self.page {
            Page::Discover => NetJob::Discover(self.discover_tab),
            Page::Search if self.search_active => NetJob::Search(self.query.clone()),
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
                self.stations = stations;
                if self.page == Page::Search {
                    self.search_active = true;
                    self.similar_label = None;
                }
            }
            NetResult::MoreLike(tag, stations) => {
                self.stations = stations;
                // Keep the richer label set by more_like_this() if present.
                if self.similar_label.is_none() {
                    self.similar_label = Some(format!("#{tag}"));
                }
            }
            NetResult::Surprise(station) => {
                self.play_station(station);
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

#[cfg(windows)]
unsafe fn hide_main_window(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::ShowWindow;
    let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
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
            while let Ok(key) = self.hotkey_rx.try_recv() {
                match key {
                    MediaKey::PlayPause => self.toggle_play_pause(),
                    MediaKey::Next => self.next_station(),
                    MediaKey::Previous => self.previous_station(),
                }
            }
            if let Some(tray) = &self.tray {
                tray.ping();
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
                }
            }

            // Minimize -> tray. egui is event-driven, so while the setting is
            // on we keep a slow repaint heartbeat going — otherwise update()
            // never runs in the idle state and a minimize would go unnoticed.
            if self.settings.minimize_to_tray {
                ctx.request_repaint_after(Duration::from_millis(400));
                if !self.hidden_to_tray {
                    if let Some(hwnd) = self.hwnd {
                        unsafe {
                            if windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd).as_bool() {
                                hide_main_window(hwnd);
                                self.hidden_to_tray = true;
                            }
                        }
                    }
                }
            } else if self.hidden_to_tray {
                self.show_from_tray();
            }

            // Close -> tray (unless Quit came from the tray menu).
            let close_requested =
                ctx.input(|i| i.viewport().close_requested());
            if close_requested && self.settings.minimize_to_tray && !self.quitting {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if let Some(hwnd) = self.hwnd {
                    unsafe { hide_main_window(hwnd) };
                }
                self.hidden_to_tray = true;
            }

            // While hidden, keep a slow heartbeat repaint going so the tray
            // thread keeps receiving pings.
            if self.hidden_to_tray {
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }

        // --- Spectrum ------------------------------------------------------------
        self.player.spectrum.update();
        self.player.spectrum.snapshot(&mut self.bands);

        // --- Layout ---------------------------------------------------------------
        egui::TopBottomPanel::top("title-search").show(ctx, |ui| {
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
                let show_history = response.has_focus()
                    && self.query.is_empty()
                    && !self.settings.search_history.is_empty();
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
                                        .add(egui::Button::new("✕").small().frame(false))
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
                                self.query = q.clone();
                                self.page = Page::Search;
                                self.last_searched = q.clone();
                                self.last_keystroke = None;
                                self.loading = true;
                                let _ = self.job_tx.send(NetJob::Search(q));
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
            .exact_width(170.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                nav_button(ui, self, Page::Discover, "🏠  Discover");
                nav_button(ui, self, Page::Search, "🔎  Search");
                nav_button(ui, self, Page::Library, "★  My Library");
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                nav_button(ui, self, Page::Tags, "🏷  Tags");
                nav_button(ui, self, Page::Countries, "🗺  Countries");
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                nav_button(ui, self, Page::Settings, "⚙  Settings");

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("FORMAT").small().weak());
                    egui::ComboBox::from_id_salt("nav-codec")
                        .selected_text(if self.settings.codec.is_empty() {
                            "Any format".to_string()
                        } else {
                            self.settings.codec.to_uppercase()
                        })
                        .show_ui(ui, |ui| {
                            for c in ["", "mp3", "aac", "ogg", "flac", "opus"] {
                                let label = if c.is_empty() {
                                    "Any format".to_string()
                                } else {
                                    c.to_uppercase()
                                };
                                if ui
                                    .selectable_label(self.settings.codec == c, label)
                                    .clicked()
                                {
                                    self.settings.codec = c.to_string();
                                    self.persist_settings();
                                    self.reload_current_list();
                                }
                            }
                        });
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("MINIMUM BITRATE").small().weak());
                    egui::ComboBox::from_id_salt("nav-bitrate")
                        .selected_text(match self.settings.min_bitrate {
                            0 => "Any bitrate".to_string(),
                            b => format!("{b} kbps+"),
                        })
                        .show_ui(ui, |ui| {
                            for b in [0u32, 64, 128, 192, 256, 320] {
                                let label = if b == 0 {
                                    "Any bitrate".to_string()
                                } else {
                                    format!("{b} kbps or more")
                                };
                                if ui
                                    .selectable_label(self.settings.min_bitrate == b, label)
                                    .clicked()
                                {
                                    self.settings.min_bitrate = b;
                                    self.persist_settings();
                                    self.reload_current_list();
                                }
                            }
                        });
                });
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
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            crate::widgets::station_info::panel(ui, self);
                        });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.page {
                Page::Discover => pages::discover::ui(ui, self),
                Page::Search => pages::search::ui(ui, self),
                Page::Library => pages::library::ui(ui, self),
                Page::Tags => pages::browse::tags_page(ui, self),
                Page::Countries => pages::browse::countries_page(ui, self),
                Page::Settings => pages::settings::ui(ui, self),
            }
        });

        // Repaint scheduling MUST come after the layout: pages and widgets set
        // `loading` / `is_playing` while drawing, and egui is event-driven —
        // without these requests no further frame would ever run.
        if self.is_playing {
            ctx.request_repaint();
        }
        if self.loading {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }
}

fn nav_button(ui: &mut egui::Ui, app: &mut BigWaveApp, page: Page, label: &str) {
    let selected = app.page == page;
    let button = egui::Button::new(egui::RichText::new(label).size(14.0))
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
        }
    }
}
