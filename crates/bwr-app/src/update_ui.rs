//! "Check for updates": background GitHub check, the update dialog and the
//! quiet startup banner. Logic lives in `bwr_core::update`; this is the UI.
//!
//! Test hooks (for screenshots / manual testing, no effect when unset):
//! `BWR_UPDATE_FIXTURE=<release.json>` reads the release from a file instead
//! of GitHub, `BWR_FAKE_VERSION=0.9.0` pretends to be an older build.

use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use bwr_core::update::{self, Release, UpdateState};
use eframe::egui::{self, Color32, Context, RichText};

use crate::pages::plugins::{action_button, Btn};

/// What the dialog is showing.
pub enum Dialog {
    Update(Release),
    UpToDate(String),
    Failed(String),
}

pub struct Updater {
    pub state: UpdateState,
    rx: Option<Receiver<(bool, Result<Release, String>)>>,
    pub dialog: Option<Dialog>,
    /// Startup check found an update: small non-modal banner.
    pub banner: Option<Release>,
    startup_done: bool,
}

/// The version this build reports (CARGO_PKG_VERSION unless faked for tests).
pub fn current_version() -> String {
    std::env::var("BWR_FAKE_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

impl Updater {
    pub fn new() -> Self {
        Self {
            state: UpdateState::load(),
            rx: None,
            dialog: None,
            banner: None,
            startup_done: false,
        }
    }

    pub fn checking(&self) -> bool {
        self.rx.is_some()
    }

    /// Start a check on a background thread. `manual` = the user pressed the button.
    pub fn start(&mut self, ctx: &Context, manual: bool) {
        if self.rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        let current = current_version();
        std::thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                let res = match std::env::var("BWR_UPDATE_FIXTURE") {
                    Ok(path) => std::fs::read_to_string(&path)
                        .map_err(|e| format!("couldn't read {path} ({e})"))
                        .and_then(|t| update::parse_release(&t)),
                    Err(_) => {
                        update::fetch_latest(update::LATEST_URL, &current, Duration::from_secs(10))
                    }
                };
                let _ = tx.send((manual, res));
                ctx.request_repaint();
            })
            .ok();
    }

    /// Called every frame: kicks off the startup check and collects results.
    pub fn poll(&mut self, ctx: &Context) {
        if !self.startup_done {
            self.startup_done = true;
            if self.state.startup_due(update::now_secs()) {
                self.start(ctx, false);
            }
        }
        let Some(rx) = &self.rx else { return };
        let Ok((manual, res)) = rx.try_recv() else {
            return;
        };
        self.rx = None;
        let current = current_version();
        match res {
            Ok(rel) => {
                self.state.last_check = update::now_secs();
                self.state.save();
                if manual {
                    self.banner = None;
                    self.dialog = Some(if update::is_newer(&rel.tag, &current) {
                        Dialog::Update(rel)
                    } else {
                        Dialog::UpToDate(current)
                    });
                } else if self.state.wants_banner(&rel, &current) {
                    self.banner = Some(rel);
                }
            }
            // A startup check stays silent on failure (and retries next start).
            Err(e) if manual => self.dialog = Some(Dialog::Failed(e)),
            Err(_e) => {
                #[cfg(windows)]
                crate::diag::log(&format!("[update] startup check failed: {_e}"));
            }
        }
    }
}

