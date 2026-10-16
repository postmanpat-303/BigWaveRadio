//! Streaming internet-radio playback.
//!
//! Architecture: one worker thread per `Play` command downloads the stream
//! into a shared buffer (filled by a separate downloader thread so network
//! stalls never block the decoder), decodes it with symphonia, resamples to
//! the output device's rate and feeds a shared sample queue. The cpal output
//! callback drains that queue (this keeps the audio callback trivially small
//! and never blocking).
//!
//! The same decoded PCM is pushed into the shared [`SpectrumFeed`] so the UI
//! can draw the 32-band visualiser.
//!
//! Switching stations bumps a shared generation counter; blocked reads return
//! EOF as soon as they notice the generation change, so the decoder always
//! unwinds promptly and never plays events from a stale stream.

use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

use bwr_spectrum::SpectrumFeed;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Debug)]
pub enum PlayerEvent {
    Connecting,
    Playing,
    Stopped,
    Failed(String),
    /// ICY StreamTitle metadata ("Artist - Song"), when the stream provides it.
    Meta(String),
}

pub enum Command {
    Play(String),
    Stop,
    SetVolume(f32),
}

type Event = (u32, PlayerEvent);

/// Open output device + its sample queue, reused across station switches.
type OutParts = (cpal::Stream, cpal::SupportedStreamConfig, Arc<Mutex<VecDeque<f32>>>);

pub struct Player {
    cmd_tx: mpsc::Sender<Command>,
    event_rx: Arc<Mutex<mpsc::Receiver<Event>>>,
    volume: Arc<AtomicU32>,
    /// Shared spectrum analyser fed by the decode thread.
    pub spectrum: Arc<SpectrumFeed>,
    generation: Arc<AtomicU32>,
}

const QUEUE_CAP: usize = 48000 * 2 * 3; // ~3 s of stereo output
const DL_BUF_CAP: usize = 2 * 1024 * 1024; // downloader buffer cap
const MAX_IO_RETRIES: u32 = 5; // tolerate ~10 s of stream stalls, then give up

impl Player {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let (event_tx, event_rx) = mpsc::channel::<Event>();
        let volume = Arc::new(AtomicU32::new(0.8f32.to_bits()));
        let spectrum = Arc::new(SpectrumFeed::new());
        let generation = Arc::new(AtomicU32::new(0));
        let meta_slot = Arc::new(Mutex::new(None));

