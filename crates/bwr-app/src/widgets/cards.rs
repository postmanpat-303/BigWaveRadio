//! Building blocks of the card look: palette, painted line icons, cards
//! with coloured headers, pills, star rows and station list rows.
//!
//! Colours: the page/nav/card greys follow the light or dark theme. The
//! *primary* colour (Now Playing header, play button, active pills, the
//! "now playing" text) is the user's accent colour; "Follow system" uses
//! the brand green. The secondary amber header (station lists) and the
//! per-mood tile colours are fixed brand colours so the views stay
//! recognisable whatever the accent. Header text is always white.

use std::sync::Arc;

use bwr_core::Station;
use eframe::egui::{
    self, epaint, Align2, Color32, CornerRadius, FontId, Galley, Painter, Pos2, Rect, Sense, Shape,
    Stroke, Ui, Vec2,
};

use crate::app::BigWaveApp;
use crate::theme;

/// Translucent white in gamma space (`Color32::from_white_alpha` works in
/// linear space, which washes headers out far more than the mockups).
pub const fn white_a(a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(a, a, a, a)
}

pub const AMBER: Color32 = Color32::from_rgb(0xf6, 0xc5, 0x6d);
pub const NEEDLE: Color32 = Color32::from_rgb(0xe2, 0x55, 0x3a);
pub const CARD_RADIUS: u8 = 16;
pub const HEADER_H: f32 = 58.0;
pub const ROW_H: f32 = 52.0;

/// Theme-dependent colours for the card look.
#[derive(Clone, Copy, Debug)]
pub struct Pal {
    pub dark: bool,
    pub bg: Color32,
    pub nav: Color32,
    pub nav_border: Color32,
    pub nav_active: Color32,
    pub card: Color32,
    pub line: Color32,
    pub text: Color32,
    pub sub: Color32,
    pub weak: Color32,
    pub pill_border: Color32,
    pub star_off: Color32,
    pub primary: Color32,
    /// Readable tint of the primary colour for text on cards.
    pub now: Color32,
}

impl Pal {
    pub fn new(ctx: &egui::Context, app: &BigWaveApp) -> Self {
        let dark = theme::is_dark(ctx, app.settings.theme);
        let primary = theme::accent_color(app.settings.accent);
        if dark {
            Pal {
                dark,
                bg: Color32::from_rgb(0x1b, 0x1d, 0x21),
                nav: Color32::from_rgb(0x22, 0x25, 0x2a),
                nav_border: Color32::from_rgb(0x2c, 0x30, 0x36),
                nav_active: Color32::from_rgb(0x2f, 0x35, 0x3b),
                card: Color32::from_rgb(0x3a, 0x40, 0x46),
                line: Color32::from_rgb(0x56, 0x5d, 0x64),
                text: Color32::from_rgb(0xe9, 0xec, 0xef),
                sub: Color32::from_rgb(0xae, 0xb6, 0xbe),
                weak: Color32::from_rgb(0x94, 0x9c, 0xa4),
                pill_border: Color32::from_rgb(0x59, 0x61, 0x6a),
                star_off: Color32::from_rgb(0x5b, 0x63, 0x6b),
                primary,
                now: primary.lerp_to_gamma(Color32::WHITE, 0.35),
            }
        } else {
            Pal {
                dark,
                bg: Color32::from_rgb(0xec, 0xee, 0xf1),
                nav: Color32::from_rgb(0xf7, 0xf8, 0xfa),
                nav_border: Color32::from_rgb(0xdd, 0xe1, 0xe5),
                nav_active: Color32::from_rgb(0xe3, 0xe7, 0xeb),
                card: Color32::WHITE,
                line: Color32::from_rgb(0xe0, 0xe4, 0xe8),
                text: Color32::from_rgb(0x1d, 0x21, 0x25),
                sub: Color32::from_rgb(0x55, 0x5d, 0x65),
                weak: Color32::from_rgb(0x6f, 0x77, 0x7f),
                pill_border: Color32::from_rgb(0xc3, 0xc9, 0xcf),
                star_off: Color32::from_rgb(0xcb, 0xd0, 0xd5),
                primary,
                now: primary.lerp_to_gamma(Color32::BLACK, 0.3),
            }
        }
    }

