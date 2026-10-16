//! Lightweight beat / break detection on top of the band levels.
//!
//! Not a studio-grade tracker: a bass-energy onset detector with a
//! refractory period, a median-interval BPM estimate and a "quiet" flag for
//! breaks (silence or a sharp drop in loudness). Good enough to drive
//! visuals at 30–60 fps on the UI thread.

use std::collections::VecDeque;

use crate::BANDS;

/// Per-frame audio features handed to plugins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioFeatures {
    /// RMS loudness 0..1.
    pub level: f32,
    /// Sample peak 0..1.
    pub peak: f32,
    /// Average band level of bass (~40–150 Hz), mid, treble (0..1).
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    /// True on the frame a beat (bass onset) was detected.
    pub beat: bool,
    /// Tempo estimate in beats per minute, 0 when unknown.
    pub bpm: f32,
    /// True while the track is in a break / quiet section.
    pub quiet: bool,
}

impl Default for AudioFeatures {
    fn default() -> Self {
        Self { level: 0.0, peak: 0.0, bass: 0.0, mid: 0.0, treble: 0.0, beat: false, bpm: 0.0, quiet: true }
    }
}

/// Band index ranges (log-spaced 40 Hz..12 kHz over 32 bands).
const BASS: std::ops::Range<usize> = 0..6;
const MID: std::ops::Range<usize> = 6..20;
const TREBLE: std::ops::Range<usize> = 20..BANDS;

fn avg(b: &[f32]) -> f32 {
    if b.is_empty() { 0.0 } else { b.iter().sum::<f32>() / b.len() as f32 }
}

#[derive(Debug, Default)]
pub struct BeatTracker {
    bass_avg: f32,
    prev_bass: f32,
    last_beat: Option<f64>,
    intervals: VecDeque<f64>,
    level_slow: f32,
    level_fast: f32,
    quiet_since: Option<f64>,
    quiet: bool,
    started: bool,
}

impl BeatTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one analysis frame. `now` is seconds on any monotonic clock.
    pub fn update(&mut self, now: f64, bands: &[f32; BANDS], level: f32, peak: f32) -> AudioFeatures {
        let bass = avg(&bands[BASS]);
        let mid = avg(&bands[MID]);
        let treble = avg(&bands[TREBLE]);

        if !self.started {
            self.started = true;
            self.bass_avg = bass;
            self.level_slow = level;
            self.level_fast = level;
        }

        // --- Breaks: fast vs slow loudness envelopes ------------------------
        self.level_fast += (level - self.level_fast) * 0.25;
        self.level_slow += (level - self.level_slow) * 0.01;
        let low = self.level_fast < 0.012 || self.level_fast < self.level_slow * 0.35;
        if low {
            let since = *self.quiet_since.get_or_insert(now);
            if now - since >= 0.30 {
                self.quiet = true;
            }
        } else {
            self.quiet_since = None;
            if self.level_fast > self.level_slow * 0.5 {
                self.quiet = false;
            }
        }

        // --- Beats: bass onset over a moving average -------------------------
        let rising = bass > self.prev_bass;
        let threshold = self.bass_avg * 1.25 + 0.06;
        let refractory = self.last_beat.map_or(true, |t| now - t >= 0.25);
        let beat = !self.quiet && rising && bass > threshold && refractory;
        self.bass_avg += (bass - self.bass_avg) * 0.08;
        self.prev_bass = bass;

        if beat {
            if let Some(t) = self.last_beat {
                let dt = now - t;
                if (0.27..=1.5).contains(&dt) {
                    self.intervals.push_back(dt);
                    if self.intervals.len() > 16 {
                        self.intervals.pop_front();
                    }
                } else if dt > 3.0 {
                    self.intervals.clear();
                }
            }
            self.last_beat = Some(now);
        }

        AudioFeatures { level, peak, bass, mid, treble, beat, bpm: self.bpm(), quiet: self.quiet }
    }

    /// Median-interval tempo estimate (60..220 BPM), 0 when unknown.
    pub fn bpm(&self) -> f32 {
        if self.intervals.len() < 4 {
            return 0.0;
        }
        let mut v: Vec<f64> = self.intervals.iter().copied().collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = v[v.len() / 2];
        (60.0 / median).clamp(40.0, 222.0) as f32
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(bass: f32, rest: f32) -> [f32; BANDS] {
        let mut b = [rest; BANDS];
        for x in &mut b[BASS] {
            *x = bass;
        }
        b
    }

    #[test]
    fn detects_120_bpm_kick() {
        let mut t = BeatTracker::new();
        let mut beats = 0;
        let fps = 60.0;
        for i in 0..(fps as usize * 10) {
            let now = i as f64 / fps;
            let phase = now % 0.5; // kick every 0.5 s = 120 BPM
            let bass = if phase < 0.05 { 0.95 } else { 0.15 };
            let f = t.update(now, &frame(bass, 0.3), 0.3, 0.6);
            if f.beat {
                beats += 1;
            }
        }
        assert!((18..=21).contains(&beats), "beats = {beats}");
        let bpm = t.bpm();
        assert!((bpm - 120.0).abs() < 6.0, "bpm = {bpm}");
    }

    #[test]
    fn silence_is_a_break_and_has_no_beats() {
        let mut t = BeatTracker::new();
        for i in 0..120 {
            let f = t.update(i as f64 / 60.0, &frame(0.8, 0.5), 0.3, 0.5);
            assert!(!f.quiet || i < 2);
        }
        let mut quiet = false;
        for i in 120..240 {
            let f = t.update(i as f64 / 60.0, &frame(0.0, 0.0), 0.0, 0.0);
            assert!(!f.beat);
            quiet |= f.quiet;
        }
        assert!(quiet);
    }
}
