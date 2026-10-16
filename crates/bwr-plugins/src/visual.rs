//! Native visual scenes for plugins (`draw.visual`): a MilkDrop-style
//! framebuffer with feedback (zoom / rotate / warp / decay of the previous
//! frame) plus fast built-in scene renderers. Plugins only pass parameters
//! (scene, palette, motion, audio values) from Rhai; the per-pixel work is
//! done here in Rust, so it stays smooth at 60 fps.
//!
//! Safety: the engine is non-strobing by construction. The mean luminance
//! of its output may only change slowly (see [`MAX_LUM_STEP`]) and is capped
//! at [`MAX_MEAN_LUM`], whatever the plugin asks for. Anything drawn over it
//! with ordinary draw commands still goes through the host flash limiter.

use std::collections::VecDeque;

use crate::draw::Rgba;

/// The built-in scenes, in display order.
pub const SCENES: &[&str] = &["tunnel", "kaleido", "plasma", "flow", "sphere", "terrain", "stars", "mandala"];

/// Max change of mean output luminance per frame (0..1). Going from black to
/// the brightness cap takes about half a second at 60 fps.
pub const MAX_LUM_STEP: f32 = 0.02;
/// Cap on the mean output luminance (0..1).
pub const MAX_MEAN_LUM: f32 = 0.5;
/// Framebuffer height at quality 1.0 (width follows the aspect ratio).
pub const BASE_HEIGHT: usize = 360;

#[derive(Debug, Clone, PartialEq)]
pub struct VisualParams {
    /// Engine key: separate ids keep separate feedback buffers.
    pub id: String,
    pub scene: String,
    /// Optional second scene for crossfades, weighted by `mix` (0..1).
    pub scene_b: String,
    pub mix: f32,
    /// Seconds; drives all animation (pass ctx.now).
    pub time: f32,
    pub palette: Vec<Rgba>,
    /// Feedback: zoom (>1 zooms in), rotate (radians per frame), warp
    /// (0..1 wobble), decay (0..1 kept per frame).
    pub zoom: f32,
    pub rotate: f32,
    pub warp: f32,
    pub decay: f32,
    /// Palette offset (0..1, wraps).
    pub hue: f32,
    pub speed: f32,
    pub intensity: f32,
    /// Scene-specific detail (symmetry, density); 1.0 = default.
    pub detail: f32,
    /// Framebuffer resolution scale (0.25..2).
    pub quality: f32,
    pub level: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    /// Beat pulse 0..1 (decaying), used for gentle pulses, never flashes.
    pub pulse: f32,
    pub bands: Vec<f32>,
    /// Target rectangle in area fractions (default: the whole area).
    pub rect: [f32; 4],
}