    pub fn stars(&self) -> Color32 {
        if self.dark {
            AMBER
        } else {
            Color32::from_rgb(0xe8, 0xa8, 0x2c)
        }
    }
}

// ---- Text helpers -------------------------------------------------------------

/// One line of text, elided with "…" to `max_w`.
pub fn line_galley(ui: &Ui, text: &str, font: FontId, color: Color32, max_w: f32) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping {
        max_width: max_w.max(8.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.fonts(|f| f.layout_job(job))
}

pub fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), font, Color32::WHITE).size().x)
}

/// The bundled light face used for the big station readout. Glyphs it
/// lacks (Chinese, Devanagari, Arabic) fall back to the UI fonts.
pub fn readout_font(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name(crate::i18n::READOUT_FAMILY.into()))
}

/// "Lounge" from "lounge,chillout,…": the station's first tag, capitalised.
pub fn genre(s: &Station) -> String {
    let first = s.tags.split(',').map(str::trim).find(|t| !t.is_empty()).unwrap_or("");
    let mut c = first.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Short country label for list rows: the ISO code when the API gave one,
/// else the country name.
pub fn country_short(s: &Station) -> String {
    s.country.trim().to_string()
}

/// "192k" for list rows.
pub fn bitrate_short(s: &Station) -> String {
    if s.bitrate > 0 {
        t!("cardsui.kbps_short", kbps = s.bitrate)
    } else {
        "—".into()
    }
}

// ---- Icons --------------------------------------------------------------------

/// Line icons painted from 24×24 outlines (same set as the mockups).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Menu,
    Power,
    Back,
    Forward,
    Search,
    Heart,
    HeartFilled,
    Speaker,
    Play,
    Pause,
    Prev,
    Next,
    Info,
    Eye,
    Bolt,
    Expand,
    Close,
    Mini,
    Home,
    Star,
    Tag,
    Globe,
    Plug,
    Gear,
    Dice,
    Radio,
    Globe2,
}

struct Pen<'a> {
    p: &'a Painter,
    c: Pos2,
    k: f32,
    stroke: Stroke,
    color: Color32,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(self.c.x + (x - 12.0) * self.k, self.c.y + (y - 12.0) * self.k)
    }
    fn line(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::line(pts, self.stroke));
    }
    fn closed(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::closed_line(pts, self.stroke));
    }
    fn circle(&self, x: f32, y: f32, r: f32) {
        self.p.circle_stroke(self.at(x, y), r * self.k, self.stroke);
    }
    fn dot(&self, x: f32, y: f32, r: f32) {
        self.p.circle_filled(self.at(x, y), r * self.k, self.color);
    }
    fn arc(&self, cx: f32, cy: f32, r: f32, from_deg: f32, to_deg: f32) {
        let n = 24;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = (from_deg + (to_deg - from_deg) * i as f32 / n as f32).to_radians();
                self.at(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        self.p.add(Shape::line(pts, self.stroke));
    }
    fn fill(&self, centre: (f32, f32), pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        fan(self.p, self.at(centre.0, centre.1), &pts, self.color);
    }
    fn rect(&self, x: f32, y: f32, w: f32, h: f32) {
        self.p.rect_filled(
            Rect::from_min_max(self.at(x, y), self.at(x + w, y + h)),
            CornerRadius::same(1),
            self.color,
        );
    }
}

/// Fill a star-shaped polygon (every edge visible from `centre`).
pub fn fan(p: &Painter, centre: Pos2, pts: &[Pos2], color: Color32) {
    if pts.len() < 3 {
        return;
    }
    let mut mesh = epaint::Mesh::default();
    mesh.colored_vertex(centre, color);
    for q in pts {
        mesh.colored_vertex(*q, color);
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(mesh));
}

fn heart_points() -> Vec<(f32, f32)> {
    (0..48)
        .map(|i| {
            let t = i as f32 / 48.0 * std::f32::consts::TAU;
            let x = 16.0 * t.sin().powi(3);
            let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
            (12.0 + x * 0.5, 11.5 - y * 0.5)
        })
        .collect()
}

