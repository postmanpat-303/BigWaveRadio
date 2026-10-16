//! Host-side photosensitivity guard.
//!
//! Estimates each overlay frame's average luminance (sampling a grid; ring
//! discs count with their *average* colour, so moving rings are motion, not
//! flicker). A "flash" is a rise of at least [`FLASH_DELTA`] over the
//! darkest level since the previous flash. Any flash that would start
//! sooner than `1 / cap_hz` after the previous one is suppressed by
//! painting the large bright shapes (fills, big rects/circles) black, or,
//! if that isn't enough, the whole frame black. This runs on the host, so
//! it holds even if a plugin ignores `ctx.flash_cap`.

use crate::draw::{DrawCmd, Rgba};

/// A shape counts as "large" when it covers this fraction of the area.
const LARGE_AREA: f32 = 0.25;
/// Rise in average relative luminance that counts as a flash (WCAG: 10 %).
pub const FLASH_DELTA: f32 = 0.10;
const GRID_X: usize = 16;
const GRID_Y: usize = 12;

#[derive(Debug, Clone)]
pub struct FlashLimiter {
    pub cap_hz: f64,
    /// Overlay width / height, for round shapes.
    pub aspect: f32,
    last_onset: Option<f64>,
    high: bool,
    trough: f32,
    peak: f32,
    pub suppressed: u64,
}

/// Average relative luminance of a frame (0..1).
pub fn frame_luminance(cmds: &[DrawCmd], aspect: f32) -> f32 {
    let mut total = 0.0;
    for gy in 0..GRID_Y {
        for gx in 0..GRID_X {
            let (x, y) = ((gx as f32 + 0.5) / GRID_X as f32, (gy as f32 + 0.5) / GRID_Y as f32);
            let mut l = 0.0f32;
            for c in cmds {
                if let Some((cl, a)) = c.sample(x, y, aspect) {
                    l = l * (1.0 - a) + cl * a;
                }
            }
            total += l;
        }
    }
    total / (GRID_X * GRID_Y) as f32
}

impl FlashLimiter {
    pub fn new(cap_hz: f64) -> Self {
        Self {
            cap_hz: cap_hz.clamp(0.1, crate::HARD_MAX_FLASH_HZ),
            aspect: 16.0 / 9.0,
            last_onset: None,
            high: false,
            trough: 0.0,
            peak: 0.0,
            suppressed: 0,
        }
    }

    pub fn set_cap(&mut self, hz: f64) {
        self.cap_hz = hz.clamp(0.1, crate::HARD_MAX_FLASH_HZ);
    }

    pub fn reset(&mut self) {
        self.last_onset = None;
        self.high = false;
        self.trough = 0.0;
        self.peak = 0.0;
    }

    fn darken(cmds: &mut [DrawCmd]) {
        for c in cmds.iter_mut() {
            let large = c.solid_area() >= LARGE_AREA;
            if let DrawCmd::Shape { fill: Some(f), .. } = c {
                if large && f.luminance() > 0.02 {
                    *f = Rgba::BLACK;
                }
            }
            if let Some(col) = c.color_mut() {
                if large && col.luminance() > 0.02 {
                    *col = Rgba::BLACK;
                }
            }
        }
    }

