//! Host-side photosensitivity guard.
//!
//! Counts dark→bright transitions of large areas (a "flash") in the overlay
//! and suppresses any flash that would start sooner than `1 / cap_hz` after
//! the previous one, by repainting the bright large shapes black. This runs
//! on the host, so it holds even if a plugin ignores `ctx.flash_cap`.

use crate::draw::{DrawCmd, Rgba};

/// A shape counts as "large" when it covers this fraction of the area.
const LARGE_AREA: f32 = 0.25;
/// Relative luminance above which a large area counts as bright.
const BRIGHT: f32 = 0.20;

#[derive(Debug, Clone)]
pub struct FlashLimiter {
    pub cap_hz: f64,
    last_onset: Option<f64>,
    bright_prev: bool,
    pub suppressed: u64,
}

impl FlashLimiter {
    pub fn new(cap_hz: f64) -> Self {
        Self { cap_hz: cap_hz.clamp(0.1, crate::HARD_MAX_FLASH_HZ), last_onset: None, bright_prev: false, suppressed: 0 }
    }

    pub fn set_cap(&mut self, hz: f64) {
        self.cap_hz = hz.clamp(0.1, crate::HARD_MAX_FLASH_HZ);
    }

    pub fn reset(&mut self) {
        self.last_onset = None;
        self.bright_prev = false;
    }

    fn is_bright_large(c: &DrawCmd) -> bool {
        let mut c2 = c.clone();
        c.solid_area() >= LARGE_AREA && c2.color_mut().luminance() >= BRIGHT
    }

    /// Filter one overlay frame at time `now` (seconds).
    pub fn filter(&mut self, now: f64, cmds: &mut [DrawCmd]) {
        // The last full fill wins, so only look at commands after it.
        let start = cmds.iter().rposition(|c| matches!(c, DrawCmd::Fill(_))).unwrap_or(0);
        let bright = cmds[start..].iter().any(Self::is_bright_large);
        if bright && !self.bright_prev {
            let min_gap = 1.0 / self.cap_hz;
            let too_soon = self.last_onset.is_some_and(|t| now - t < min_gap);
            if too_soon {
                for c in cmds.iter_mut() {
                    if Self::is_bright_large(c) {
                        *c.color_mut() = Rgba::BLACK;
                    }
                }
                self.suppressed += 1;
                self.bright_prev = false;
                return;
            }
            self.last_onset = Some(now);
        }
        self.bright_prev = bright;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_flash_rate() {
        let white = Rgba::parse("#ffffff").unwrap();
        let mut lim = FlashLimiter::new(3.0);
        let mut onsets = 0;
        let mut prev = false;
        // Plugin tries to flash at 30 Hz (alternating every frame at 60 fps) for 2 s.
        for f in 0..120 {
            let now = f as f64 / 60.0;
            let mut cmds = vec![DrawCmd::Fill(if f % 2 == 0 { white } else { Rgba::BLACK })];
            lim.filter(now, &mut cmds);
            let bright = cmds[0] == DrawCmd::Fill(white);
            if bright && !prev {
                onsets += 1;
            }
            prev = bright;
        }
        assert!(onsets <= 6 + 1, "onsets = {onsets}");
        assert!(onsets >= 5, "onsets = {onsets}");
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
                DrawCmd::Rect { x: 0.0, y: 0.0, w: 0.1, h: 0.1, color: Rgba::parse("#fff").unwrap() },
            ];
            lim.filter(f as f64 / 60.0, &mut cmds);
            assert_eq!(*cmds[1].color_mut(), Rgba::parse("#fff").unwrap());
        }
    }
}