fn star_points(cx: f32, cy: f32, outer: f32, inner: f32) -> Vec<(f32, f32)> {
    (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { outer } else { inner };
            let a = (-90.0 + 36.0 * i as f32).to_radians();
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// Paint `icon` centred on `c`, `size` px square.
pub fn icon(p: &Painter, icon: Icon, c: Pos2, size: f32, color: Color32) {
    let k = size / 24.0;
    let pen = Pen { p, c, k, stroke: Stroke::new((1.8 * k).max(1.1), color), color };
    match icon {
        Icon::Menu => {
            for y in [7.0, 12.0, 17.0] {
                pen.line(&[(4.0, y), (20.0, y)]);
            }
        }
        Icon::Power => {
            pen.arc(12.0, 13.1, 8.0, -56.0, 236.0);
            pen.line(&[(12.0, 3.0), (12.0, 12.0)]);
        }
        Icon::Back => {
            pen.line(&[(19.0, 12.0), (5.0, 12.0)]);
            pen.line(&[(11.0, 6.0), (5.0, 12.0), (11.0, 18.0)]);
        }
        Icon::Forward => {
            pen.line(&[(5.0, 12.0), (19.0, 12.0)]);
            pen.line(&[(13.0, 6.0), (19.0, 12.0), (13.0, 18.0)]);
        }
        Icon::Search => {
            pen.circle(11.0, 11.0, 7.0);
            pen.line(&[(20.0, 20.0), (16.0, 16.0)]);
        }
        Icon::Heart => pen.closed(&heart_points()),
        Icon::HeartFilled => pen.fill((12.0, 12.0), &heart_points()),
        Icon::Speaker => {
            pen.fill((7.5, 12.0), &[(3.0, 9.0), (7.0, 9.0), (12.0, 5.0), (12.0, 19.0), (7.0, 15.0), (3.0, 15.0)]);
            pen.arc(13.0, 12.0, 3.5, -55.0, 55.0);
            pen.arc(13.0, 12.0, 7.0, -55.0, 55.0);
        }
        Icon::Play => pen.fill((11.0, 12.0), &[(8.0, 5.0), (19.0, 12.0), (8.0, 19.0)]),
        Icon::Pause => {
            pen.rect(7.0, 5.0, 4.0, 14.0);
            pen.rect(13.0, 5.0, 4.0, 14.0);
        }
        Icon::Prev => {
            pen.rect(6.0, 6.0, 2.0, 12.0);
            pen.fill((15.0, 12.0), &[(9.5, 12.0), (18.0, 6.0), (18.0, 18.0)]);
        }
        Icon::Next => {
            pen.rect(16.0, 6.0, 2.0, 12.0);
            pen.fill((9.0, 12.0), &[(6.0, 6.0), (14.5, 12.0), (6.0, 18.0)]);
        }
        Icon::Info => {
            pen.circle(12.0, 12.0, 9.0);
            pen.line(&[(12.0, 11.0), (12.0, 17.0)]);
            pen.dot(12.0, 7.6, 1.1);
        }
        Icon::Eye => {
            let upper: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let x = 2.0 + 20.0 * i as f32 / 16.0;
                    (x, 12.0 - 7.0 * (std::f32::consts::PI * (x - 2.0) / 20.0).sin())
                })
                .collect();
            let mut all = upper.clone();
            all.extend(upper.iter().rev().map(|&(x, y)| (x, 24.0 - y)));
            pen.closed(&all);
            pen.circle(12.0, 12.0, 3.0);
        }
        Icon::Bolt => pen.closed(&[(13.0, 2.0), (4.0, 14.0), (11.0, 14.0), (10.0, 22.0), (19.0, 10.0), (12.0, 10.0)]),
        Icon::Expand => {
            pen.line(&[(14.0, 4.0), (20.0, 4.0), (20.0, 10.0)]);
            pen.line(&[(10.0, 20.0), (4.0, 20.0), (4.0, 14.0)]);
            pen.line(&[(20.0, 4.0), (13.0, 11.0)]);
            pen.line(&[(4.0, 20.0), (11.0, 13.0)]);
        }
        Icon::Close => {
            pen.line(&[(6.0, 6.0), (18.0, 18.0)]);
            pen.line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::Mini => {
            pen.closed(&[(3.0, 5.0), (21.0, 5.0), (21.0, 19.0), (3.0, 19.0)]);
            pen.rect(12.0, 12.0, 7.0, 5.0);
        }
        Icon::Home => pen.closed(&[
            (3.0, 11.0),
            (12.0, 4.0),
            (21.0, 11.0),
            (21.0, 21.0),
            (15.0, 21.0),
            (15.0, 15.0),
            (9.0, 15.0),
            (9.0, 21.0),
            (3.0, 21.0),
        ]),
        Icon::Star => pen.closed(&star_points(12.0, 12.5, 9.5, 4.2)),
        Icon::Tag => {
            pen.closed(&[(3.0, 12.0), (3.0, 4.0), (11.0, 4.0), (21.0, 14.0), (13.0, 22.0)]);
            pen.circle(7.5, 7.5, 1.3);
        }
        Icon::Globe | Icon::Globe2 => {
            pen.circle(12.0, 12.0, 9.0);
            pen.line(&[(3.0, 12.0), (21.0, 12.0)]);
            let half: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let a = (-90.0 + 180.0 * i as f32 / 16.0f32).to_radians();
                    (12.0 + 3.8 * a.cos(), 12.0 + 9.0 * a.sin())
                })
                .collect();
            pen.line(&half);
            let other: Vec<(f32, f32)> = half.iter().map(|&(x, y)| (24.0 - x, y)).collect();
            pen.line(&other);
        }
        Icon::Plug => {
            pen.line(&[(9.0, 2.0), (9.0, 7.0)]);
            pen.line(&[(15.0, 2.0), (15.0, 7.0)]);
            let mut body = vec![(6.0, 11.0), (6.0, 7.0), (18.0, 7.0), (18.0, 11.0)];
            body.extend((1..16).map(|i| {
                let a = (180.0 * i as f32 / 16.0f32).to_radians();
                (12.0 + 6.0 * a.cos(), 11.0 + 6.0 * a.sin())
            }));
            pen.closed(&body);
            pen.line(&[(12.0, 17.0), (12.0, 22.0)]);
        }
        Icon::Gear => {
            pen.circle(12.0, 12.0, 3.0);
            for (a, b) in [
                ((12.0, 2.0), (12.0, 5.0)),
                ((12.0, 19.0), (12.0, 22.0)),
                ((2.0, 12.0), (5.0, 12.0)),
                ((19.0, 12.0), (22.0, 12.0)),
                ((4.9, 4.9), (7.0, 7.0)),
                ((17.0, 17.0), (19.1, 19.1)),
                ((4.9, 19.1), (7.0, 17.0)),
                ((17.0, 7.0), (19.1, 4.9)),
            ] {
                pen.line(&[a, b]);
            }
        }
        Icon::Dice => {
            pen.closed(&[(4.0, 4.0), (20.0, 4.0), (20.0, 20.0), (4.0, 20.0)]);
            for (x, y) in [(8.5, 8.5), (15.5, 15.5), (12.0, 12.0), (15.5, 8.5), (8.5, 15.5)] {
                pen.dot(x, y, 1.3);
            }
        }
        Icon::Radio => {
            pen.dot(12.0, 12.0, 2.0);
            pen.arc(12.0, 12.0, 5.5, -50.0, 50.0);
            pen.arc(12.0, 12.0, 5.5, 130.0, 230.0);
            pen.arc(12.0, 12.0, 9.5, -50.0, 50.0);
            pen.arc(12.0, 12.0, 9.5, 130.0, 230.0);
        }
    }
}