        {
            let volume = volume.clone();
            let spectrum = spectrum.clone();
            let generation = generation.clone();
            let meta_slot = meta_slot.clone();
            std::thread::spawn(move || {
                worker_loop(cmd_rx, event_tx, volume, spectrum, generation, meta_slot);
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

    /// Current playback generation; events from `poll` carry the generation
    /// they belong to — ignore any that don't match this value.
    pub fn generation(&self) -> u32 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Drain pending events (call once per UI frame).
    pub fn poll(&self) -> Vec<Event> {
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
    event_tx: mpsc::Sender<Event>,
    volume: Arc<AtomicU32>,
    spectrum: Arc<SpectrumFeed>,
    generation: Arc<AtomicU32>,
    meta_slot: Arc<Mutex<Option<String>>>,
) {
    // The output device stream is opened lazily on the first Play and reused
    // for every later switch (opening a new WASAPI stream per click makes the
    // audio engine churn and can wedge the device).
    let mut out: Option<OutParts> = None;

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            Command::Play(mut url) => {
                // Coalesce rapid station switches: only the newest Play and
                // any trailing Stop matter.
                while let Ok(next) = cmd_rx.try_recv() {
                    match next {
                        Command::Play(u) => url = u,
                        Command::Stop => {
                            url.clear();
                            break;
                        }
                        Command::SetVolume(_) => {}
                    }
                }
                if url.is_empty() {
                    let _ = event_tx.send((generation.load(Ordering::SeqCst), PlayerEvent::Stopped));
                    continue;
                }
                let gen = generation.load(Ordering::SeqCst);
                play_once(&url, &event_tx, &volume, &spectrum, gen, &generation, &mut out, meta_slot.clone());
                spectrum.reset();
            }
            Command::Stop => {
                // Drop any decoded audio still buffered in the output queue,
                // otherwise the tail (~3 s) keeps playing after Stop.
                if let Some((_, _, queue)) = &out {
                    queue.lock().unwrap().clear();
                }
                let _ = event_tx.send((generation.load(Ordering::SeqCst), PlayerEvent::Stopped));
            }
            Command::SetVolume(_) => {}
        }
    }
}

fn play_once(
    url: &str,
    event_tx: &mpsc::Sender<Event>,
    volume: &Arc<AtomicU32>,
    spectrum: &Arc<SpectrumFeed>,
    gen: u32,
    generation: &Arc<AtomicU32>,
    out: &mut Option<OutParts>,
    meta_slot: Arc<Mutex<Option<String>>>,
) {
    let send = |ev: PlayerEvent| {
        let _ = event_tx.send((gen, ev));
    };
    send(PlayerEvent::Connecting);

    // --- Output device (open once, reuse after) ------------------------------
    if out.is_none() {
        match open_output(volume.clone()) {
            Ok(v) => *out = Some(v),
            Err(e) => {
                send(PlayerEvent::Failed(format!("audio output: {e}")));
                return;
            }
        }
    }
    let (_stream, out_config, queue) = out.as_ref().unwrap();
    // Drop any audio left over from the previous station.
    queue.lock().unwrap().clear();

    // --- Input stream ---------------------------------------------------------
    let source = ChannelSource::start(url, generation.clone(), gen, meta_slot.clone());
    let mss = symphonia::core::io::MediaSourceStream::new(Box::new(source), Default::default());
    let probed = match symphonia::default::get_probe().format(
        &Default::default(),
        mss,
        &Default::default(),
        &Default::default(),
    ) {
        Ok(p) => p,
        Err(e) => {
            send(PlayerEvent::Failed(format!("probe: {e}")));
            return;
        }
    };

    let mut format = probed.format;
    let track = match format.default_track() {
        Some(t) => t,
        None => {
            send(PlayerEvent::Failed("no audio track".into()));
            return;
        }
    };
    let mut decoder = match symphonia::default::get_codecs().make(&track.codec_params, &Default::default()) {
        Ok(d) => d,
        Err(e) => {
            send(PlayerEvent::Failed(format!("decoder: {e}")));
            return;
        }
    };

    let out_rate = out_config.sample_rate().0;
    let out_channels = out_config.channels() as usize;

    let mut sent_playing = false;
    let mut io_retries = 0u32;

    loop {
        if generation.load(Ordering::SeqCst) != gen {
            return; // switched or stopped: unwind promptly
        }

        // Forward any ICY title reported by the downloader.
        if let Some(title) = meta_slot.lock().unwrap().take() {
            send(PlayerEvent::Meta(title));
        }

        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(_))
            | Err(symphonia::core::errors::Error::ResetRequired) => {
                // Live streams drop connections; retry briefly while current.
                if generation.load(Ordering::SeqCst) != gen {
                    return;
                }
                io_retries += 1;
                if io_retries > MAX_IO_RETRIES {
                    send(PlayerEvent::Failed("stream ended (connection lost)".into()));
                    return;
                }
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => {
                send(PlayerEvent::Failed(format!("stream: {e}")));
                return;
            }
        };
        io_retries = 0;

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Resample + channel-map into the output queue.
        if let Some((spec, interleaved)) = to_interleaved_f32(&decoded) {
            if !sent_playing {
                send(PlayerEvent::Playing);
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
                        // Keep the analyser in step with what is audible now.
                        let queued_frames = q.len() / out_channels.max(1);
                        spectrum.set_delay_samples(
                            (queued_frames as u64 * spec.rate as u64 / out_rate.max(1) as u64) as usize,
                        );
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
                if generation.load(Ordering::SeqCst) != gen {
                    return;
                }
            }
        }
    }
}

/// Open the default cpal output device and start a stream draining `queue`.
fn open_output(
    volume: Arc<AtomicU32>,
) -> std::result::Result<OutParts, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_string())?;
    let config = device
        .default_output_config()
        .map_err(|e| e.to_string())?;

    let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::with_capacity(QUEUE_CAP)));
    let cb_queue = queue.clone();

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
                move |_e| {},
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
                move |_e| {},
                None,
            )
            .map_err(|e| e.to_string())?,
        other => return Err(format!("unsupported sample format {other:?}")),
    };
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, config, queue))
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

// ----------------------------------------------------------------------------
// Buffered HTTP source: a downloader thread fills a shared buffer; the decoder
// reads from it with a short timeout so it always notices station switches.
// ----------------------------------------------------------------------------

struct SharedBuf {
    buf: VecDeque<u8>,
    done: bool,
}

