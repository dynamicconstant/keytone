//! Turns raw key events into "play this clip, at this gain and pitch".
//!
//! Responsibilities:
//! * suppress OS auto-repeat (a held key plays once)
//! * only play a key-up sound if we saw (and handled) the matching key-down
//! * pick a clip variant, avoiding the same variant twice in a row
//! * add subtle per-press pitch/gain variation so typing sounds natural

use crate::pack::LoadedPack;
use crate::sound::Sound;
use keyhook::{Dir, KeyEvent};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PlayCmd {
    pub sound: Arc<Sound>,
    /// Linear gain (pack volume × random variation). Master volume is applied
    /// by the engine.
    pub gain: f32,
    /// Pitch factor (1.0 = original).
    pub pitch: f32,
}

#[derive(Default)]
pub struct Mapper {
    held: HashSet<&'static str>,
    last_variant: HashMap<(&'static str, bool), usize>,
    rng: Option<fastrand::Rng>,
}

impl Mapper {
    pub fn new() -> Self {
        Mapper { rng: Some(fastrand::Rng::new()), ..Default::default() }
    }

    /// Deterministic RNG for tests.
    pub fn with_seed(seed: u64) -> Self {
        Mapper { rng: Some(fastrand::Rng::with_seed(seed)), ..Default::default() }
    }

    /// Forget held keys (e.g. after the hook restarts or the app is disabled).
    pub fn reset(&mut self) {
        self.held.clear();
    }

    pub fn handle(&mut self, ev: KeyEvent, pack: &LoadedPack) -> Option<PlayCmd> {
        match ev.dir {
            Dir::Down => {
                if !self.held.insert(ev.code) {
                    return None; // auto-repeat
                }
            }
            Dir::Up => {
                if !self.held.remove(ev.code) {
                    return None; // never saw the press
                }
            }
        }

        let candidates = pack.resolve(ev.code, ev.dir);
        if candidates.is_empty() {
            return None;
        }
        let rng = self.rng.get_or_insert_with(fastrand::Rng::new);

        let key = (ev.code, ev.dir == Dir::Down);
        let i = if candidates.len() == 1 {
            0
        } else {
            let last = self.last_variant.get(&key).copied();
            let mut i = rng.usize(..candidates.len());
            if Some(i) == last {
                i = (i + 1 + rng.usize(..candidates.len() - 1)) % candidates.len();
            }
            i
        };
        self.last_variant.insert(key, i);

        let r = &pack.meta.random;
        let cents = (rng.f32() * 2.0 - 1.0) * r.pitch_cents.max(0.0);
        let db = (rng.f32() * 2.0 - 1.0) * r.gain_db.max(0.0);
        Some(PlayCmd {
            sound: candidates[i].clone(),
            gain: pack.meta.volume.max(0.0) * 10f32.powf(db / 20.0),
            pitch: 2f32.powf(cents / 1200.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{PackFile, Slot};

    fn pack() -> (tempfile::TempDir, LoadedPack) {
        let d = tempfile::tempdir().unwrap();
        for (i, n) in ["a.wav", "b.wav", "c.wav", "up.wav"].iter().enumerate() {
            Sound::from_mono(8000, &vec![0.1 * (i + 1) as f32; 400]).write_wav(&d.path().join(n)).unwrap();
        }
        let mut pf = PackFile::new("t", "T");
        pf.defaults = Slot {
            down: Some(vec!["a.wav".into(), "b.wav".into(), "c.wav".into()]),
            up: Some(vec!["up.wav".into()]),
        };
        let p = LoadedPack::from_meta(pf, d.path()).unwrap();
        (d, p)
    }

    fn ev(code: &'static str, dir: Dir) -> KeyEvent {
        KeyEvent { code, dir }
    }

    #[test]
    fn suppresses_autorepeat() {
        let (_d, p) = pack();
        let mut m = Mapper::with_seed(1);
        assert!(m.handle(ev("KeyA", Dir::Down), &p).is_some());
        for _ in 0..20 {
            assert!(m.handle(ev("KeyA", Dir::Down), &p).is_none());
        }
        assert!(m.handle(ev("KeyA", Dir::Up), &p).is_some());
        assert!(m.handle(ev("KeyA", Dir::Down), &p).is_some());
    }

    #[test]
    fn orphan_key_up_is_silent() {
        let (_d, p) = pack();
        let mut m = Mapper::with_seed(1);
        assert!(m.handle(ev("KeyB", Dir::Up), &p).is_none());
    }

    #[test]
    fn variants_never_repeat_back_to_back_and_all_get_used() {
        let (_d, p) = pack();
        let mut m = Mapper::with_seed(7);
        let mut last = None;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let c = m.handle(ev("KeyA", Dir::Down), &p).unwrap();
            m.handle(ev("KeyA", Dir::Up), &p);
            let id = Arc::as_ptr(&c.sound) as usize;
            assert_ne!(Some(id), last);
            last = Some(id);
            seen.insert(id);
        }
        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn randomization_stays_in_bounds() {
        let (_d, p) = pack();
        let mut m = Mapper::with_seed(3);
        let max_pitch = 2f32.powf(p.meta.random.pitch_cents / 1200.0);
        let max_gain = 10f32.powf(p.meta.random.gain_db / 20.0);
        for _ in 0..500 {
            let c = m.handle(ev("KeyQ", Dir::Down), &p).unwrap();
            m.handle(ev("KeyQ", Dir::Up), &p);
            assert!(c.pitch <= max_pitch + 1e-4 && c.pitch >= 1.0 / max_pitch - 1e-4);
            assert!(c.gain <= max_gain + 1e-4 && c.gain >= 1.0 / max_gain - 1e-4);
        }
    }
}