    /// Filter one overlay frame at time `now` (seconds).
    pub fn filter(&mut self, now: f64, cmds: &mut Vec<DrawCmd>) {
        let mut l = frame_luminance(cmds, self.aspect);
        if self.high {
            self.peak = self.peak.max(l);
            if l <= self.peak - FLASH_DELTA {
                self.high = false;
                self.trough = l;
            }
            return;
        }
        self.trough = self.trough.min(l);
        if l - self.trough < FLASH_DELTA {
            return;
        }
        let too_soon = self.last_onset.is_some_and(|t| now - t < 1.0 / self.cap_hz);
        if too_soon {
            self.suppressed += 1;
            Self::darken(cmds);
            l = frame_luminance(cmds, self.aspect);
            if l - self.trough >= FLASH_DELTA {
                cmds.clear();
                cmds.push(DrawCmd::Fill(Rgba::BLACK));
                l = 0.0;
            }
            self.trough = self.trough.min(l);
            return;
        }
        self.last_onset = Some(now);
        self.high = true;
        self.peak = l;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white() -> Rgba {
        Rgba::parse("#ffffff").unwrap()
    }

    fn count_onsets(lim: &mut FlashLimiter, frames: usize, mut make: impl FnMut(usize) -> Vec<DrawCmd>) -> usize {
        let mut onsets = 0;
        let mut prev = 0.0;
        let mut trough = 0.0f32;
        let mut high = false;
        for f in 0..frames {
            let mut cmds = make(f);
            lim.filter(f as f64 / 60.0, &mut cmds);
            let l = frame_luminance(&cmds, lim.aspect);
            if !high && l - trough >= FLASH_DELTA {
                onsets += 1;
                high = true;
                prev = l;
            } else if high {
                prev = f32::max(prev, l);
                if l <= prev - FLASH_DELTA {
                    high = false;
                    trough = l;
                }
            } else {
                trough = trough.min(l);
            }
        }
        onsets
    }

    #[test]
    fn caps_flash_rate() {
        let mut lim = FlashLimiter::new(3.0);
        // Plugin tries to flash at 30 Hz for 2 s.
        let n = count_onsets(&mut lim, 120, |f| vec![DrawCmd::Fill(if f % 2 == 0 { white() } else { Rgba::BLACK })]);
        assert!((5..=7).contains(&n), "onsets = {n}");
    }

    #[test]
    fn hard_ceiling() {
        assert_eq!(FlashLimiter::new(100.0).cap_hz, crate::HARD_MAX_FLASH_HZ);
    }

    #[test]
    fn small_shapes_are_not_flashes() {
        let mut lim = FlashLimiter::new(1.0);
        for f in 0..10 {
            let mut cmds = vec![
                DrawCmd::Fill(Rgba::BLACK),
                DrawCmd::Rect { x: 0.0, y: 0.0, w: 0.1, h: 0.1, color: white() },
            ];
            lim.filter(f as f64 / 60.0, &mut cmds);
            assert_eq!(cmds[1].color_mut().copied(), Some(white()));
        }
    }

    fn rings(cx: f32, cy: f32, colors: Vec<Rgba>) -> DrawCmd {
        DrawCmd::Rings { cx, cy, radius: 0.9, width: 0.015, colors }
    }

    #[test]
    fn moving_rings_are_not_flashes() {
        let clear = Rgba([0, 0, 0, 0]);
        let mut lim = FlashLimiter::new(3.0);
        let mut lums = Vec::new();
        for f in 0..600 {
            let t = f as f32 / 60.0;
            let mut cmds = vec![
                DrawCmd::Fill(Rgba::BLACK),
                rings(0.5 + 0.3 * (0.63 * t).sin(), 0.5 + 0.25 * (0.91 * t + 1.0).sin(), vec![white(), clear]),
                rings(0.5 + 0.3 * (0.77 * t + 2.0).sin(), 0.5 + 0.25 * (0.53 * t).sin(), vec![clear, Rgba::BLACK]),
            ];
            let before = cmds.clone();
            lim.filter(f as f64 / 60.0, &mut cmds);
            if f > 0 {
                assert_eq!(cmds, before, "frame {f}: rings were suppressed");
            }
            lums.push(frame_luminance(&cmds, lim.aspect));
        }
        // Frame-to-frame change stays tiny: swinging rings are motion.
        let max_step = lums.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(max_step < 0.01, "max per-frame luminance step {max_step}");
    }

    #[test]
    fn flashes_over_rings_are_still_capped() {
        let clear = Rgba([0, 0, 0, 0]);
        let mut lim = FlashLimiter::new(3.0);
        let n = count_onsets(&mut lim, 120, |f| {
            let on = f % 2 == 0;
            let ink = if on { Rgba::BLACK } else { white() };
            vec![DrawCmd::Fill(if on { white() } else { Rgba::BLACK }), rings(0.4, 0.5, vec![ink, clear]), rings(0.6, 0.5, vec![clear, if on { white() } else { Rgba::BLACK }])]
        });
        assert!((5..=7).contains(&n), "onsets = {n}");
    }
}
