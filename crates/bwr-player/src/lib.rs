//! Streaming internet-radio playback.
//!
//! Architecture: one worker thread per `Play` command downloads the stream,
//! decodes it with symphonia, resamples to the output device's rate and feeds
//! a shared sample queue. The cpal output callback drains that queue (this
//! keeps the audio callback trivially small and never blocking).
//!
//! The same decoded PCM is pushed into the shared [`SpectrumFeed`] so the UI
//! can draw the 32-band visualiser.

use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use bwr_spectrum::SpectrumFeed;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Debug)]
pub enum PlayerEvent {
    Connecting,
    Playing,
    Stopped,
    Failed(String),
}

pub enum Command {
    Play(String),
    Stop,
    SetVolume(f32),
}

pub struct Player {
    cmd_tx: mpsc::Sender<Command>,
    event_rx: Arc<Mutex<mpsc::Receiver<PlayerEvent>>>,
    volume: Arc<AtomicU32>,
    /// Shared spectrum analyser fed by the decode thread.
    pub spectrum: Arc<SpectrumFeed>,
    generation: Arc<AtomicU32>,
}

const QUEUE_CAP: usize = 48000 * 2 * 3; // ~3 s of stereo output

impl Player {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let (event_tx, event_rx) = mpsc::channel::<PlayerEvent>();
        let volume = Arc::new(AtomicU32::new(0.8f32.to_bits()));
        let spectrum = Arc::new(SpectrumFeed::new());
        let generation = Arc::new(AtomicU32::new(0));

        {
            let volume = volume.clone();
            let spectrum = spectrum.clone();
            let generation = generation.clone();
            std::thread::spawn(move || {
                worker_loop(cmd_rx, event_tx, volume, spectrum, generation);
            });
        }

        Self {
            cmd_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            volume,
            spectrum,
            generation,
        }
    }

    pub fn play(&self, url: &str) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let _ = self.cmd_tx.send(Command::Play(url.to_string()));
    }

    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let _ = self.cmd_tx.send(Command::Stop);
    }

    pub fn set_volume(&self, v: f32) {
        self.volume
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        let _ = self.cmd_tx.send(Command::SetVolume(v.clamp(0.0, 1.0)));
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    /// Drain pending events (call once per UI frame).
    pub fn poll(&self) -> Vec<PlayerEvent> {
        let rx = self.event_rx.lock().unwrap();
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            out.push(ev);
        }
        out
    }
}

fn worker_loop(
    cmd_rx: mpsc::Receiver<Command>,
    event_tx: mpsc::Sender<PlayerEvent>,
    volume: Arc<AtomicU32>,
    spectrum: Arc<SpectrumFeed>,
    generation: Arc<AtomicU32>,
) {
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            Command::Play(url) => {
                let gen = generation.load(Ordering::SeqCst);
                play_once(&url, &event_tx, &volume, &spectrum, &cmd_rx, gen, &generation);
                spectrum.reset();
            }
            Command::Stop => {
                let _ = event_tx.send(PlayerEvent::Stopped);
            }
            Command::SetVolume(_) => {}
        }
    }
}

