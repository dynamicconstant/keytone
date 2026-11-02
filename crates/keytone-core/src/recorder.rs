//! Microphone recorder for making custom key sounds.

use crate::sound::Sound;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub const MAX_RECORD_SECS: u32 = 10;

struct Shared {
    samples: Mutex<Vec<f32>>,
    level: AtomicU32,
}

/// An active recording. The cpal stream is kept on its own thread because it
/// is not `Send` on all platforms; dropping the handle stops recording.
pub struct Recording {
    shared: Arc<Shared>,
    rate: u32,
    stop: crossbeam_channel::Sender<()>,
}

impl Recording {
    pub fn start() -> Result<Recording> {
        let shared = Arc::new(Shared { samples: Mutex::new(Vec::new()), level: AtomicU32::new(0) });
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(1);
        let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<u32>>(1);
        let sh = shared.clone();
        std::thread::Builder::new().name("keytone-recorder".into()).spawn(move || {
            match open_input(sh) {
                Ok((stream, rate)) => {
                    let _ = ready_tx.send(Ok(rate));
                    let _ = stop_rx.recv_timeout(std::time::Duration::from_secs(MAX_RECORD_SECS as u64 + 2));
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        })?;
        let rate = ready_rx.recv().map_err(|_| anyhow!("recorder thread died"))??;
        Ok(Recording { shared, rate, stop: stop_tx })
    }

    /// Current input peak (0..1), for a level meter. Resets on read.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level.swap(0, Ordering::Relaxed))
    }

    pub fn elapsed_secs(&self) -> f32 {
        self.shared.samples.lock().len() as f32 / self.rate as f32
    }

    /// Stop and return the recorded clip, trimmed and normalized.
    pub fn finish(self) -> Result<Sound> {
        let _ = self.stop.send(());
        let samples = std::mem::take(&mut *self.shared.samples.lock());
        let mut s = Sound::from_mono(self.rate, &samples);
        let peak = s.peak();
        // Gate relative to the take's own loudness, so quiet mics still work.
        let threshold = (peak * 0.05).max(0.002);
        s.trim_leading_silence(threshold);
        s.trim_trailing_silence(threshold);
        if s.is_empty() || peak < 0.005 {
            return Err(anyhow!("nothing was recorded — check your microphone"));
        }
        s.normalize(0.9);
        Ok(s)
    }
}

fn open_input(shared: Arc<Shared>) -> Result<(cpal::Stream, u32)> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| anyhow!("no microphone found"))?;
    let supported = device.default_input_config()?;
    let fmt = supported.sample_format();
    let config: cpal::StreamConfig = supported.config();
    let rate = config.sample_rate.0;
    use cpal::SampleFormat as F;
    let stream = match fmt {
        F::F32 => build::<f32>(&device, &config, shared)?,
        F::I16 => build::<i16>(&device, &config, shared)?,
        F::I32 => build::<i32>(&device, &config, shared)?,
        F::U16 => build::<u16>(&device, &config, shared)?,
        F::U8 => build::<u8>(&device, &config, shared)?,
        F::I8 => build::<i8>(&device, &config, shared)?,
        F::F64 => build::<f64>(&device, &config, shared)?,
        other => return Err(anyhow!("unsupported mic format {other:?}")),
    };
    stream.play()?;
    Ok((stream, rate))
}

fn build<T>(device: &cpal::Device, config: &cpal::StreamConfig, shared: Arc<Shared>) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let ch = config.channels as usize;
    let max = (MAX_RECORD_SECS * config.sample_rate.0) as usize;
    Ok(device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut buf = shared.samples.lock();
            let mut peak = f32::from_bits(shared.level.load(Ordering::Relaxed));
            for frame in data.chunks_exact(ch.max(1)) {
                if buf.len() >= max {
                    break;
                }
                let s: f32 = frame.iter().map(|x| x.to_sample::<f32>()).sum::<f32>() / ch as f32;
                peak = peak.max(s.abs());
                buf.push(s);
            }
            shared.level.store(peak.to_bits(), Ordering::Relaxed);
        },
        |e| log::error!("mic stream error: {e}"),
        None,
    )?)
}