pub struct ChannelSource {
    shared: Arc<(Mutex<SharedBuf>, Condvar)>,
    generation: Arc<AtomicU32>,
    gen: u32,
    pos: u64,
}

/// Byte-state machine that splits an ICY stream into audio bytes and
/// metadata blocks. Streams with no `icy-metaint` header pass through
/// untouched (everything is audio).
struct IcyParser {
    metaint: usize,
    until_meta: usize,
    state: MetaState,
    meta_buf: Vec<u8>,
}

enum MetaState {
    Audio,
    MetaBody(usize),
}

impl IcyParser {
    fn new(metaint: Option<usize>) -> Self {
        let metaint = metaint.unwrap_or(0);
        Self {
            metaint,
            until_meta: metaint,
            state: MetaState::Audio,
            meta_buf: Vec::new(),
        }
    }

    /// Feed one downloaded chunk: audio bytes accumulate in `audio`, complete
    /// StreamTitle values are appended to `titles`.
    fn feed(&mut self, chunk: &[u8], audio: &mut Vec<u8>, titles: &mut Vec<String>) {
        if self.metaint == 0 {
            audio.extend_from_slice(chunk);
            return;
        }
        for &b in chunk {
            match self.state {
                MetaState::Audio => {
                    if self.until_meta > 0 {
                        self.until_meta -= 1;
                        audio.push(b);
                    } else {
                        // This byte is the metadata-block length (in 16-byte units).
                        let n = b as usize * 16;
                        if n == 0 {
                            self.until_meta = self.metaint;
                        } else {
                            self.meta_buf.clear();
                            self.state = MetaState::MetaBody(n);
                        }
                    }
                }
                MetaState::MetaBody(remaining) => {
                    self.meta_buf.push(b);
                    if remaining == 1 {
                        if let Some(title) = parse_stream_title(&self.meta_buf) {
                            titles.push(title);
                        }
                        self.until_meta = self.metaint;
                        self.state = MetaState::Audio;
                    } else {
                        self.state = MetaState::MetaBody(remaining - 1);
                    }
                }
            }
        }
    }
}

/// Extract `StreamTitle='...'` from one ICY metadata block.
fn parse_stream_title(block: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(block);
    let key = "StreamTitle='";
    let start = text.find(key)? + key.len();
    let rest = &text[start..];
    let end = rest.find('\'')?;
    let title = rest[..end].trim().to_string();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

impl ChannelSource {
    fn start(
        url: &str,
        generation: Arc<AtomicU32>,
        gen: u32,
        meta_slot: Arc<Mutex<Option<String>>>,
    ) -> Self {
        let shared: Arc<(Mutex<SharedBuf>, Condvar)> =
            Arc::new((Mutex::new(SharedBuf { buf: VecDeque::new(), done: false }), Condvar::new()));
        let s2 = shared.clone();
        let url = url.to_string();
        let gen_watch = generation.clone();
        std::thread::spawn(move || {
            let mark_done = |s: &Arc<(Mutex<SharedBuf>, Condvar)>| {
                let (lock, cv) = &**s;
                lock.lock().unwrap().done = true;
                cv.notify_all();
            };
            let client = match reqwest::blocking::Client::builder()
                .dns_resolver(Arc::new(bwr_core::Ipv4OnlyResolver))
                .timeout(Duration::from_secs(20))
                .build()
            {
                Ok(c) => c,
                Err(_) => {
                    mark_done(&s2);
                    return;
                }
            };
            match client.get(&url).send() {
                Ok(mut resp) if resp.status().is_success() => {
                    let metaint = resp
                        .headers()
                        .get("icy-metaint")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .filter(|&n| n > 0);
                    let mut parser = IcyParser::new(metaint);
                    let mut chunk = [0u8; 16384];
                    loop {
                        // Bail out promptly when the user switches station.
                        if gen_watch.load(Ordering::SeqCst) != gen {
                            mark_done(&s2);
                            return;
                        }
                        match resp.read(&mut chunk) {
                            Ok(0) => break,
                            Ok(n) => {
                                let mut audio = Vec::with_capacity(n);
                                let mut titles = Vec::new();
                                parser.feed(&chunk[..n], &mut audio, &mut titles);
                                if !titles.is_empty() {
                                    let mut slot = meta_slot.lock().unwrap();
                                    for t in titles {
                                        *slot = Some(t);
                                    }
                                }
                                let (lock, cv) = &*s2;
                                let mut st = lock.lock().unwrap();
                                while st.buf.len() + audio.len() >= DL_BUF_CAP && !st.done {
                                    let (g, _) = cv
                                        .wait_timeout(st, Duration::from_millis(200))
                                        .unwrap();
                                    st = g;
                                    if gen_watch.load(Ordering::SeqCst) != gen {
                                        // Still holding the lock: set done in place.
                                        st.done = true;
                                        cv.notify_all();
                                        return;
                                    }
                                }
                                st.buf.extend(&audio);
                                cv.notify_all();
                            }
                            Err(_) => break,
                        }
                    }
                }
                _ => {}
            }
            mark_done(&s2);
        });
        Self { shared, generation, gen, pos: 0 }
    }
}

impl Read for ChannelSource {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let (lock, cv) = &*self.shared;
        let mut st = lock.lock().unwrap();
        loop {
            if self.generation.load(Ordering::SeqCst) != self.gen {
                return Ok(0); // EOF: decoder unwinds and notices the switch
            }
            if !st.buf.is_empty() {
                let n = out.len().min(st.buf.len());
                for slot in out[..n].iter_mut() {
                    *slot = st.buf.pop_front().unwrap();
                }
                self.pos += n as u64;
                return Ok(n);
            }
            if st.done {
                return Ok(0);
            }
            let (g, _) = cv.wait_timeout(st, Duration::from_millis(100)).unwrap();
            st = g;
        }
    }
}