fn play_once(
    url: &str,
    event_tx: &mpsc::Sender<PlayerEvent>,
    volume: &Arc<AtomicU32>,
    spectrum: &Arc<SpectrumFeed>,
    cmd_rx: &mpsc::Receiver<Command>,
    gen: u32,
    generation: &Arc<AtomicU32>,
) {
    let _ = event_tx.send(PlayerEvent::Connecting);

    // --- Output device ------------------------------------------------------
    let (out_stream, out_config, _out_tx, queue) = match open_output(volume.clone()) {
        Ok(v) => v,
        Err(e) => {
            let _ = event_tx.send(PlayerEvent::Failed(format!("audio output: {e}")));
            return;
        }
    };

    // --- Input stream -------------------------------------------------------
    let response = match reqwest::blocking::Client::builder()
        .dns_resolver(Arc::new(bwr_core::Ipv4OnlyResolver))
        .timeout(Duration::from_secs(20))
        .build()
        .and_then(|c| c.get(url).send())
    {
        Ok(r) => r,
        Err(e) => {
            let _ = event_tx.send(PlayerEvent::Failed(format!("connect: {e}")));
            drop(out_stream);
            return;
        }
    };
    if !response.status().is_success() {
        let _ = event_tx.send(PlayerEvent::Failed(format!("HTTP {}", response.status())));
        drop(out_stream);
        return;
    }

    let source = HttpStream::new(response);
    let mss = symphonia::core::io::MediaSourceStream::new(Box::new(source), Default::default());
    let probed = match symphonia::default::get_probe().format(
        &Default::default(),
        mss,
        &Default::default(),
        &Default::default(),
    ) {
        Ok(p) => p,
        Err(e) => {
            let _ = event_tx.send(PlayerEvent::Failed(format!("probe: {e}")));
            drop(out_stream);
            return;
        }
    };

    let mut format = probed.format;
    let track = match format.default_track() {
        Some(t) => t,
        None => {
            let _ = event_tx.send(PlayerEvent::Failed("no audio track".into()));
            drop(out_stream);
            return;
        }
    };
    let mut decoder = match symphonia::default::get_codecs().make(&track.codec_params, &Default::default()) {
        Ok(d) => d,
        Err(e) => {
            let _ = event_tx.send(PlayerEvent::Failed(format!("decoder: {e}")));
            drop(out_stream);
            return;
        }
    };

    let out_rate = out_config.sample_rate().0;
    let out_channels = out_config.channels() as usize;

    let mut sent_playing = false;

    'decode: loop {
        // Obey stop/switch commands while decoding.
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Command::Stop => {
                    let _ = event_tx.send(PlayerEvent::Stopped);
                    drop(out_stream);
                    return;
                }
                Command::Play(_) => {
                    drop(out_stream);
                    return; // caller loop starts the new URL
                }
                Command::SetVolume(_) => {}
            }
        }
        if generation.load(Ordering::SeqCst) != gen {
            drop(out_stream);
            return;
        }

        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(_))
            | Err(symphonia::core::errors::Error::ResetRequired) => {
                // Live streams drop connections; retry forever while still current.
                std::thread::sleep(Duration::from_secs(2));
                continue 'decode;
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue 'decode,
            Err(e) => {
                let _ = event_tx.send(PlayerEvent::Failed(format!("stream: {e}")));
                break 'decode;
            }
        };

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue 'decode,
        };

        // Resample + channel-map into the output queue.
        if let Some((spec, interleaved)) = to_interleaved_f32(&decoded) {
            if !sent_playing {
                let _ = event_tx.send(PlayerEvent::Playing);
                sent_playing = true;
            }
            spectrum.push_interleaved(&interleaved, spec.channels.count());
            let mapped = map_channels(&interleaved, spec.channels.count(), out_channels);
            let resampled = resample_linear(&mapped, spec.rate, out_rate, out_channels);

            // Backpressure: don't decode more than QUEUE_CAP ahead.
            loop {
                {
                    let mut q = queue.lock().unwrap();
                    if q.len() + resampled.len() <= QUEUE_CAP {
                        q.extend(resampled.iter().copied());
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
                if generation.load(Ordering::SeqCst) != gen {
                    drop(out_stream);
                    return;
                }
            }
        }
    }

    drop(out_stream);
}

/// Open the default cpal output device and start a stream draining `queue`.
fn open_output(
    volume: Arc<AtomicU32>,
) -> std::result::Result<
    (cpal::Stream, cpal::SupportedStreamConfig, mpsc::Sender<()>, Arc<Mutex<VecDeque<f32>>>),
    String,
> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_string())?;
    let config = device
        .default_output_config()
        .map_err(|e| e.to_string())?;

    let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::with_capacity(QUEUE_CAP)));
    let cb_queue = queue.clone();

    let err_tx_dummy = (); // (structure kept simple; errors surface as stream silence)

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                &config.config(),
                move |out: &mut [f32], _| {
                    let vol = f32::from_bits(volume.load(Ordering::Relaxed));
                    let mut q = cb_queue.lock().unwrap();
                    for slot in out.iter_mut() {
                        *slot = q.pop_front().unwrap_or(0.0) * vol;
                    }
                },
                move |_e| {
                    let _ = err_tx_dummy;
                },
                None,
            )
            .map_err(|e| e.to_string())?,
        cpal::SampleFormat::I16 => device
            .build_output_stream(
                &config.config(),
                move |out: &mut [i16], _| {
                    let vol = f32::from_bits(volume.load(Ordering::Relaxed));
                    let mut q = cb_queue.lock().unwrap();
                    for slot in out.iter_mut() {
                        let s = (q.pop_front().unwrap_or(0.0) * vol).clamp(-1.0, 1.0);
                        *slot = (s * i16::MAX as f32) as i16;
                    }
                },
                move |_e| {
                    let _ = err_tx_dummy;
                },
                None,
            )
            .map_err(|e| e.to_string())?,
        other => return Err(format!("unsupported sample format {other:?}")),
    };
    stream.play().map_err(|e| e.to_string())?;
    let tx = mpsc::channel().0;
    Ok((stream, config, tx, queue))
}

