//! Decoded, in-memory audio clips.

use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// A clip decoded to interleaved stereo `f32` at its native sample rate.
/// Resampling to the output device happens at play time (the engine plays
/// clips at a rate of `clip_rate / device_rate * pitch`), so device changes
/// never require re-decoding.
#[derive(Debug, Clone)]
pub struct Sound {
    pub rate: u32,
    pub frames: Vec<[f32; 2]>,
}

impl Sound {
    pub fn duration_secs(&self) -> f32 {
        self.frames.len() as f32 / self.rate as f32
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Copy out a segment, given in milliseconds.
    pub fn slice_ms(&self, start_ms: u32, dur_ms: u32) -> Sound {
        let ms = |v: u32| (v as u64 * self.rate as u64 / 1000) as usize;
        let start = ms(start_ms).min(self.frames.len());
        let end = (start + ms(dur_ms)).min(self.frames.len());
        Sound { rate: self.rate, frames: self.frames[start..end].to_vec() }
    }

    /// Remove leading silence (below `threshold`, linear amplitude), keeping a
    /// ~1 ms pre-roll. Leading silence is the #1 cause of "laggy" key sounds.
    pub fn trim_leading_silence(&mut self, threshold: f32) {
        let first = self
            .frames
            .iter()
            .position(|f| f[0].abs() > threshold || f[1].abs() > threshold);
        match first {
            Some(i) => {
                let preroll = (self.rate / 1000) as usize;
                let cut = i.saturating_sub(preroll);
                if cut > 0 {
                    self.frames.drain(..cut);
                }
            }
            None => self.frames.clear(),
        }
    }

    /// Remove trailing silence (keeps a ~5 ms tail).
    pub fn trim_trailing_silence(&mut self, threshold: f32) {
        if let Some(last) = self
            .frames
            .iter()
            .rposition(|f| f[0].abs() > threshold || f[1].abs() > threshold)
        {
            let tail = (self.rate / 200) as usize;
            self.frames.truncate((last + 1 + tail).min(self.frames.len()));
        } else {
            self.frames.clear();
        }
    }

    pub fn peak(&self) -> f32 {
        self.frames.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()))
    }

    /// Scale so the loudest sample reaches `target` (linear).
    pub fn normalize(&mut self, target: f32) {
        let p = self.peak();
        if p > 1e-6 {
            let g = target / p;
            for f in &mut self.frames {
                f[0] *= g;
                f[1] *= g;
            }
        }
    }

    pub fn from_mono(rate: u32, samples: &[f32]) -> Sound {
        Sound { rate, frames: samples.iter().map(|&s| [s, s]).collect() }
    }

    /// Write as 16-bit PCM WAV.
    pub fn write_wav(&self, path: &Path) -> Result<()> {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: self.rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec)
            .with_context(|| format!("creating {}", path.display()))?;
        for f in &self.frames {
            for s in f {
                w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
            }
        }
        w.finalize()?;
        Ok(())
    }
}

/// Decode any supported file (WAV, MP3, OGG/Vorbis, FLAC, AAC/M4A).
/// `max_secs` guards against accidentally loading a whole song as a key click.
pub fn decode_file(path: &Path, max_secs: f32) -> Result<Sound> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .with_context(|| format!("unrecognized audio format: {}", path.display()))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("no audio track in {}", path.display()))?;
    let track_id = track.id;
    let mut rate = track.codec_params.sample_rate.unwrap_or(48_000);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("unsupported codec")?;

    let mut frames: Vec<[f32; 2]> = Vec::new();
    let mut max_frames = (max_secs * rate as f32) as usize;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = *buf.spec();
                if spec.rate != rate {
                    rate = spec.rate;
                    max_frames = (max_secs * rate as f32) as usize;
                }
                let ch = spec.channels.count().max(1);
                let mut sbuf = SampleBuffer::<f32>::new(buf.capacity() as u64, spec);
                sbuf.copy_interleaved_ref(buf);
                for f in sbuf.samples().chunks_exact(ch) {
                    frames.push(if ch == 1 { [f[0], f[0]] } else { [f[0], f[1]] });
                }
                if frames.len() > max_frames {
                    return Err(anyhow!(
                        "{} is longer than {max_secs:.0} s — key sounds should be short clips",
                        path.display()
                    ));
                }
            }
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        }
    }
    if frames.is_empty() {
        return Err(anyhow!("{} contains no audio", path.display()));
    }
    Ok(Sound { rate, frames })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Sound {
        Sound { rate: 1000, frames: (0..n).map(|i| [i as f32 / n as f32; 2]).collect() }
    }

    #[test]
    fn slice() {
        let s = ramp(1000);
        let seg = s.slice_ms(100, 50);
        assert_eq!(seg.frames.len(), 50);
        assert!((seg.frames[0][0] - 0.1).abs() < 1e-6);
        // Past the end clamps instead of panicking.
        assert_eq!(s.slice_ms(990, 500).frames.len(), 10);
        assert_eq!(s.slice_ms(5000, 10).frames.len(), 0);
    }

    #[test]
    fn trims_and_normalizes() {
        let mut s = Sound { rate: 1000, frames: vec![[0.0; 2]; 100] };
        s.frames.extend(std::iter::repeat_n([0.5, -0.25], 10));
        s.frames.extend(vec![[0.0; 2]; 100]);
        s.trim_leading_silence(0.01);
        assert_eq!(s.frames.len(), 1 + 10 + 100); // 1 ms pre-roll at 1 kHz
        s.trim_trailing_silence(0.01);
        assert_eq!(s.frames.len(), 1 + 10 + 5);
        s.normalize(1.0);
        assert!((s.peak() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn wav_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.wav");
        let s = Sound::from_mono(44_100, &(0..4410).map(|i| ((i as f32) * 0.05).sin() * 0.5).collect::<Vec<_>>());
        s.write_wav(&p).unwrap();
        let d = decode_file(&p, 5.0).unwrap();
        assert_eq!(d.rate, 44_100);
        assert_eq!(d.frames.len(), 4410);
        assert!((d.frames[100][0] - s.frames[100][0]).abs() < 1e-3);
    }

    #[test]
    fn rejects_long_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("long.wav");
        Sound::from_mono(8000, &vec![0.1; 8000 * 3]).write_wav(&p).unwrap();
        assert!(decode_file(&p, 2.0).is_err());
        assert!(decode_file(&p, 5.0).is_ok());
    }
}
