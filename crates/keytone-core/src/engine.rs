//! Low-latency playback engine.
//!
//! * The real-time audio callback never locks, never allocates (in steady
//!   state) and never frees: new voices arrive through a lock-free queue and
//!   finished clips are handed back through another queue to be dropped on
//!   the engine's control thread.
//! * Clips play at `clip_rate / device_rate * pitch` with linear
//!   interpolation, so resampling and pitch variation are the same operation.
//! * A soft limiter keeps 10-key mashing from clipping.
//! * The control thread owns the `cpal::Stream` (which is not `Send` on every
//!   platform), rebuilds it on device errors, and follows the system default
//!   output device when the user hasn't picked one.

use crate::mapper::PlayCmd;
use crate::sound::Sound;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use crossbeam_channel::{bounded, Receiver, Sender};
use crossbeam_queue::ArrayQueue;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const MAX_VOICES: usize = 48;
const FADE_IN_FRAMES: u32 = 32;

#[derive(Debug, Clone, Serialize, Default)]
pub struct EngineInfo {
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_frames: Option<u32>,
    pub latency_ms: Option<f32>,
    pub error: Option<String>,
    /// Non-fatal note, e.g. the requested buffer size was not usable.
    pub note: Option<String>,
}

struct Shared {
    incoming: ArrayQueue<PlayCmd>,
    garbage: ArrayQueue<Arc<Sound>>,
    volume: AtomicU32,
    stream_failed: AtomicBool,
    /// Incremented by every audio callback; lets the control thread detect a
    /// stalled stream (some ALSA bridges stop calling back with small fixed
    /// buffers).
    callbacks: AtomicU64,
}

enum Ctl {
    Configure { device: Option<String>, buffer_frames: Option<u32>, reply: Sender<EngineInfo> },
    Shutdown,
}

pub struct Engine {
    shared: Arc<Shared>,
    ctl: Sender<Ctl>,
    info: Arc<Mutex<EngineInfo>>,
}

impl Engine {
    /// Start the engine. Never fails: if no output device can be opened the
    /// engine runs silently and `info().error` explains why.
    pub fn start(device: Option<String>, buffer_frames: Option<u32>) -> Engine {
        let shared = Arc::new(Shared {
            incoming: ArrayQueue::new(256),
            garbage: ArrayQueue::new(1024),
            volume: AtomicU32::new(0.8f32.to_bits()),
            stream_failed: AtomicBool::new(false),
            callbacks: AtomicU64::new(0),
        });
        let info = Arc::new(Mutex::new(EngineInfo::default()));
        let (tx, rx) = bounded::<Ctl>(8);
        {
            let shared = shared.clone();
            let info = info.clone();
            std::thread::Builder::new()
                .name("keytone-audio-ctl".into())
                .spawn(move || control_thread(rx, shared, info))
                .expect("spawn audio control thread");
        }
        let engine = Engine { shared, ctl: tx, info };
        engine.configure(device, buffer_frames);
        engine
    }

    /// Switch output device and/or buffer size. `None` device = system default.
    pub fn configure(&self, device: Option<String>, buffer_frames: Option<u32>) -> EngineInfo {
        let (rtx, rrx) = bounded(1);
        if self.ctl.send(Ctl::Configure { device, buffer_frames, reply: rtx }).is_err() {
            return self.info();
        }
        rrx.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| self.info())
    }

    pub fn info(&self) -> EngineInfo {
        self.info.lock().clone()
    }

    pub fn play(&self, cmd: PlayCmd) {
        // If the callback is starved and the queue is full, dropping a click
        // is better than blocking the key hook.
        let _ = self.shared.incoming.push(cmd);
    }

    pub fn play_sound(&self, sound: Arc<Sound>) {
        self.play(PlayCmd { sound, gain: 1.0, pitch: 1.0 });
    }

    pub fn set_volume(&self, v: f32) {
        self.shared.volume.store(v.clamp(0.0, 1.5).to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.shared.volume.load(Ordering::Relaxed))
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.ctl.send(Ctl::Shutdown);
    }
}