/// A small filled 5-point star (list rows, rating).
pub fn star_shape(p: &Painter, c: Pos2, r: f32, color: Color32) {
    let pts: Vec<Pos2> = star_points(0.0, 0.0, r, r * 0.45)
        .into_iter()
        .map(|(x, y)| Pos2::new(c.x + x, c.y + y))
        .collect();
    fan(p, c, &pts, color);
}

/// Paint `n` of 5 stars left to right from `left_center`; returns the width.
pub fn stars(p: &Painter, pal: &Pal, left_center: Pos2, n: u8, r: f32) -> f32 {
    let step = r * 2.3;
    for i in 0..5u8 {
        let c = Pos2::new(left_center.x + r + i as f32 * step, left_center.y);
        star_shape(p, c, r, if i < n { pal.stars() } else { pal.star_off });
    }
    step * 5.0
}

/// Clickable 5-star rating; returns the new rating when one was clicked
/// (clicking the current level again clears it).
pub fn rating_input(ui: &mut Ui, pal: &Pal, rect: Rect, rating: u8, id: egui::Id) -> Option<u8> {
    let r = (rect.height() / 2.0).min(rect.width() / 11.5);
    let step = r * 2.3;
    let mut out = None;
    let mut hover_level = None;
    let mut hovered_resp = None;
    for i in 0..5u8 {
        let c = Pos2::new(rect.left() + r + i as f32 * step, rect.center().y);
        let hit = Rect::from_center_size(c, Vec2::splat(step));
        let resp = ui.interact(hit, id.with(i), Sense::click());
        if resp.hovered() {
            hover_level = Some(i + 1);
            hovered_resp = Some(resp.clone());
        }
        if resp.clicked() {
            out = Some(if rating == i + 1 { 0 } else { i + 1 });
        }
    }
    let show = hover_level.unwrap_or(rating);
    let p = ui.painter();
    for i in 0..5u8 {
        let c = Pos2::new(rect.left() + r + i as f32 * step, rect.center().y);
        let col = if i < show {
            if hover_level.is_some() {
                pal.stars().gamma_multiply(0.8)
            } else {
                pal.stars()
            }
        } else {
            pal.star_off
        };
        star_shape(p, c, r, col);
    }
    if let (Some(level), Some(resp)) = (hover_level, hovered_resp) {
        resp.on_hover_text(tn!("info.rate", level));
    }
    out
}

