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
    Click(String),
    Favicon(String, String),
}

pub enum NetResult {
    Stations(Vec<Station>),
    Tags(Vec<Tag>),
    Countries(Vec<Country>),
    Favicon(String, Vec<u8>),
    Failed(String),
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
    pub is_playing: bool,
    pub loading: bool,

    pub tags: Vec<Tag>,
    pub countries: Vec<Country>,
    pub browse_selection: Option<BrowseSel>,

    // Search
    pub query: String,
    pub search_active: bool,
    pub last_keystroke: Option<Instant>,
    last_searched: String,

    // Networking
    pub job_tx: mpsc::Sender<NetJob>,
    result_rx: mpsc::Receiver<NetResult>,

    // Visualiser
    pub bands: [f32; BANDS],

    // Station artwork shown in the right-hand info panel, keyed by station uuid.
    pub favicons: std::collections::HashMap<String, egui::TextureHandle>,
    favicon_pending: std::collections::HashSet<String>,
    pub about_logo: egui::TextureHandle,

    ctx: Context,
    min_bitrate: Arc<AtomicU32>,
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
        let min_bitrate = Arc::new(AtomicU32::new(settings.min_bitrate));
        let min_bitrate_worker = min_bitrate.clone();
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
                        let min = min_bitrate_worker.load(Ordering::Relaxed);
                        let res = match tab {
                            DiscoverTab::Trending => client.trending(min),
                            DiscoverTab::Popular => client.popular(min),
                            DiscoverTab::Recent => client.recently_added(min),
                        };
                        send_result(&result_tx, res);
                    }
                    NetJob::Search(ref q) => {
                        let res = client.search(q, min_bitrate_worker.load(Ordering::Relaxed));
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
                        let min = min_bitrate_worker.load(Ordering::Relaxed);
                        let res = match sel {
                            BrowseSel::Tag(t) => client.stations_by_tag(t, min),
                            BrowseSel::Country(c) => client.stations_by_country(c, min),
                        };
                        send_result(&result_tx, res);
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

        let app = Self {
            settings,
            library: Library::load(),
            player,
            page: Page::Discover,
            discover_tab: DiscoverTab::Trending,
            stations: Vec::new(),
            current: None,
            status: "Ready".into(),
            is_playing: false,
            loading: false,
            tags: Vec::new(),
            countries: Vec::new(),
            browse_selection: None,
            query: String::new(),
            search_active: false,
            last_keystroke: None,
            last_searched: String::new(),
            job_tx,
            result_rx,
            bands: [0.0; BANDS],
            favicons: std::collections::HashMap::new(),
            favicon_pending: std::collections::HashSet::new(),
            about_logo,
            ctx: cc.egui_ctx.clone(),
            min_bitrate,
            first_frame: true,
        };
        theme::apply(&app.ctx, app.settings.theme, app.settings.accent);
        app
    }

    // ---- Public API used by pages/widgets -----------------------------------

    pub fn play_station(&mut self, station: Station) {
        self.status = "Connecting…".into();
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
        self.min_bitrate
            .store(self.settings.min_bitrate, Ordering::Relaxed);
        theme::apply(&self.ctx, self.settings.theme, self.settings.accent);
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
                }
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
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if self.first_frame {
            self.first_frame = false;
            self.loading = true;
            let _ = self.job_tx.send(NetJob::Discover(self.discover_tab));
            let _ = self.job_tx.send(NetJob::Tags);
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
                let width = (ui.available_width() - 20.0)
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
            .exact_height(74.0)
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
            Page::Search => {}
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