pub fn list_output_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.output_devices() {
        Ok(devs) => devs.filter_map(|d| d.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

fn default_device_name() -> Option<String> {
    cpal::default_host().default_output_device().and_then(|d| d.name().ok())
}

fn control_thread(rx: Receiver<Ctl>, shared: Arc<Shared>, info: Arc<Mutex<EngineInfo>>) {
    let mut stream: Option<cpal::Stream> = None;
    let mut want_device: Option<String> = None;
    let mut want_buffer: Option<u32> = None;
    // Set when the device stalled with a fixed buffer; cleared on reconfigure.
    let mut force_default_buffer = false;
    let mut last_default_check = Instant::now();
    let mut current_default = default_device_name();
    let mut watch = (0u64, Instant::now());

    let rebuild = |stream: &mut Option<cpal::Stream>, dev: &Option<String>, buf: Option<u32>, note: Option<String>| {
        *stream = None; // stop the old stream first
        shared.stream_failed.store(false, Ordering::Relaxed);
        match build_stream(dev.as_deref(), buf, shared.clone()) {
            Ok((s, mut i)) => {
                log::info!("audio: {} @ {} Hz, {} ch, buffer {:?}", i.device, i.sample_rate, i.channels, i.buffer_frames);
                *stream = Some(s);
                if i.note.is_none() {
                    i.note = note;
                }
                *info.lock() = i;
            }
            Err(e) => {
                log::warn!("audio: cannot open output: {e:#}");
                *info.lock() = EngineInfo { error: Some(format!("{e:#}")), ..Default::default() };
            }
        }
        (shared.callbacks.load(Ordering::Relaxed), Instant::now())
    };
    let effective = |want: Option<u32>, force: bool| if force { None } else { want };

    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(Ctl::Configure { device, buffer_frames, reply }) => {
                want_device = device;
                want_buffer = buffer_frames;
                force_default_buffer = false;
                watch = rebuild(&mut stream, &want_device, want_buffer, None);
                let _ = reply.send(info.lock().clone());
            }
            Ok(Ctl::Shutdown) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }

        // Free finished clips off the real-time thread.
        while shared.garbage.pop().is_some() {}

        if shared.stream_failed.load(Ordering::Relaxed) {
            log::warn!("audio: stream error, reopening");
            std::thread::sleep(Duration::from_millis(500));
            watch = rebuild(&mut stream, &want_device, effective(want_buffer, force_default_buffer), None);
            continue;
        }

        // Stall watchdog: an output stream should call back continuously.
        if stream.is_some() {
            let n = shared.callbacks.load(Ordering::Relaxed);
            if n != watch.0 {
                watch = (n, Instant::now());
            } else if watch.1.elapsed() > Duration::from_millis(700) {
                let fixed = info.lock().buffer_frames;
                if let Some(frames) = fixed {
                    log::warn!("audio: stream stalled with a fixed {frames}-frame buffer; using the driver default");
                    force_default_buffer = true;
                    watch = rebuild(
                        &mut stream,
                        &want_device,
                        None,
                        Some(format!("This device stalled with a {frames}-frame buffer, so the driver default is used")),
                    );
                } else {
                    log::warn!("audio: stream stalled; reopening");
                    std::thread::sleep(Duration::from_millis(500));
                    watch = rebuild(&mut stream, &want_device, None, None);
                }
                continue;
            }
        }

        // Follow the OS default output (e.g. headphones plugged in).
        if want_device.is_none() && last_default_check.elapsed() > Duration::from_secs(2) {
            last_default_check = Instant::now();
            let now = default_device_name();
            if now != current_default {
                current_default = now;
                log::info!("audio: default output changed to {current_default:?}");
                watch = rebuild(&mut stream, &want_device, effective(want_buffer, force_default_buffer), None);
            }
        }
    }
}

fn build_stream(
    device_name: Option<&str>,
    buffer_frames: Option<u32>,
    shared: Arc<Shared>,
) -> anyhow::Result<(cpal::Stream, EngineInfo)> {
    let host = cpal::default_host();
    let device = match device_name {
        Some(name) => host
            .output_devices()?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .or_else(|| host.default_output_device()),
        None => host.default_output_device(),
    }
    .ok_or_else(|| anyhow::anyhow!("no audio output device found"))?;
    let name = device.name().unwrap_or_else(|_| "Unknown device".into());
    let supported = device.default_output_config()?;
    let sample_format = supported.sample_format();
    let mut config: cpal::StreamConfig = supported.config();

    let mut chosen = None;
    if let Some(n) = buffer_frames {
        if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size() {
            let n = n.clamp(*min, *max);
            config.buffer_size = cpal::BufferSize::Fixed(n);
            chosen = Some(n);
        }
    }

    let open = |config: &cpal::StreamConfig| -> anyhow::Result<cpal::Stream> {
        use cpal::SampleFormat as F;
        let s = shared.clone();
        Ok(match sample_format {
            F::F32 => make::<f32>(&device, config, s)?,
            F::F64 => make::<f64>(&device, config, s)?,
            F::I16 => make::<i16>(&device, config, s)?,
            F::I32 => make::<i32>(&device, config, s)?,
            F::I8 => make::<i8>(&device, config, s)?,
            F::U16 => make::<u16>(&device, config, s)?,
            F::U8 => make::<u8>(&device, config, s)?,
            F::U32 => make::<u32>(&device, config, s)?,
            other => anyhow::bail!("unsupported sample format {other:?}"),
        })
    };

    let stream = match open(&config) {
        Ok(s) => s,
        // Some drivers reject fixed buffer sizes; fall back to the default.
        Err(e) if chosen.is_some() => {
            log::warn!("audio: fixed buffer rejected ({e:#}), using driver default");
            config.buffer_size = cpal::BufferSize::Default;
            chosen = None;
            open(&config)?
        }
        Err(e) => return Err(e),
    };
    stream.play()?;

    let info = EngineInfo {
        device: name,
        sample_rate: config.sample_rate.0,
        channels: config.channels,
        buffer_frames: chosen,
        latency_ms: chosen.map(|n| n as f32 * 1000.0 / config.sample_rate.0 as f32),
        error: None,
        note: None,
    };
    Ok((stream, info))
}