impl Default for VisualParams {
    fn default() -> Self {
        Self {
            id: "main".into(),
            scene: "plasma".into(),
            scene_b: String::new(),
            mix: 0.0,
            time: 0.0,
            palette: Vec::new(),
            zoom: 1.0,
            rotate: 0.0,
            warp: 0.0,
            decay: 0.9,
            hue: 0.0,
            speed: 1.0,
            intensity: 1.0,
            detail: 1.0,
            quality: 1.0,
            level: 0.0,
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            pulse: 0.0,
            bands: Vec::new(),
            rect: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

type Px = [f32; 3];

pub struct VisualEngine {
    pub w: usize,
    pub h: usize,
    buf: Vec<Px>,
    tmp: Vec<Px>,
    out: Vec<u8>,
    lut: Vec<Px>,
    lut_src: Vec<Rgba>,
    smooth_lum: f32,
    last_time: Option<f32>,
    rng: u32,
    particles: Vec<[f32; 3]>,
    stars: Vec<[f32; 3]>,
    history: VecDeque<Vec<f32>>,
    hist_clock: f32,
    /// Mean luminance of the last output frame (after the guard).
    pub last_lum: f32,
}

fn default_palette() -> Vec<Rgba> {
    ["#000428", "#004e92", "#00f0ff", "#ff3cac", "#784ba0", "#ffcc00"].iter().filter_map(|s| Rgba::parse(s)).collect()
}

fn srgb(c: Rgba) -> Px {
    [c.0[0] as f32 / 255.0, c.0[1] as f32 / 255.0, c.0[2] as f32 / 255.0]
}

impl VisualEngine {
    pub fn new(w: usize, h: usize) -> Self {
        let n = w.max(1) * h.max(1);
        let mut e = Self {
            w: w.max(1),
            h: h.max(1),
            buf: vec![[0.0; 3]; n],
            tmp: vec![[0.0; 3]; n],
            out: vec![0; n * 4],
            lut: Vec::new(),
            lut_src: Vec::new(),
            smooth_lum: 0.0,
            last_time: None,
            rng: 0x9E37_79B9,
            particles: Vec::new(),
            stars: Vec::new(),
            history: VecDeque::new(),
            hist_clock: 0.0,
            last_lum: 0.0,
        };
        e.particles = (0..2400).map(|_| [e.rand() * 2.0 - 1.0, e.rand() * 2.0 - 1.0, e.rand()]).collect();
        e.stars = (0..700).map(|_| [e.rand() * 2.0 - 1.0, e.rand() * 2.0 - 1.0, e.rand()]).collect();
        e
    }

    /// Size for an area of `aspect` (w / h) at `quality`.
    pub fn size_for(aspect: f32, quality: f32) -> (usize, usize) {
        let h = ((BASE_HEIGHT as f32) * quality.clamp(0.25, 2.0)).round().max(16.0) as usize;
        let w = ((h as f32) * aspect.clamp(0.25, 4.0)).round().max(16.0) as usize;
        (w, h)
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    fn pal(&self, x: f32) -> Px {
        let n = self.lut.len();
        let f = x.rem_euclid(1.0) * n as f32;
        let i = f as usize % n;
        let j = (i + 1) % n;
        let t = f.fract();
        let (a, b) = (self.lut[i], self.lut[j]);
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
    }

    fn build_lut(&mut self, palette: &[Rgba]) {
        if !self.lut.is_empty() && self.lut_src == palette {
            return;
        }
        self.lut_src = palette.to_vec();
        let src: Vec<Px> = if palette.is_empty() { default_palette() } else { palette.to_vec() }.into_iter().map(srgb).collect();
        let n = 256;
        self.lut = (0..n)
            .map(|i| {
                let f = i as f32 / n as f32 * src.len() as f32;
                let (a, b) = (src[f as usize % src.len()], src[(f as usize + 1) % src.len()]);
                let t = f.fract();
                let t = t * t * (3.0 - 2.0 * t);
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
            })
            .collect();
    }

    fn band(p: &VisualParams, x: f32) -> f32 {
        if p.bands.is_empty() {
            return p.level;
        }
        let f = x.clamp(0.0, 1.0) * (p.bands.len() - 1) as f32;
        let i = f as usize;
        let j = (i + 1).min(p.bands.len() - 1);
        p.bands[i] + (p.bands[j] - p.bands[i]) * f.fract()
    }

    /// Render one frame; returns RGBA8 pixels (`w * h * 4`).
    pub fn render(&mut self, p: &VisualParams) -> &[u8] {
        self.build_lut(&p.palette);
        let dt = match self.last_time {
            Some(t) => (p.time - t).clamp(0.0, 0.1),
            None => 1.0 / 60.0,
        };
        self.last_time = Some(p.time);
        let t = p.time * p.speed;
        self.feedback(p, t);
        let mix = p.mix.clamp(0.0, 1.0);
        let a = p.scene.clone();
        if mix < 1.0 {
            self.scene(&a, p, t, dt, 1.0 - mix);
        }
        if mix > 0.0 && !p.scene_b.is_empty() {
            let b = p.scene_b.clone();
            self.scene(&b, p, t, dt, mix);
        }
        std::mem::swap(&mut self.buf, &mut self.tmp);
        self.output()
    }

    fn feedback(&mut self, p: &VisualParams, t: f32) {
        let (w, h) = (self.w, self.h);
        let (cx, cy, hf) = (w as f32 / 2.0, h as f32 / 2.0, h as f32);
        let z = 1.0 / (p.zoom.clamp(0.8, 1.25) + p.pulse * 0.01);
        let (s, c) = p.rotate.clamp(-0.2, 0.2).sin_cos();
        let warp = p.warp.clamp(0.0, 2.0) * 0.012;
        let decay = p.decay.clamp(0.0, 0.995);
        let buf = &self.buf;
        par_rows(&mut self.tmp, w, |y, row| {
            let v = (y as f32 - cy) / hf;
            for (x, out) in row.iter_mut().enumerate() {
                let u = (x as f32 - cx) / hf;
                let mut su = (c * u - s * v) * z;
                let mut sv = (s * u + c * v) * z;
                if warp > 0.0 {
                    su += warp * (v * 7.0 + t * 1.3).sin();
                    sv += warp * (u * 6.0 - t * 1.1).cos();
                }
                let px = sample(buf, w, h, su * hf + cx - 0.5, sv * hf + cy - 0.5);
                *out = [px[0] * decay, px[1] * decay, px[2] * decay];
            }
        });
    }

    fn splat(&mut self, x: f32, y: f32, c: Px, k: f32) {
        let (w, h) = (self.w as i32, self.h as i32);
        let (x0, y0) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        for (dx, dy, wt) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
            let (xx, yy) = (x0 + dx, y0 + dy);
            if xx >= 0 && yy >= 0 && xx < w && yy < h {
                let d = &mut self.tmp[(yy * w + xx) as usize];
                let a = wt * k;
                d[0] += c[0] * a;
                d[1] += c[1] * a;
                d[2] += c[2] * a;
            }
        }
    }

    /// Additive anti-aliased line in pixel coordinates.
    fn line(&mut self, a: (f32, f32), b: (f32, f32), c: Px, k: f32) {
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if !len.is_finite() || len > 4000.0 {
            return;
        }
        let steps = (len / 0.7).ceil().max(1.0) as usize;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.splat(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, c, k);
        }
    }

    fn scene(&mut self, name: &str, p: &VisualParams, t: f32, dt: f32, wgt: f32) {
        let (w, h) = (self.w, self.h);
        let (cx, cy, hf) = (w as f32 / 2.0, h as f32 / 2.0, h as f32);
        let inten = p.intensity.clamp(0.0, 2.0) * wgt;
        let hue = p.hue;
        let energy = 0.6 + p.bass * 0.6 + p.pulse * 0.3;
        match name {
            "plasma" | "tunnel" | "kaleido" => {
                let lut = &self.lut;
                par_rows(&mut self.tmp, w, |y, row| {
                    let v = (y as f32 - cy) / hf;
                    for (x, d) in row.iter_mut().enumerate() {
                        let u = (x as f32 - cx) / hf;
                        let (c, a) = pixel_scene(name, lut, p, t, energy, u, v);
                        let a = a * inten;
                        d[0] += (c[0] - d[0]) * a;
                        d[1] += (c[1] - d[1]) * a;
                        d[2] += (c[2] - d[2]) * a;
                    }
                });
            }
            "flow" => {
                let n = ((self.particles.len() as f32) * p.detail.clamp(0.25, 1.0)) as usize;
                let speed = (0.18 + p.level * 0.6 + p.pulse * 0.2) * dt;
                let aspect = w as f32 / hf;
                for i in 0..n {
                    let [mut x, mut y, mut age] = self.particles[i];
                    let ang = (x * 2.3 + t * 0.21).sin() * 2.2 + (y * 2.7 - t * 0.17).cos() * 2.2 + (x * y * 3.0 + t * 0.1).sin();
                    let (nx, ny) = (x + ang.cos() * speed, y + ang.sin() * speed);
                    let a = ((x * 0.5 + 0.5) * w as f32, (y * 0.5 + 0.5) * hf);
                    let b = ((nx * 0.5 + 0.5) * w as f32, (ny * 0.5 + 0.5) * hf);
                    let c = self.pal(hue + (i as f32 / n as f32) * 0.35 + age * 0.2);
                    self.line(a, b, c, 0.5 * inten * (0.6 + p.treble * 0.8));
                    x = nx;
                    y = ny;
                    age += dt * 0.25;
                    if age > 1.0 || x.abs() > 1.05 || y.abs() > 1.05 {
                        x = self.rand() * 2.0 - 1.0;
                        y = self.rand() * 2.0 - 1.0;
                        age = 0.0;
                    }
                    let _ = aspect;
                    self.particles[i] = [x, y, age];
                }
            }
            "sphere" => {
                let rad = 0.32 * (1.0 + p.pulse * 0.08 + p.bass * 0.06);
                let (ry, rx) = (t * 0.5, t * 0.31);
                let lat = (10.0 * p.detail.clamp(0.5, 2.0)) as usize;
                let lon = (16.0 * p.detail.clamp(0.5, 2.0)) as usize;
                let seg = 48;
                let proj = |th: f32, ph: f32, rr: f32| -> (f32, f32) {
                    let (x, y, z) = (rr * th.sin() * ph.cos(), rr * th.cos(), rr * th.sin() * ph.sin());
                    let (x, z) = (x * ry.cos() + z * ry.sin(), -x * ry.sin() + z * ry.cos());
                    let (y, z) = (y * rx.cos() - z * rx.sin(), y * rx.sin() + z * rx.cos());
                    let s = 1.6 / (2.2 + z);
                    (cx + x * s * hf, cy + y * s * hf)
                };
                let pi = std::f32::consts::PI;
                for i in 1..lat {
                    let th = pi * i as f32 / lat as f32;
                    let c = self.pal(hue + i as f32 / lat as f32 * 0.5);
                    for j in 0..seg {
                        let (p0, p1) = (std::f32::consts::TAU * j as f32 / seg as f32, std::f32::consts::TAU * (j + 1) as f32 / seg as f32);
                        let r0 = rad * (1.0 + 0.25 * Self::band(p, j as f32 / seg as f32));
                        let r1 = rad * (1.0 + 0.25 * Self::band(p, (j + 1) as f32 / seg as f32));
                        let (a, b) = (proj(th, p0, r0), proj(th, p1, r1));
                        self.line(a, b, c, 0.22 * inten);
                    }
                }
                for j in 0..lon {
                    let ph = std::f32::consts::TAU * j as f32 / lon as f32;
                    let c = self.pal(hue + 0.5 + j as f32 / lon as f32 * 0.3);
                    for i in 0..seg {
                        let (t0, t1) = (pi * i as f32 / seg as f32, pi * (i + 1) as f32 / seg as f32);
                        let r = rad * (1.0 + 0.25 * Self::band(p, j as f32 / lon as f32));
                        let (a, b) = (proj(t0, ph, r), proj(t1, ph, r));
                        self.line(a, b, c, 0.22 * inten);
                    }
                }
            }
            "terrain" => {
                self.hist_clock += dt;
                if self.hist_clock >= 1.0 / 30.0 || self.history.is_empty() {
                    self.hist_clock = 0.0;
                    let cols = 48;
                    let row: Vec<f32> = (0..cols)
                        .map(|i| {
                            let x = (i as f32 / (cols - 1) as f32 - 0.5).abs() * 2.0;
                            Self::band(p, 1.0 - x) * (1.0 + p.pulse * 0.2)
                        })
                        .collect();
                    self.history.push_front(row);
                    self.history.truncate(40);
                }
                let rows = self.history.len();
                let hist: Vec<Vec<f32>> = self.history.iter().cloned().collect();
                let proj = |xn: f32, zr: f32, hgt: f32| -> (f32, f32) {
                    let z = 0.6 + zr * 3.0;
                    let s = 1.0 / z;
                    (cx + xn * 1.6 * s * hf, cy + (0.32 - hgt * 0.45) * s * hf + 0.05 * hf - 0.15 * hf * (1.0 - s))
                };
                for (ri, row) in hist.iter().enumerate() {
                    let zr = ri as f32 / 40.0;
                    let fade = 1.0 - ri as f32 / rows.max(1) as f32;
                    let cols = row.len();
                    for ci in 0..cols - 1 {
                        let (x0, x1) = (ci as f32 / (cols - 1) as f32 - 0.5, (ci + 1) as f32 / (cols - 1) as f32 - 0.5);
                        let c = self.pal(hue + row[ci] * 0.6 + zr * 0.2);
                        self.line(proj(x0, zr, row[ci]), proj(x1, zr, row[ci + 1]), c, 0.5 * inten * fade);
                        if ri + 1 < rows && ci % 3 == 0 {
                            let nxt = &hist[ri + 1];
                            self.line(proj(x0, zr, row[ci]), proj(x0, (ri + 1) as f32 / 40.0, nxt[ci]), c, 0.3 * inten * fade);
                        }
                    }
                }
            }
            "stars" => {
                let speed = (0.25 + p.level * 1.2 + p.pulse * 0.4) * dt * p.speed.max(0.1);
                let n = ((self.stars.len() as f32) * p.detail.clamp(0.25, 1.0)) as usize;
                let rot = t * 0.05;
                for i in 0..n {
                    let [x, y, z] = self.stars[i];
                    let nz = z - speed;
                    let pr = |zz: f32| {
                        let s = 0.5 / zz.max(0.02);
                        let (rx, ry) = (x * rot.cos() - y * rot.sin(), x * rot.sin() + y * rot.cos());
                        (cx + rx * s * hf, cy + ry * s * hf)
                    };
                    if nz <= 0.02 {
                        let (a, b) = (self.rand() * 2.0 - 1.0, self.rand() * 2.0 - 1.0);
                        self.stars[i] = [a, b, 1.0];
                        continue;
                    }
                    let c = self.pal(hue + i as f32 * 0.013);
                    let k = (1.0 - nz).powi(2) * 1.2 * inten;
                    self.line(pr(z), pr(nz), c, k);
                    self.stars[i] = [x, y, nz];
                }
            }
            "mandala" => {
                let sym = (2.0 * p.detail.clamp(0.5, 3.0)).round().max(1.0) as usize;
                let bars = 48;
                let r0 = 0.15 + p.pulse * 0.02;
                for s in 0..sym {
                    for k in 0..bars {
                        let f = k as f32 / bars as f32;
                        let mirrored = if f < 0.5 { f * 2.0 } else { (1.0 - f) * 2.0 };
                        let amp = Self::band(p, mirrored);
                        let ang = std::f32::consts::TAU * (s as f32 + f) / sym as f32 + t * 0.2;
                        let (sa, ca) = ang.sin_cos();
                        let r1 = r0 + 0.04 + amp * 0.3;
                        let c = self.pal(hue + mirrored * 0.5 + t * 0.02);
                        let a = (cx + ca * r0 * hf, cy + sa * r0 * hf);
                        let b = (cx + ca * r1 * hf, cy + sa * r1 * hf);
                        self.line(a, b, c, 0.3 * inten);
                        self.splat(b.0, b.1, c, 0.5 * inten);
                    }
                }
                // Inner rotating polygon.
                let m = 6;
                for i in 0..m {
                    let (a0, a1) = (t * -0.4 + std::f32::consts::TAU * i as f32 / m as f32, t * -0.4 + std::f32::consts::TAU * (i + 1) as f32 / m as f32);
                    let rr = r0 * 0.8;
                    let c = self.pal(hue + 0.25);
                    self.line((cx + a0.cos() * rr * hf, cy + a0.sin() * rr * hf), (cx + a1.cos() * rr * hf, cy + a1.sin() * rr * hf), c, 0.8 * inten);
                }
            }
            _ => {}
        }
    }

    fn output(&mut self) -> &[u8] {
        // Tone map, then the non-strobe guard on mean luminance, applied in
        // output space so the visible brightness changes slowly.
        let n = self.buf.len();
        let mut sum = 0.0f32;
        for i in 0..n {
            let tc = tone(self.buf[i]);
            sum += lum(tc);
            let o = &mut self.out[i * 4..i * 4 + 4];
            o[0] = (tc[0] * 255.0) as u8;
            o[1] = (tc[1] * 255.0) as u8;
            o[2] = (tc[2] * 255.0) as u8;
            o[3] = 255;
        }
        let mean = sum / n.max(1) as f32;
        let lo = (self.smooth_lum - MAX_LUM_STEP).max(0.0);
        let hi = (self.smooth_lum + MAX_LUM_STEP).min(MAX_MEAN_LUM);
        let target = mean.clamp(lo, hi.max(lo));
        let scale = if mean > 1e-4 { (target / mean).min(4.0) } else { 1.0 };
        if (scale - 1.0).abs() > 1e-3 {
            for i in 0..n {
                let o = &mut self.out[i * 4..i * 4 + 3];
                for v in o.iter_mut() {
                    *v = (*v as f32 * scale).min(255.0) as u8;
                }
            }
        }
        // Keep the feedback buffer itself from running away in brightness.
        if mean > MAX_MEAN_LUM {
            let k = (MAX_MEAN_LUM / mean).max(0.5);
            for c in self.buf.iter_mut() {
                c[0] *= k;
                c[1] *= k;
                c[2] *= k;
            }
        }
        self.smooth_lum = self.measured_lum();
        self.last_lum = self.smooth_lum;
        &self.out
    }

    /// Mean output luminance measured from the RGBA output (for tests).
    pub fn measured_lum(&self) -> f32 {
        let n = self.out.len() / 4;
        let mut s = 0.0;
        for i in 0..n {
            let o = &self.out[i * 4..i * 4 + 3];
            s += lum([o[0] as f32 / 255.0, o[1] as f32 / 255.0, o[2] as f32 / 255.0]);
        }
        s / n.max(1) as f32
    }
}

fn pal_lut(lut: &[Px], x: f32) -> Px {
    let n = lut.len();
    let f = x.rem_euclid(1.0) * n as f32;
    let i = f as usize % n;
    let j = (i + 1) % n;
    let t = f.fract();
    let (a, b) = (lut[i], lut[j]);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Run `f(y, row)` over every row of `out` (width `w`) on several threads.
fn par_rows(out: &mut [Px], w: usize, f: impl Fn(usize, &mut [Px]) + Sync) {
    let h = out.len() / w.max(1);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 8);
    if threads == 1 || h < 32 {
        for (y, row) in out.chunks_mut(w).enumerate() {
            f(y, row);
        }
        return;
    }
    let rows_per = h.div_ceil(threads);
    std::thread::scope(|sc| {
        for (ci, chunk) in out.chunks_mut(rows_per * w).enumerate() {
            let f = &f;
            sc.spawn(move || {
                for (r, row) in chunk.chunks_mut(w).enumerate() {
                    f(ci * rows_per + r, row);
                }
            });
        }
    });
}

/// Per-pixel scenes: colour and blend amount at (u, v) (height units).
fn pixel_scene(name: &str, lut: &[Px], p: &VisualParams, t: f32, energy: f32, u: f32, v: f32) -> (Px, f32) {
    let hue = p.hue;
    let scale = |c: Px, m: f32| [c[0] * m, c[1] * m, c[2] * m];
    match name {
        "plasma" => {
            let k = 5.0 + p.detail.clamp(0.2, 4.0) * 4.0;
            let f = (u * k + t).sin()
                + (v * k * 0.9 - t * 0.7).sin()
                + ((u + v) * k * 0.6 + t * 0.5).sin()
                + ((u * u + v * v).sqrt() * k * 1.4 - t * 1.2 - p.bass * 2.0).sin();
            let c = pal_lut(lut, f * 0.15 + hue + t * 0.03);
            let band = 0.5 + 0.5 * (f * 3.0 + t).sin();
            let m = 0.12 + 0.88 * band * band * band;
            (scale(c, m * energy * 1.2), 0.4)
        }
        "tunnel" => {
            let rings = 4.0 * p.detail.clamp(0.2, 4.0);
            let r = (u * u + v * v).sqrt() + 1e-3;
            let ang = v.atan2(u) / std::f32::consts::PI + t * 0.05;
            let depth = 0.3 / r + t * 1.6;
            let a = (depth * rings * std::f32::consts::PI).sin();
            let b = (ang * 6.0 * std::f32::consts::PI + depth * 0.8).sin();
            let check = (a * b * 6.0).clamp(-1.0, 1.0) * 0.5 + 0.5;
            let fog = (r * 3.0).min(1.0);
            let c = pal_lut(lut, depth * 0.06 + ang * 0.5 + hue);
            let m = (0.08 + 0.92 * check) * fog * energy;
            (scale(c, m * 1.2), 0.5)
        }
        _ => {
            // kaleido
            let n = (6.0 * p.detail.clamp(0.5, 3.0)).round().max(3.0);
            let seg = std::f32::consts::TAU / n;
            let r = (u * u + v * v).sqrt();
            let a = (v.atan2(u) + t * 0.15).rem_euclid(seg);
            let a = (a - seg / 2.0).abs();
            let (fu, fv) = (r * a.cos(), r * a.sin());
            let f = (fu * 17.0 + t).sin() * (fv * 19.0 - t * 1.3).cos()
                + (r * 9.0 - t * 2.0 - p.bass * 3.0).sin() * 0.7
                + ((fu - fv) * 7.0 + t * 0.6).sin() * 0.5;
            let c = pal_lut(lut, f * 0.2 + r * 0.8 + hue + t * 0.05);
            let edge = (f.abs() * 1.4).min(1.0);
            let m = edge * edge * (1.25 - r).clamp(0.0, 1.0) * energy * 1.3;
            (scale(c, m), 0.32)
        }
    }
}

fn tone(c: Px) -> Px {
    let f = |v: f32| (1.0 - (-v.max(0.0) * 1.6).exp()).min(1.0);
    [f(c[0]), f(c[1]), f(c[2])]
}

fn lum(c: Px) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn sample(buf: &[Px], w: usize, h: usize, x: f32, y: f32) -> Px {
    if x < 0.0 || y < 0.0 || x >= (w - 1) as f32 || y >= (h - 1) as f32 {
        return [0.0; 3];
    }
    let (x0, y0) = (x as usize, y as usize);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let i = y0 * w + x0;
    let (a, b, c, d) = (buf[i], buf[i + 1], buf[i + w], buf[i + w + 1]);
    let mut o = [0.0; 3];
    for k in 0..3 {
        let top = a[k] + (b[k] - a[k]) * fx;
        let bot = c[k] + (d[k] - c[k]) * fx;
        o[k] = top + (bot - top) * fy;
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(scene: &str) -> VisualParams {
        VisualParams { scene: scene.into(), zoom: 1.02, rotate: 0.01, warp: 0.5, decay: 0.92, level: 0.5, bass: 0.6, bands: vec![0.5; 32], ..Default::default() }
    }

    #[test]
    fn every_scene_draws_something() {
        for s in SCENES {
            let mut e = VisualEngine::new(160, 90);
            for f in 0..90 {
                let mut p = params(s);
                p.time = f as f32 / 60.0;
                e.render(&p);
            }
            assert!(e.measured_lum() > 0.01, "{s} is black ({})", e.measured_lum());
        }
    }

    #[test]
    fn luminance_never_jumps() {
        // Beat pulses, full bass and instant scene cuts: still no flash.
        for s in SCENES {
            let mut e = VisualEngine::new(128, 72);
            let mut prev = 0.0;
            for f in 0..240 {
                let mut p = params(if f % 20 < 10 { s } else { "plasma" });
                p.time = f as f32 / 60.0;
                p.pulse = if f % 15 == 0 { 1.0 } else { 0.0 };
                p.bass = if f % 30 < 2 { 1.0 } else { 0.0 };
                p.intensity = 2.0;
                p.decay = 0.99;
                e.render(&p);
                let l = e.measured_lum();
                assert!(l <= MAX_MEAN_LUM + 0.03, "{s}: too bright {l}");
                assert!((l - prev).abs() <= MAX_LUM_STEP * 1.6 + 0.01, "{s} frame {f}: {prev} -> {l}");
                prev = l;
            }
        }
    }

    #[test]
    fn crossfade_mixes_two_scenes() {
        let mut e = VisualEngine::new(64, 36);
        let mut p = params("plasma");
        p.scene_b = "stars".into();
        p.mix = 0.5;
        for f in 0..30 {
            p.time = f as f32 / 60.0;
            e.render(&p);
        }
        assert!(e.measured_lum() > 0.0);
    }
}