/// Draw the dialog and/or banner (call once per frame, after the panels).
pub fn show(ctx: &Context, up: &mut Updater, accent: Color32) {
    banner(ctx, up, accent);
    let Some(dialog) = &up.dialog else { return };
    let mut close = false;
    let mut skip: Option<String> = None;
    let modal = egui::Modal::new(egui::Id::new("update-dialog")).show(ctx, |ui| {
        let w = (ctx.screen_rect().width() - 80.0).clamp(320.0, 560.0);
        ui.set_width(w);
        match dialog {
            Dialog::Update(rel) => {
                ui.heading(RichText::new(format!("⬆  {}", t!("update.available"))).color(accent));
                ui.add_space(8.0);
                egui::Grid::new("update-versions").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    ui.label(t!("update.installed"));
                    ui.label(RichText::new(format!("v{}", current_version())).strong());
                    ui.end_row();
                    ui.label(t!("update.latest"));
                    ui.label(RichText::new(format!("v{}", rel.version)).strong().color(accent));
                    ui.end_row();
                });
                ui.add_space(10.0);
                ui.label(RichText::new(&rel.name).strong());
                ui.add_space(4.0);
                let notes = update::notes_to_plain(&rel.notes);
                egui::Frame::new()
                    .fill(ui.visuals().extreme_bg_color)
                    .corner_radius(6)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(240.0).auto_shrink([false, true]).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if notes.is_empty() {
                                ui.label(RichText::new(t!("update.no_notes")).weak());
                            } else {
                                ui.label(RichText::new(notes).size(13.0));
                            }
                        });
                    });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(match (rel.asset_url.is_some(), update::Platform::current()) {
                        (true, update::Platform::MacOs) => t!("update.how_macos"),
                        (true, update::Platform::Linux { .. }) => t!("update.how_linux"),
                        (true, _) => t!("update.how_windows"),
                        (false, _) => t!("update.how_page"),
                    })
                    .size(12.0)
                    .weak(),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if action_button(ui, "⬇", t!("update.download"), Btn::Primary).on_hover_text(rel.download_url()).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(rel.download_url()));
                        close = true;
                    }
                    if action_button(ui, "🌐", t!("update.view_page"), Btn::Secondary).on_hover_text(&rel.html_url).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(&rel.html_url));
                    }
                    if action_button(ui, "", t!("update.not_now"), Btn::Secondary).clicked() {
                        close = true;
                    }
                    if ui
                        .add(egui::Button::new(RichText::new(t!("update.skip")).size(12.5)).frame(false))
                        .on_hover_text(t!("update.skip_tip"))
                        .clicked()
                    {
                        skip = Some(rel.version.clone());
                        close = true;
                    }
                });
            }
            Dialog::UpToDate(v) => {
                ui.heading(format!("✔  {}", t!("update.up_to_date_title")));
                ui.add_space(6.0);
                ui.label(t!("update.up_to_date", version = v));
                ui.add_space(10.0);
                if action_button(ui, "", t!("common.ok"), Btn::Primary).clicked() {
                    close = true;
                }
            }
            Dialog::Failed(e) => {
                ui.heading(t!("update.failed_title"));
                ui.add_space(6.0);
                ui.label(t!("update.reason", reason = e));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if action_button(ui, "🌐", t!("update.open_releases"), Btn::Secondary).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(update::RELEASES_PAGE));
                    }
                    if action_button(ui, "", t!("common.ok"), Btn::Primary).clicked() {
                        close = true;
                    }
                });
            }
        }
    });
    if modal.should_close() {
        close = true;
    }
    if let Some(v) = skip {
        up.state.skipped_version = Some(v);
        up.state.save();
        up.banner = None;
    }
    if close {
        up.dialog = None;
    }
}

/// Small non-modal banner above the now-playing bar (startup check only).
fn banner(ctx: &Context, up: &mut Updater, accent: Color32) {
    if up.dialog.is_some() {
        return;
    }
    let Some(rel) = up.banner.clone() else { return };
    let mut open_dialog = false;
    let mut dismiss = false;
    egui::Area::new(egui::Id::new("update-banner"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -104.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .stroke(egui::Stroke::new(1.0_f32, accent))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⬆").color(accent).size(16.0));
                        ui.label(t!("update.banner", version = rel.version));
                        if action_button(ui, "", t!("update.details"), Btn::SmallPrimary).clicked() {
                            open_dialog = true;
                        }
                        if ui
                            .add(egui::Button::new("🗙").frame(false))
                            .on_hover_text(t!("common.dismiss"))
                            .clicked()
                        {
                            dismiss = true;
                        }
                    });
                });
        });
    if open_dialog {
        up.dialog = Some(Dialog::Update(rel));
        up.banner = None;
    } else if dismiss {
        up.banner = None;
    }
}