fn make<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut mixer = Mixer::new(config.sample_rate.0);
    let mut scratch: Vec<f32> = vec![0.0; 8192 * channels.max(2)];
    let err_shared = shared.clone();
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            shared.callbacks.fetch_add(1, Ordering::Relaxed);
            while let Some(cmd) = shared.incoming.pop() {
                if let Some(stolen) = mixer.add(cmd) {
                    let _ = shared.garbage.push(stolen);
                }
            }
            if scratch.len() < data.len() {
                scratch.resize(data.len(), 0.0); // only if the driver hands us a huge buffer
            }
            let buf = &mut scratch[..data.len()];
            let master = f32::from_bits(shared.volume.load(Ordering::Relaxed));
            mixer.render(buf, channels, master, &shared.garbage);
            for (o, s) in data.iter_mut().zip(buf.iter()) {
                *o = T::from_sample(*s);
            }
        },
        move |err| {
            log::error!("audio stream error: {err}");
            err_shared.stream_failed.store(true, Ordering::Relaxed);
        },
        None,
    )
}

struct Voice {
    sound: Arc<Sound>,
    pos: f64,
    step: f64,
    gain: f32,
    age: u64,
    fade: u32,
}

/// The pure mixing core (no I/O), unit-testable.
pub struct Mixer {
    out_rate: u32,
    voices: Vec<Option<Voice>>,
    counter: u64,
}

impl Mixer {
    pub fn new(out_rate: u32) -> Mixer {
        Mixer { out_rate, voices: (0..MAX_VOICES).map(|_| None).collect(), counter: 0 }
    }

    pub fn active(&self) -> usize {
        self.voices.iter().filter(|v| v.is_some()).count()
    }

    /// Start a voice. If all slots are busy the oldest voice is stolen and its
    /// clip returned so the caller can free it off the audio thread.
    pub fn add(&mut self, cmd: PlayCmd) -> Option<Arc<Sound>> {
        self.counter += 1;
        let v = Voice {
            step: cmd.sound.rate as f64 / self.out_rate as f64 * cmd.pitch.max(0.01) as f64,
            sound: cmd.sound,
            pos: 0.0,
            gain: cmd.gain,
            age: self.counter,
            fade: 0,
        };
        if let Some(slot) = self.voices.iter_mut().find(|s| s.is_none()) {
            *slot = Some(v);
            return None;
        }
        let oldest = self
            .voices
            .iter_mut()
            .min_by_key(|s| s.as_ref().map(|v| v.age).unwrap_or(0))
            .expect("MAX_VOICES > 0");
        oldest.replace(v).map(|old| old.sound)
    }