// ---- Cards ----------------------------------------------------------------------

/// Paint a card body (shadow + rounded fill) and its coloured header.
/// Returns the header rect and the body rect.
pub fn card_frame(p: &Painter, pal: &Pal, rect: Rect, header: Color32, header_h: f32) -> (Rect, Rect) {
    let r = CornerRadius::same(CARD_RADIUS);
    let shadow = epaint::Shadow {
        offset: [0, 10],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if pal.dark { 110 } else { 40 }),
    };
    p.add(shadow.as_shape(rect, r));
    p.rect_filled(rect, r, pal.card);
    let head = Rect::from_min_size(rect.min, Vec2::new(rect.width(), header_h));
    let top = CornerRadius { nw: CARD_RADIUS, ne: CARD_RADIUS, sw: 0, se: 0 };
    p.rect_filled(head, top, header);
    // The mockups' soft diagonal gloss over the header.
    let gloss = white_a(14);
    let mut mesh = epaint::Mesh::default();
    let inset = 6.0;
    let a = Pos2::new(head.left() + inset, head.top());
    let c = Pos2::new(head.left() + inset, head.bottom());
    let wide = Pos2::new(head.right() - inset, head.top());
    mesh.colored_vertex(a, gloss);
    mesh.colored_vertex(wide, gloss);
    mesh.colored_vertex(c, gloss);
    mesh.add_triangle(0, 1, 2);
    p.add(Shape::mesh(mesh));
    let body = Rect::from_min_max(Pos2::new(rect.left(), head.bottom()), rect.max);
    (head, body)
}

/// A clickable icon in a header; returns true when clicked.
pub fn header_icon(ui: &mut Ui, center: Pos2, ic: Icon, id: egui::Id, tip: &str) -> bool {
    let rect = Rect::from_center_size(center, Vec2::splat(36.0));
    let resp = ui.interact(rect, id, Sense::click());
    if resp.hovered() {
        ui.painter().circle_filled(center, 18.0, white_a(36));
    }
    icon(ui.painter(), ic, center, 24.0, Color32::WHITE);
    resp.on_hover_text(tip).clicked()
}