impl Seek for ChannelSource {
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

impl symphonia::core::io::MediaSource for ChannelSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(parser: &mut IcyParser, data: &[u8], titles: &mut Vec<String>) -> Vec<u8> {
        let mut audio = Vec::new();
        parser.feed(data, &mut audio, titles);
        audio
    }

    #[test]
    fn parser_disabled_passes_everything_through() {
        let mut p = IcyParser::new(None);
        let mut titles = Vec::new();
        let audio = feed_all(&mut p, &[1, 2, 3, 200, 7], &mut titles);
        assert_eq!(audio, vec![1, 2, 3, 200, 7]);
        assert!(titles.is_empty());
    }

    #[test]
    fn parser_splits_audio_and_metadata_blocks() {
        let mut p = IcyParser::new(Some(4));
        let mut titles = Vec::new();
        // 4 audio bytes, length byte 2 (=32 meta bytes: 21-byte payload
        // padded with zeros), zero-length block, then 4 more audio bytes.
        let mut stream = vec![10, 11, 12, 13, 2];
        let mut block = b"StreamTitle='Song A';".to_vec();
        block.resize(32, 0);
        stream.extend_from_slice(&block);
        // 4 audio bytes, zero-length block, 4 more audio bytes.
        stream.extend_from_slice(&[30, 31, 32, 33, 0, 40, 41, 42, 43]);
        let audio = feed_all(&mut p, &stream, &mut titles);
        assert_eq!(audio, vec![10, 11, 12, 13, 30, 31, 32, 33, 40, 41, 42, 43]);
        assert_eq!(titles, vec!["Song A".to_string()]);
    }

    #[test]
    fn parser_handles_zero_length_blocks_and_chunk_boundaries() {
        let mut p = IcyParser::new(Some(3));
        let mut titles = Vec::new();
        // First block has zero-length metadata; feed in awkward chunks.
        let stream: Vec<u8> = vec![
            1, 2, 3, 0, // zero-length meta block
            4, 5, 6, 3, // 48-byte meta block follows
        ];
        let audio = feed_all(&mut p, &stream, &mut titles);
        assert_eq!(audio, vec![1, 2, 3, 4, 5, 6]);
        let mut meta48 = b"StreamTitle='A - B';StreamUrl='';".to_vec();
        meta48.resize(48, 0);
        let audio2 = feed_all(&mut p, &meta48[..10], &mut titles);
        assert!(audio2.is_empty());
        assert!(titles.is_empty()); // incomplete
        let audio3 = feed_all(&mut p, &meta48[10..], &mut titles);
        assert!(audio3.is_empty());
        assert_eq!(titles, vec!["A - B".to_string()]);
    }

    #[test]
    fn stream_title_parser() {
        assert_eq!(
            parse_stream_title(b"StreamTitle='Artist - Track';StreamUrl='';"),
            Some("Artist - Track".to_string())
        );
        assert_eq!(parse_stream_title(b"StreamTitle='';"), None);
        assert_eq!(parse_stream_title(b"garbage"), None);
    }
}
