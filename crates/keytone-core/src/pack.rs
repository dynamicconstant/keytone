//! Sound-pack format (`pack.json`), loading/decoding, and resolution.
//!
//! A pack is a folder:
//! ```text
//! my-pack/
//!   pack.json
//!   key1.wav  key2.wav  space.wav  enter.wav ...
//! ```
//! Sound references inside `pack.json` are either a file name in the pack
//! folder (`"space.wav"`) or `"#segment"`, a named slice of the pack's
//! optional sprite file (used by Mechvibes-style single-file packs).
//!
//! Resolution for a key event: `keys[code]` → `groups[group]` → `defaults`.
//! Each level may set `down` and/or `up`; an unset field inherits, an empty
//! list means "explicitly silent".

use crate::sound::{decode_file, Sound};
use anyhow::{anyhow, bail, Context, Result};
use keyhook::{group_of, Dir, KeyGroup};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const PACK_FILE: &str = "pack.json";
pub const SCHEMA: u32 = 1;
/// Longest single key sound we accept.
pub const MAX_CLIP_SECS: f32 = 10.0;
/// Longest sprite file we accept.
pub const MAX_SPRITE_SECS: f32 = 300.0;
const SILENCE: f32 = 0.003; // ~ -50 dBFS

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Slot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<Vec<String>>,
}