// ---- Pills ----------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct PillStyle {
    pub fill: Color32,
    pub border: Color32,
    pub text: Color32,
}

impl PillStyle {
    pub fn normal(pal: &Pal) -> Self {
        PillStyle { fill: Color32::TRANSPARENT, border: pal.pill_border, text: pal.sub }
    }
    pub fn active(fill: Color32) -> Self {
        PillStyle { fill, border: fill, text: Color32::WHITE }
    }
}

pub const PILL_H: f32 = 30.0;
const PILL_FONT: f32 = 13.0;

pub fn pill_width(ui: &Ui, label: &str, ic: Option<Icon>) -> f32 {
    text_width(ui, label, FontId::proportional(PILL_FONT)) + 26.0 + if ic.is_some() { 18.0 } else { 0.0 }
}

/// Paint an interactive pill in `rect`.
pub fn pill_at(ui: &mut Ui, rect: Rect, id: egui::Id, label: &str, ic: Option<Icon>, style: PillStyle) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    let fill = if resp.hovered() && style.fill == Color32::TRANSPARENT {
        style.border.gamma_multiply(0.25)
    } else if resp.hovered() {
        style.fill.gamma_multiply(0.9)
    } else {
        style.fill
    };
    p.rect(rect, CornerRadius::same(255), fill, Stroke::new(1.0_f32, style.border), egui::StrokeKind::Inside);
    let mut x = rect.left() + 13.0;
    if let Some(ic) = ic {
        icon(p, ic, Pos2::new(x + 7.0, rect.center().y), 14.0, style.text);
        x += 18.0;
    }
    p.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(PILL_FONT), style.text);
    resp
}

// ---- Round buttons -----------------------------------------------------------------

/// Outlined (or filled) circular icon button.
#[allow(clippy::too_many_arguments)]
pub fn round_button(
    ui: &mut Ui,
    center: Pos2,
    radius: f32,
    ic: Icon,
    fill: Option<Color32>,
    pal: &Pal,
    enabled: bool,
    id: egui::Id,
    tip: &str,
) -> bool {
    let rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    let resp = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let glyph = if fill.is_some() { Color32::WHITE } else { pal.text };
    match fill {
        Some(f) => {
            let f = if !enabled {
                f.gamma_multiply(0.45)
            } else if resp.is_pointer_button_down_on() {
                f.gamma_multiply(0.8)
            } else if resp.hovered() {
                f.lerp_to_gamma(Color32::WHITE, 0.12)
            } else {
                f
            };
            p.circle_filled(center, radius, f);
        }
        None => {
            if resp.hovered() && enabled {
                p.circle_filled(center, radius, pal.pill_border.gamma_multiply(0.35));
            }
            let border = if pal.dark { Color32::from_rgb(0x6a, 0x72, 0x7a) } else { pal.pill_border };
            p.circle_stroke(center, radius - 0.7, Stroke::new(1.4_f32, border));
        }
    }
    let g = if enabled { glyph } else { glyph.gamma_multiply(0.4) };
    icon(p, ic, center, radius * 0.95, g);
    resp.on_hover_text(tip).clicked()
}