    /// Mix all voices into `out` (interleaved, `channels` per frame).
    pub fn render(&mut self, out: &mut [f32], channels: usize, master: f32, garbage: &ArrayQueue<Arc<Sound>>) {
        let channels = channels.max(1);
        out.iter_mut().for_each(|s| *s = 0.0);
        let frames = out.len() / channels;

        for slot in self.voices.iter_mut() {
            let Some(v) = slot else { continue };
            let src = &v.sound.frames;
            let mut finished = false;
            for f in 0..frames {
                let i = v.pos as usize;
                if i + 1 >= src.len() {
                    finished = true;
                    break;
                }
                let t = (v.pos - i as f64) as f32;
                let a = src[i];
                let b = src[i + 1];
                let mut g = v.gain;
                if v.fade < FADE_IN_FRAMES {
                    g *= v.fade as f32 / FADE_IN_FRAMES as f32;
                    v.fade += 1;
                }
                let l = (a[0] + (b[0] - a[0]) * t) * g;
                let r = (a[1] + (b[1] - a[1]) * t) * g;
                let base = f * channels;
                if channels == 1 {
                    out[base] += 0.5 * (l + r);
                } else {
                    out[base] += l;
                    out[base + 1] += r;
                }
                v.pos += v.step;
            }
            if finished {
                if let Some(v) = slot.take() {
                    if let Err(s) = garbage.push(v.sound) {
                        drop(s); // queue full: unavoidable free on the audio thread
                    }
                }
            }
        }

        for s in out.iter_mut() {
            *s = soft_clip(*s * master);
        }
    }
}

/// Transparent below 0.9, smoothly saturates towards 1.0 above.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    const K: f32 = 0.9;
    let a = x.abs();
    if a <= K {
        x
    } else {
        let y = K + (1.0 - K) * ((a - K) / (1.0 - K)).tanh();
        y.copysign(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(rate: u32, n: usize, v: f32) -> Arc<Sound> {
        Arc::new(Sound { rate, frames: vec![[v, v]; n] })
    }

    fn cmd(s: Arc<Sound>) -> PlayCmd {
        PlayCmd { sound: s, gain: 1.0, pitch: 1.0 }
    }

    #[test]
    fn plays_and_finishes() {
        let g = ArrayQueue::new(16);
        let mut m = Mixer::new(48_000);
        m.add(cmd(clip(48_000, 100, 0.5)));
        let mut out = vec![0.0; 256 * 2];
        m.render(&mut out, 2, 1.0, &g);
        // After the fade-in the clip plays at full level.
        assert!((out[2 * 50] - 0.5).abs() < 1e-4);
        assert_eq!(out[2 * 200], 0.0);
        assert_eq!(m.active(), 0);
        assert_eq!(g.len(), 1, "finished clip handed back for freeing");
    }

    #[test]
    fn resamples_by_rate() {
        // A 24 kHz clip of 100 frames lasts ~200 frames at 48 kHz.
        let g = ArrayQueue::new(16);
        let mut m = Mixer::new(48_000);
        m.add(cmd(clip(24_000, 100, 0.5)));
        let mut out = vec![0.0; 400];
        m.render(&mut out, 1, 1.0, &g);
        let nonzero = out.iter().filter(|s| **s != 0.0).count();
        assert!((195..=199).contains(&nonzero), "{nonzero}");
    }

    #[test]
    fn steals_oldest_when_full() {
        let mut m = Mixer::new(48_000);
        for _ in 0..MAX_VOICES {
            assert!(m.add(cmd(clip(48_000, 10_000, 0.01))).is_none());
        }
        let stolen = m.add(cmd(clip(48_000, 10_000, 0.01)));
        assert!(stolen.is_some());
        assert_eq!(m.active(), MAX_VOICES);
    }

    #[test]
    fn limiter_never_exceeds_unity() {
        let g = ArrayQueue::new(64);
        let mut m = Mixer::new(48_000);
        for _ in 0..20 {
            m.add(cmd(clip(48_000, 1000, 0.9)));
        }
        let mut out = vec![0.0; 1024];
        m.render(&mut out, 2, 1.0, &g);
        assert!(out.iter().all(|s| s.abs() <= 1.0));
        assert_eq!(soft_clip(0.5), 0.5);
        assert!(soft_clip(10.0) <= 1.0 && soft_clip(-10.0) >= -1.0);
    }

    #[test]
    fn mono_and_multichannel_layouts() {
        let g = ArrayQueue::new(16);
        let mut m = Mixer::new(48_000);
        m.add(cmd(Arc::new(Sound { rate: 48_000, frames: vec![[0.4, 0.2]; 200] })));
        let mut out = vec![0.0; 100 * 6];
        m.render(&mut out, 6, 1.0, &g);
        let f = 60 * 6;
        assert!((out[f] - 0.4).abs() < 1e-4 && (out[f + 1] - 0.2).abs() < 1e-4);
        assert_eq!(&out[f + 2..f + 6], &[0.0; 4]);
    }

    #[test]
    fn engine_without_device_does_not_panic() {
        let e = Engine::start(Some("definitely-not-a-device".into()), Some(256));
        e.set_volume(0.5);
        assert!((e.volume() - 0.5).abs() < 1e-6);
        e.play_sound(clip(48_000, 10, 0.1));
        let _ = e.info();
    }
}