impl Slot {
    pub fn get(&self, dir: Dir) -> &Option<Vec<String>> {
        match dir {
            Dir::Down => &self.down,
            Dir::Up => &self.up,
        }
    }
    pub fn get_mut(&mut self, dir: Dir) -> &mut Option<Vec<String>> {
        match dir {
            Dir::Down => &mut self.down,
            Dir::Up => &mut self.up,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.down.is_none() && self.up.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RandomCfg {
    /// Max pitch deviation per press, in cents (100 = one semitone).
    #[serde(default = "d_pitch")]
    pub pitch_cents: f32,
    /// Max gain deviation per press, in dB.
    #[serde(default = "d_gain")]
    pub gain_db: f32,
}
fn d_pitch() -> f32 {
    30.0
}
fn d_gain() -> f32 {
    1.5
}
impl Default for RandomCfg {
    fn default() -> Self {
        RandomCfg { pitch_cents: d_pitch(), gain_db: d_gain() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sprite {
    pub file: String,
    /// name -> [start_ms, duration_ms]
    pub segments: BTreeMap<String, [u32; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackFile {
    #[serde(default = "d_schema")]
    pub schema: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// Overall pack gain (linear), applied on top of the master volume.
    #[serde(default = "d_one")]
    pub volume: f32,
    /// Strip leading silence from clips at load time (lower perceived latency).
    #[serde(default = "d_true")]
    pub trim_silence: bool,
    #[serde(default)]
    pub random: RandomCfg,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprite: Option<Sprite>,
    #[serde(default)]
    pub defaults: Slot,
    #[serde(default)]
    pub groups: BTreeMap<String, Slot>,
    #[serde(default)]
    pub keys: BTreeMap<String, Slot>,
}
fn d_schema() -> u32 {
    SCHEMA
}
fn d_one() -> f32 {
    1.0
}
fn d_true() -> bool {
    true
}

impl PackFile {
    pub fn new(id: &str, name: &str) -> Self {
        PackFile {
            schema: SCHEMA,
            id: id.into(),
            name: name.into(),
            author: String::new(),
            description: String::new(),
            cover: None,
            volume: 1.0,
            trim_silence: true,
            random: RandomCfg::default(),
            sprite: None,
            defaults: Slot::default(),
            groups: BTreeMap::new(),
            keys: BTreeMap::new(),
        }
    }

    pub fn read(dir: &Path) -> Result<PackFile> {
        let p = dir.join(PACK_FILE);
        let text = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        let pf: PackFile =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", p.display()))?;
        if pf.schema > SCHEMA {
            bail!("pack {} uses a newer format (schema {}); please update Keytone", pf.name, pf.schema);
        }
        Ok(pf)
    }

    pub fn write(&self, dir: &Path) -> Result<()> {
        let p = dir.join(PACK_FILE);
        let tmp = dir.join(".pack.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    /// Every sound reference used anywhere in the pack.
    pub fn all_refs(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.defaults)
            .chain(self.groups.values())
            .chain(self.keys.values())
            .flat_map(|s| s.down.iter().chain(s.up.iter()))
            .flatten()
    }

    /// Get/insert the slot for a target.
    pub fn slot_mut(&mut self, target: &Target) -> &mut Slot {
        match target {
            Target::Default => &mut self.defaults,
            Target::Group(g) => self.groups.entry(g.clone()).or_default(),
            Target::Key(k) => self.keys.entry(k.clone()).or_default(),
        }
    }

    /// Drop empty slots so pack.json stays tidy.
    pub fn prune(&mut self) {
        self.groups.retain(|_, s| !s.is_empty());
        self.keys.retain(|_, s| !s.is_empty());
    }
}

/// What a mapping edit applies to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "lowercase")]
pub enum Target {
    Default,
    Group(String),
    Key(String),
}

/// Reject anything that could escape the pack folder.
pub fn safe_file_name(name: &str) -> Result<&str> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.starts_with('.')
        || Path::new(name).is_absolute()
    {
        bail!("invalid file name in pack: {name:?}");
    }
    Ok(name)
}

/// A pack with all clips decoded and the lookup tables built.
#[derive(Debug)]
pub struct LoadedPack {
    pub meta: PackFile,
    pub dir: PathBuf,
    defaults: SlotClips,
    groups: HashMap<KeyGroup, SlotClips>,
    keys: HashMap<String, SlotClips>,
    /// Decoded clip per reference, for previews in the UI.
    clips: HashMap<String, Arc<Sound>>,
    /// Non-fatal problems (missing files etc.) for the UI to show.
    pub warnings: Vec<String>,
}

type SlotClips = [Option<Vec<Arc<Sound>>>; 2];

struct Loader<'a> {
    meta: &'a PackFile,
    dir: &'a Path,
    sprite: Option<Sound>,
    clips: HashMap<String, Arc<Sound>>,
    warnings: Vec<String>,
}

impl Loader<'_> {
    fn slot(&mut self, slot: &Slot) -> SlotClips {
        let mut out: SlotClips = [None, None];
        for d in [Dir::Down, Dir::Up] {
            if let Some(refs) = slot.get(d) {
                out[idx(d)] = Some(refs.iter().filter_map(|r| self.clip(r)).collect());
            }
        }
        out
    }

    fn clip(&mut self, r: &str) -> Option<Arc<Sound>> {
        if let Some(c) = self.clips.get(r) {
            return Some(c.clone());
        }
        let res: Result<Sound> = if let Some(seg) = r.strip_prefix('#') {
            match (&self.sprite, self.meta.sprite.as_ref().and_then(|s| s.segments.get(seg))) {
                (Some(sp), Some([start, dur])) => Ok(sp.slice_ms(*start, *dur)),
                _ => Err(anyhow!("sprite segment not found")),
            }
        } else {
            safe_file_name(r).and_then(|n| decode_file(&self.dir.join(n), MAX_CLIP_SECS))
        };
        match res {
            Ok(mut s) => {
                if self.meta.trim_silence {
                    s.trim_leading_silence(SILENCE);
                }
                if s.is_empty() {
                    self.warnings.push(format!("{r} is silent"));
                    return None;
                }
                let a = Arc::new(s);
                self.clips.insert(r.to_string(), a.clone());
                Some(a)
            }
            Err(e) => {
                self.warnings.push(format!("{r}: {e:#}"));
                None
            }
        }
    }
}

fn idx(dir: Dir) -> usize {
    match dir {
        Dir::Down => 0,
        Dir::Up => 1,
    }
}

impl LoadedPack {
    pub fn load(dir: &Path) -> Result<LoadedPack> {
        let meta = PackFile::read(dir)?;
        Self::from_meta(meta, dir)
    }

    pub fn from_meta(meta: PackFile, dir: &Path) -> Result<LoadedPack> {
        let sprite = match &meta.sprite {
            Some(sp) => {
                let path = dir.join(safe_file_name(&sp.file)?);
                Some(decode_file(&path, MAX_SPRITE_SECS)?)
            }
            None => None,
        };
        let mut ld = Loader { meta: &meta, dir, sprite, clips: HashMap::new(), warnings: Vec::new() };

        let defaults = ld.slot(&meta.defaults);
        let mut groups = HashMap::new();
        for (name, slot) in &meta.groups {
            match KeyGroup::from_name(name) {
                Some(g) => {
                    groups.insert(g, ld.slot(slot));
                }
                None => ld.warnings.push(format!("unknown key group {name:?}")),
            }
        }
        let mut keys = HashMap::new();
        for (code, slot) in &meta.keys {
            if keyhook::intern(code).is_none() {
                ld.warnings.push(format!("unknown key code {code:?}"));
            }
            keys.insert(code.clone(), ld.slot(slot));
        }

        let Loader { clips, warnings, .. } = ld;
        Ok(LoadedPack { dir: dir.to_path_buf(), meta, defaults, groups, keys, clips, warnings })
    }

    /// Candidate clips for a key event. Empty slice = silent.
    pub fn resolve(&self, code: &str, dir: Dir) -> &[Arc<Sound>] {
        let i = idx(dir);
        if let Some(Some(v)) = self.keys.get(code).map(|s| &s[i]) {
            return v;
        }
        if let Some(Some(v)) = self.groups.get(&group_of(code)).map(|s| &s[i]) {
            return v;
        }
        match &self.defaults[i] {
            Some(v) => v,
            None => &[],
        }
    }

    pub fn clip(&self, reference: &str) -> Option<Arc<Sound>> {
        self.clips.get(reference).cloned()
    }

    pub fn clip_count(&self) -> usize {
        self.clips.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_clip(dir: &Path, name: &str, amp: f32, secs: f32) {
        let n = (8000.0 * secs) as usize;
        let s: Vec<f32> = (0..n).map(|i| ((i as f32) * 0.3).sin() * amp).collect();
        Sound::from_mono(8000, &s).write_wav(&dir.join(name)).unwrap();
    }

    fn pack_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (n, a) in [("a.wav", 0.2), ("b.wav", 0.4), ("space.wav", 0.6), ("enter.wav", 0.8), ("up.wav", 0.1)] {
            write_clip(d.path(), n, a, 0.05);
        }
        let mut pf = PackFile::new("t", "Test");
        pf.defaults.down = Some(vec!["a.wav".into(), "b.wav".into()]);
        pf.groups.insert("space".into(), Slot { down: Some(vec!["space.wav".into()]), up: Some(vec!["up.wav".into()]) });
        pf.groups.insert("modifiers".into(), Slot { down: Some(vec![]), up: None });
        pf.keys.insert("Enter".into(), Slot { down: Some(vec!["enter.wav".into()]), up: None });
        pf.keys.insert("KeyX".into(), Slot { down: Some(vec!["missing.wav".into()]), up: None });
        pf.write(d.path()).unwrap();
        d
    }

    #[test]
    fn resolution_order() {
        let d = pack_dir();
        let p = LoadedPack::load(d.path()).unwrap();
        let peak = |v: &[Arc<Sound>]| v.iter().map(|s| (s.peak() * 10.0).round() as i32).collect::<Vec<_>>();
        assert_eq!(peak(p.resolve("KeyA", Dir::Down)), vec![2, 4]); // defaults
        assert_eq!(peak(p.resolve("Space", Dir::Down)), vec![6]); // group
        assert_eq!(peak(p.resolve("Space", Dir::Up)), vec![1]); // group up
        assert_eq!(peak(p.resolve("Enter", Dir::Down)), vec![8]); // key
        assert!(p.resolve("Enter", Dir::Up).is_empty()); // nothing defined -> silent
        assert!(p.resolve("ShiftLeft", Dir::Down).is_empty()); // explicitly silent group
        // Missing file: warning, and the key is silent rather than crashing.
        assert!(p.resolve("KeyX", Dir::Down).is_empty());
        assert!(p.warnings.iter().any(|w| w.contains("missing.wav")));
        assert_eq!(p.clip_count(), 5);
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(safe_file_name("../../etc/passwd").is_err());
        assert!(safe_file_name("/abs.wav").is_err());
        assert!(safe_file_name("sub/dir.wav").is_err());
        assert!(safe_file_name(".hidden").is_err());
        assert!(safe_file_name("ok-file.wav").is_ok());
    }

    #[test]
    fn json_roundtrip_and_minimal() {
        let minimal = r#"{"id":"x","name":"X","defaults":{"down":["k.wav"]}}"#;
        let pf: PackFile = serde_json::from_str(minimal).unwrap();
        assert_eq!(pf.volume, 1.0);
        assert!(pf.trim_silence);
        assert_eq!(pf.random.pitch_cents, 30.0);
        let back: PackFile = serde_json::from_str(&serde_json::to_string(&pf).unwrap()).unwrap();
        assert_eq!(pf, back);
    }

    #[test]
    fn target_serde() {
        let t: Target = serde_json::from_str(r#"{"kind":"key","name":"Space"}"#).unwrap();
        assert_eq!(t, Target::Key("Space".into()));
        let t: Target = serde_json::from_str(r#"{"kind":"default"}"#).unwrap();
        assert_eq!(t, Target::Default);
    }
}