/// Thin volume bar; returns the new value while dragged/clicked.
pub fn thin_slider(ui: &mut Ui, rect: Rect, value: f32, pal: &Pal, id: egui::Id) -> Option<f32> {
    let hit = rect.expand2(Vec2::new(0.0, 8.0));
    let resp = ui.interact(hit, id, Sense::click_and_drag());
    let mut out = None;
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            out = Some(((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
        }
    }
    let v = out.unwrap_or(value);
    let p = ui.painter();
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
    p.rect_filled(track, CornerRadius::same(2), pal.line);
    let fill = Rect::from_min_max(track.min, Pos2::new(track.left() + track.width() * v, track.bottom()));
    let fill_col = if pal.dark { Color32::from_rgb(0xcf, 0xd4, 0xd9) } else { pal.sub };
    p.rect_filled(fill, CornerRadius::same(2), fill_col);
    if resp.hovered() || resp.dragged() {
        p.circle_filled(Pos2::new(fill.right(), track.center().y), 6.0, fill_col);
    }
    out
}

// ---- Station rows -------------------------------------------------------------------

pub struct RowOpts {
    pub rank: Option<usize>,
    pub play_button: bool,
}

/// One station row: name, "genre · country · stars", bitrate on the
/// right; a speaker icon marks the playing station and the hovered row
/// gets the amber underline. Click plays; right-click offers more.
pub fn station_row(ui: &mut Ui, app: &mut BigWaveApp, pal: &Pal, station: &Station, opts: &RowOpts) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let playing = app.current.as_ref().is_some_and(|c| c.uuid == station.uuid);
    let p = ui.painter();
    if resp.hovered() {
        p.hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(2.0_f32, AMBER));
    } else {
        p.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0_f32, pal.line));
    }
    let mut x = rect.left() + 2.0;
    if let Some(rank) = opts.rank {
        p.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, rank.to_string(), FontId::proportional(13.0), pal.weak);
        x += 24.0;
    }
    if playing {
        icon(p, Icon::Speaker, Pos2::new(x + 8.0, rect.center().y), 16.0, pal.sub);
        x += 22.0;
    }
    let mut right = rect.right() - 2.0;
    if opts.play_button {
        let c = Pos2::new(right - 13.0, rect.center().y);
        let hovered = resp.hovered();
        let border = if pal.dark { Color32::from_rgb(0x6a, 0x72, 0x7a) } else { pal.pill_border };
        if hovered {
            p.circle_filled(c, 13.0, pal.primary);
        }
        p.circle_stroke(c, 12.5, Stroke::new(1.3_f32, if hovered { pal.primary } else { border }));
        icon(p, Icon::Play, Pos2::new(c.x + 0.5, c.y), 12.0, if hovered { Color32::WHITE } else { pal.sub });
        right -= 26.0 + 12.0;
    }
    let br = bitrate_short(station);
    let br_w = text_width(ui, &br, FontId::proportional(13.5));
    let p = ui.painter();
    p.text(Pos2::new(right, rect.center().y), Align2::RIGHT_CENTER, &br, FontId::proportional(13.5), pal.sub);
    let max_w = (right - br_w - 12.0 - x).max(20.0);
    let name = line_galley(ui, station.name.trim(), FontId::proportional(15.5), pal.text, max_w);
    let p = ui.painter();
    p.galley(Pos2::new(x, rect.top() + 7.0), name, pal.text);
    let mut sub: Vec<String> = Vec::new();
    let g = genre(station);
    if !g.is_empty() {
        sub.push(g);
    }
    let c = country_short(station);
    if !c.is_empty() {
        sub.push(c);
    }
    let sub_text = if sub.is_empty() { String::new() } else { sub.join(" · ") + " · " };
    let stars_w = 5.0 * 2.3 * 4.6;
    let sub_g = line_galley(ui, &sub_text, FontId::proportional(11.5), pal.weak, (max_w - stars_w).max(10.0));
    let sub_w = sub_g.size().x;
    let p = ui.painter();
    let sy = rect.top() + 36.0;
    p.galley(Pos2::new(x, sy - sub_g.size().y / 2.0), sub_g, pal.weak);
    stars(p, pal, Pos2::new(x + sub_w, sy), app.ratings.get(&station.uuid), 4.6);

    let resp = resp.on_hover_text(crate::widgets::station_card::summary_line(station));
    if resp.clicked() {
        app.play_station(station.clone());
    }
    resp.context_menu(|ui| row_menu(ui, app, station));
}

/// Right-click menu shared by the station rows.
pub fn row_menu(ui: &mut Ui, app: &mut BigWaveApp, station: &Station) {
    if ui.button(t!("cardsui.play")).clicked() {
        app.play_station(station.clone());
        ui.close_menu();
    }
    let saved = app.library.contains(&station.uuid);
    let label = if saved { t!("player.remove_library") } else { t!("player.save_library") };
    if ui.button(label).clicked() {
        app.library.toggle(station);
        ui.close_menu();
    }
    if ui.button(format!("✨ {}", t!("info.more_like"))).clicked() {
        app.more_like_station(station);
        ui.close_menu();
    }
    ui.separator();
    let rating = app.ratings.get(&station.uuid);
    ui.horizontal(|ui| {
        for star in 1..=5u8 {
            let filled = rating >= star;
            if ui
                .add(egui::Button::new(if filled { "★" } else { "☆" }).frame(false))
                .on_hover_text(tn!("info.rate", star))
                .clicked()
            {
                let new = if rating == star { 0 } else { star };
                set_rating(app, station, new);
                ui.close_menu();
            }
        }
    });
}

