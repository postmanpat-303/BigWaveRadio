//! 32-band spectrum analysis, fed by the player's decoded PCM.
//!
//! The player pushes interleaved output samples into a shared ring; the UI
//! (or any thread) calls `snapshot()` to get the current 32 band levels
//! (0.0..=1.0), computed with per-band auto-gain so the visualiser stays
//! lively at any listening volume.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

mod beat;
pub use beat::{AudioFeatures, BeatTracker};
use std::sync::Mutex;

use rustfft::num_complex::Complex32;
use rustfft::Fft;

pub const BANDS: usize = 32;
const FFT_SIZE: usize = 2048;
/// Ring capacity in mono f32 samples (~21 s at 48 kHz — generous headroom).
const RING_CAP: usize = 1 << 21;

/// Sample rate the analyser assumes (output devices are resampled to this by
/// the player before analysis for a stable visualiser).
pub const ANALYSIS_RATE: u32 = 48_000;

struct Ring {
    buf: Vec<f32>,
    write: usize,
    read: usize,
}

pub struct SpectrumFeed {
    ring: Mutex<Ring>,
    /// Latest smoothed band levels, fixed-point 1_000_000 = 1.0.
    levels: [AtomicU32; BANDS],
    fft: std::sync::Arc<dyn Fft<f32>>,
    scratch: Mutex<Vec<Complex32>>,
    band_max: Mutex<[f32; BANDS]>,
    /// How many mono samples the analysis should lag behind the newest pushed
    /// sample. The player decodes ahead of the speakers (it keeps an output
    /// queue of up to ~3 s), so without this the visuals run early.
    delay: AtomicUsize,
}

impl Default for SpectrumFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumFeed {
    pub fn new() -> Self {
        Self {
            ring: Mutex::new(Ring {
                buf: vec![0.0; RING_CAP],
                write: 0,
                read: 0,
            }),
            levels: std::array::from_fn(|_| AtomicU32::new(0)),
            fft: rustfft::FftPlanner::new().plan_fft_forward(FFT_SIZE),
            scratch: Mutex::new(vec![Complex32::default(); FFT_SIZE]),
            band_max: Mutex::new([0.0001; BANDS]),
            delay: AtomicUsize::new(0),
        }
    }

    /// Push interleaved samples from the output stream (any sample rate).
    /// `channels` is the channel count; samples are mixed down to mono.
    pub fn push_interleaved(&self, samples: &[f32], channels: usize) {
        if channels == 0 || samples.len() < channels {
            return;
        }
        let mut ring = self.ring.lock().unwrap();
        for frame in samples.chunks_exact(channels) {
            let mono: f32 = frame.iter().sum::<f32>() / channels as f32;
            let w = ring.write;
            ring.buf[w] = mono;
            ring.write = (w + 1) & (RING_CAP - 1);
            if ring.write == ring.read {
                // Overrun: drop oldest.
                ring.read = (ring.read + 1) & (RING_CAP - 1);
            }
        }
    }

    /// Tell the analyser how far (in mono samples at the pushed rate) the
    /// audible output lags behind the newest pushed sample.
    pub fn set_delay_samples(&self, samples: usize) {
        self.delay.store(samples.min(RING_CAP / 2), Ordering::Relaxed);
    }

    /// Read the newest audible `out.len()` mono samples without consuming them.
    fn newest_window(&self, out: &mut [f32]) -> bool {
        let n = out.len();
        let ring = self.ring.lock().unwrap();
        let available = (ring.write + RING_CAP - ring.read) & (RING_CAP - 1);
        if available < n {
            return false;
        }
        let delay = self.delay.load(Ordering::Relaxed).min(available - n);
        let start = (ring.write + RING_CAP - delay - n) & (RING_CAP - 1);
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = ring.buf[(start + i) & (RING_CAP - 1)];
        }
        true
    }

    /// Recompute band levels from the freshest PCM. Call at ~30 fps from the UI.
    pub fn update(&self) {
        let mut window = [0.0f32; FFT_SIZE];
        if !self.newest_window(&mut window) {
            self.decay();
            return;
        }

        let mut scratch = self.scratch.lock().unwrap();
        for (i, s) in window.iter().enumerate() {
            let hann = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE as f32 - 1.0)).cos());
            scratch[i] = Complex32::new(s * hann, 0.0);
        }
        self.fft.process(&mut scratch);

        // Log-spaced bands from 40 Hz to 12 kHz.
        let bin_hz = ANALYSIS_RATE as f64 / FFT_SIZE as f64;
        let f_min = 40.0f64;
        let f_max = 12_000.0f64;
        let ratio = (f_max / f_min).powf(1.0 / BANDS as f64);

        let mut band_max = self.band_max.lock().unwrap();
        for b in 0..BANDS {
            let lo = ((f_min * ratio.powf(b as f64)) / bin_hz).max(1.0) as usize;
            let hi = ((f_min * ratio.powf(b as f64 + 1.0)) / bin_hz).max(lo as f64) as usize;
            let mut sum = 0.0f64;
            let mut n = 0usize;
            for k in lo..=hi.min(FFT_SIZE / 2 - 1) {
                let c = scratch[k];
                sum += (c.re * c.re + c.im * c.im) as f64;
                n += 1;
            }
            if n == 0 {
                continue;
            }
            let avg = (sum / n as f64).sqrt() as f32;
            // Per-band auto-gain with slow decay keeps it lively at any volume.
            band_max[b] = band_max[b].mul_add(0.999, 0.0).max(avg.max(0.0001));
            let level = (avg / band_max[b]).clamp(0.0, 1.0).powf(0.7);
            // Fast attack, slower release.
            let prev = self.levels[b].load(Ordering::Relaxed) as f32 / 1_000_000.0;
            let smoothed = if level > prev { level } else { prev * 0.82 };
            self.levels[b].store((smoothed * 1_000_000.0) as u32, Ordering::Relaxed);
        }
    }

    fn decay(&self) {
        for b in &self.levels {
            let prev = b.load(Ordering::Relaxed) as f32 / 1_000_000.0;
            let next = prev * 0.82;
            b.store((next * 1_000_000.0) as u32, Ordering::Relaxed);
        }
    }

    /// RMS level and peak (both 0.0..=1.0) of the most recent audible ~21 ms.
    pub fn level_peak(&self) -> (f32, f32) {
        let mut window = [0.0f32; 1024];
        if !self.newest_window(&mut window) {
            return (0.0, 0.0);
        }
        let mut sum = 0.0f32;
        let mut peak = 0.0f32;
        for s in window {
            sum += s * s;
            peak = peak.max(s.abs());
        }
        ((sum / window.len() as f32).sqrt().clamp(0.0, 1.0), peak.clamp(0.0, 1.0))
    }

    /// Copy the current 32 band levels (0.0..=1.0).
    pub fn snapshot(&self, out: &mut [f32; BANDS]) {
        for (i, b) in out.iter_mut().enumerate() {
            *b = self.levels[i].load(Ordering::Relaxed) as f32 / 1_000_000.0;
        }
    }

    /// Reset ring + levels (called when playback stops).
    pub fn reset(&self) {
        let mut ring = self.ring.lock().unwrap();
        ring.write = 0;
        ring.read = 0;
        self.delay.store(0, Ordering::Relaxed);
        for b in &self.levels {
            b.store(0, Ordering::Relaxed);
        }
    }
}