fn to_interleaved_f32(
    buf: &symphonia::core::audio::AudioBufferRef,
) -> Option<(symphonia::core::audio::SignalSpec, Vec<f32>)> {
    use symphonia::core::audio::{AudioBufferRef, SampleBuffer};

    let spec = *buf.spec();
    let mut sample_buf = SampleBuffer::<f32>::new(buf.capacity() as u64, spec);
    match buf {
        AudioBufferRef::F32(b) => sample_buf.copy_interleaved_typed(b),
        AudioBufferRef::U8(b) => sample_buf.copy_interleaved_typed(b),
        AudioBufferRef::U16(b) => sample_buf.copy_interleaved_typed(b),
        AudioBufferRef::U24(b) => sample_buf.copy_interleaved_typed(b),
        AudioBufferRef::U32(b) => sample_buf.copy_interleaved_typed(b),
        AudioBufferRef::S8(b) => sample_buf.copy_interleaved_typed(b),
        AudioBufferRef::S16(b) => sample_buf.copy_interleaved_typed(b),
        AudioBufferRef::S24(b) => sample_buf.copy_interleaved_typed(b),
        AudioBufferRef::S32(b) => sample_buf.copy_interleaved_typed(b),
        AudioBufferRef::F64(b) => sample_buf.copy_interleaved_typed(b),
    }
    Some((spec, sample_buf.samples().to_vec()))
}

/// Map N source channels onto M output channels (mono→stereo dup, etc.).
fn map_channels(samples: &[f32], src_ch: usize, dst_ch: usize) -> Vec<f32> {
    if src_ch == dst_ch {
        return samples.to_vec();
    }
    let frames = samples.len() / src_ch;
    let mut out = Vec::with_capacity(frames * dst_ch);
    for f in 0..frames {
        for c in 0..dst_ch {
            let s = if src_ch == 1 {
                samples[f]
            } else {
                samples[f * src_ch + c.min(src_ch - 1)]
            };
            out.push(s);
        }
    }
    out
}

/// Simple linear-interpolation resampler (plenty for radio streams).
fn resample_linear(samples: &[f32], src_rate: u32, dst_rate: u32, channels: usize) -> Vec<f32> {
    if src_rate == dst_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let frames = samples.len() / channels;
    let out_frames = (frames as u64 * dst_rate as u64 / src_rate as u64) as usize;
    let mut out = Vec::with_capacity(out_frames * channels);
    let step = src_rate as f64 / dst_rate as f64;
    let mut pos = 0.0f64;
    for _ in 0..out_frames {
        let i = pos as usize;
        let frac = (pos - i as f64) as f32;
        let i2 = (i + 1).min(frames - 1);
        for c in 0..channels {
            let a = samples[i * channels + c];
            let b = samples[i2 * channels + c];
            out.push(a + (b - a) * frac);
        }
        pos += step;
        if pos as usize >= frames - 1 {
            break;
        }
    }
    out
}

/// Read-only HTTP stream with forward-only Seek, good enough for live radio.
struct HttpStream {
    response: reqwest::blocking::Response,
    pos: u64,
}

impl HttpStream {
    fn new(response: reqwest::blocking::Response) -> Self {
        Self { response, pos: 0 }
    }
}

impl Read for HttpStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.response.read(buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for HttpStream {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            SeekFrom::Current(0) => Ok(self.pos),
            SeekFrom::Current(n) if n > 0 => {
                let mut remaining = n as u64;
                let mut chunk = [0u8; 8192];
                while remaining > 0 {
                    let take = remaining.min(chunk.len() as u64) as usize;
                    let read = self.read(&mut chunk[..take])?;
                    if read == 0 {
                        break;
                    }
                    remaining -= read as u64;
                }
                Ok(self.pos)
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "backward seek not supported on live streams",
            )),
        }
    }
}

impl symphonia::core::io::MediaSource for HttpStream {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}