/// A soft fade from transparent to the card colour (bottom of lists).
pub fn fade(p: &Painter, rect: Rect, color: Color32) {
    let mut mesh = epaint::Mesh::default();
    let clear = Color32::from_rgba_premultiplied(0, 0, 0, 0);
    mesh.colored_vertex(rect.left_top(), clear);
    mesh.colored_vertex(rect.right_top(), clear);
    mesh.colored_vertex(rect.left_bottom(), color);
    mesh.colored_vertex(rect.right_bottom(), color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    p.add(Shape::mesh(mesh));
}

/// Empty/loading text centred in a list area.
pub fn list_note(ui: &mut Ui, pal: &Pal, text: &str) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(text).color(pal.weak).size(13.5));
    });
}

/// Rate a station and cache its record for the stars list.
pub fn set_rating(app: &mut BigWaveApp, station: &Station, stars: u8) {
    if app.ratings.set(&station.uuid, stars) > 0 {
        app.ratings.record(station);
    }
}

/// Thin vertical spectrum lines (behind the readout, in mood tiles and the
/// mini player). Bass sits in the middle, treble towards the edges; each
/// line gets a fixed pseudo-random jitter so the field looks organic.
/// Lines are centred on `rect`'s middle, or rise from its bottom when
/// `from_bottom` is set. They never flash: heights follow the smoothed band
/// levels, so this is safe at any frame rate.
pub fn spectrum_lines(p: &Painter, rect: Rect, color: Color32, bands: &[f32], spacing: f32, from_bottom: bool) {
    if rect.width() < spacing || bands.is_empty() {
        return;
    }
    let n = (rect.width() / spacing) as i32;
    let cx = rect.center().x;
    let half = n / 2;
    for i in -half..=half {
        let x = cx + i as f32 * spacing;
        if x < rect.left() || x > rect.right() {
            continue;
        }
        let d = (i.unsigned_abs() as f32 / half.max(1) as f32).min(1.0);
        let env = ((1.0 - d) * std::f32::consts::FRAC_PI_2).sin().powf(0.8);
        let band = ((d * (bands.len() - 1) as f32) as usize).min(bands.len() - 1);
        let level = bands[band].clamp(0.0, 1.0);
        let seed = (i.wrapping_mul(2_654_435_761u32 as i32) as u32) >> 24;
        let jitter = 0.45 + 0.55 * (seed as f32 / 255.0);
        let h = rect.height() * env * jitter * (0.22 + 0.78 * level);
        if h < 1.0 {
            continue;
        }
        let c = color.gamma_multiply(0.18 + 0.4 * (level * 1.5).min(1.0));
        let (top, bottom) = if from_bottom {
            (rect.bottom() - h, rect.bottom())
        } else {
            (rect.center().y - h / 2.0, rect.center().y + h / 2.0)
        };
        p.line_segment([Pos2::new(x, top), Pos2::new(x, bottom)], Stroke::new(1.0_f32, c));
    }
}

/// Station meta line: "Lounge · 192 kbps AAC · United Kingdom".
pub fn meta_line(s: &Station) -> String {
    let mut parts: Vec<String> = Vec::new();
    let tags: Vec<String> = s
        .tags
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .take(2)
        .map(|t| {
            let mut c = t.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect();
    parts.extend(tags);
    if s.bitrate > 0 {
        let codec = s.codec.trim().to_uppercase();
        parts.push(t!("cardsui.kbps_codec", kbps = s.bitrate, codec = codec).trim().to_string());
    } else if !s.codec.trim().is_empty() {
        parts.push(s.codec.trim().to_uppercase());
    }
    if !s.country.trim().is_empty() {
        parts.push(s.country.trim().to_string());
    }
    parts.join(" · ")
}
