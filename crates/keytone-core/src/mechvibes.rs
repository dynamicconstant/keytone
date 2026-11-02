//! Import Mechvibes sound packs (`config.json` + audio).
//!
//! Mechvibes keys are libuiohook virtual key codes. `key_define_type` is
//! either `"single"` (one sprite file, `defines` maps code -> [start_ms,
//! duration_ms]) or `"multi"` (`defines` maps code -> file name).

use crate::pack::{safe_file_name, PackFile, Slot, Sprite};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

#[derive(Deserialize)]
struct MvConfig {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    key_define_type: Option<String>,
    #[serde(default)]
    sound: Option<String>,
    #[serde(default)]
    defines: HashMap<String, Value>,
}

pub fn is_mechvibes_pack(dir: &Path) -> bool {
    dir.join("config.json").is_file() && !dir.join(crate::pack::PACK_FILE).is_file()
}

/// Convert the Mechvibes pack in `src` into a Keytone pack written to `dst`
/// (an empty folder). Returns the new pack description.
pub fn convert(src: &Path, dst: &Path, id: &str) -> Result<PackFile> {
    let text = std::fs::read_to_string(src.join("config.json")).context("reading config.json")?;
    let cfg: MvConfig = serde_json::from_str(&text).context("parsing Mechvibes config.json")?;
    let single = match cfg.key_define_type.as_deref() {
        Some("single") | None => cfg.defines.values().any(|v| v.is_array()),
        Some("multi") => false,
        Some(other) => bail!("unknown Mechvibes key_define_type {other:?}"),
    };

    let mut pf = PackFile::new(id, &cfg.name);
    pf.description = format!(
        "Imported from Mechvibes{}",
        cfg.id.map(|i| format!(" ({i})")).unwrap_or_default()
    );
    // Mechvibes packs are usually already varied; keep the variation light.
    pf.random.pitch_cents = 15.0;
    pf.random.gain_db = 1.0;

    let mut copied: BTreeMap<String, ()> = BTreeMap::new();
    let mut copy = |name: &str| -> Result<String> {
        let name = safe_file_name(name)?.to_string();
        if !copied.contains_key(&name) {
            std::fs::copy(src.join(&name), dst.join(&name))
                .with_context(|| format!("copying {name}"))?;
            copied.insert(name.clone(), ());
        }
        Ok(name)
    };

    let mut fallback: Option<(i64, String)> = None;
    if single {
        let file = cfg.sound.as_deref().ok_or_else(|| anyhow!("single-file pack without \"sound\""))?;
        let file = copy(file)?;
        let mut segments = BTreeMap::new();
        for (k, v) in &cfg.defines {
            let Some(code) = k.parse::<i64>().ok().and_then(uiohook_to_code) else { continue };
            let Some(arr) = v.as_array() else { continue };
            let (Some(start), Some(dur)) = (arr.first().and_then(Value::as_f64), arr.get(1).and_then(Value::as_f64)) else {
                continue;
            };
            if dur <= 0.0 {
                continue;
            }
            segments.insert(code.to_string(), [start.max(0.0) as u32, dur as u32]);
            pf.keys.insert(code.to_string(), Slot { down: Some(vec![format!("#{code}")]), up: None });
            consider_fallback(&mut fallback, k, format!("#{code}"));
        }
        pf.sprite = Some(Sprite { file, segments });
    } else {
        for (k, v) in &cfg.defines {
            let Some(code) = k.parse::<i64>().ok().and_then(uiohook_to_code) else { continue };
            let Some(name) = v.as_str() else { continue };
            let name = match copy(name) {
                Ok(n) => n,
                Err(e) => {
                    log::warn!("mechvibes import: skipping {name}: {e:#}");
                    continue;
                }
            };
            pf.keys.insert(code.to_string(), Slot { down: Some(vec![name.clone()]), up: None });
            consider_fallback(&mut fallback, k, name);
        }
    }

    // Keys the pack doesn't define play the pack's "A" (or the first) sound,
    // so nothing is unexpectedly silent.
    if let Some((_, r)) = fallback {
        pf.defaults.down = Some(vec![r]);
    }
    if pf.keys.is_empty() {
        bail!("no usable key definitions found in config.json");
    }
    pf.write(dst)?;
    Ok(pf)
}

fn consider_fallback(fallback: &mut Option<(i64, String)>, uio: &str, reference: String) {
    // Prefer KeyA (30), otherwise the lowest-numbered letter-ish key.
    let n: i64 = uio.parse().unwrap_or(i64::MAX);
    let score = if n == 30 { -1 } else { n };
    if fallback.as_ref().map(|(s, _)| score < *s).unwrap_or(true) {
        *fallback = Some((score, reference));
    }
}

/// libuiohook virtual key code -> W3C code.
pub fn uiohook_to_code(vc: i64) -> Option<&'static str> {
    Some(match vc {
        1 => "Escape",
        2 => "Digit1", 3 => "Digit2", 4 => "Digit3", 5 => "Digit4", 6 => "Digit5",
        7 => "Digit6", 8 => "Digit7", 9 => "Digit8", 10 => "Digit9", 11 => "Digit0",
        12 => "Minus", 13 => "Equal", 14 => "Backspace", 15 => "Tab",
        16 => "KeyQ", 17 => "KeyW", 18 => "KeyE", 19 => "KeyR", 20 => "KeyT", 21 => "KeyY",
        22 => "KeyU", 23 => "KeyI", 24 => "KeyO", 25 => "KeyP", 26 => "BracketLeft",
        27 => "BracketRight", 28 => "Enter", 29 => "ControlLeft",
        30 => "KeyA", 31 => "KeyS", 32 => "KeyD", 33 => "KeyF", 34 => "KeyG", 35 => "KeyH",
        36 => "KeyJ", 37 => "KeyK", 38 => "KeyL", 39 => "Semicolon", 40 => "Quote",
        41 => "Backquote", 42 => "ShiftLeft", 43 => "Backslash",
        44 => "KeyZ", 45 => "KeyX", 46 => "KeyC", 47 => "KeyV", 48 => "KeyB", 49 => "KeyN",
        50 => "KeyM", 51 => "Comma", 52 => "Period", 53 => "Slash", 54 => "ShiftRight",
        55 => "NumpadMultiply", 56 => "AltLeft", 57 => "Space", 58 => "CapsLock",
        59 => "F1", 60 => "F2", 61 => "F3", 62 => "F4", 63 => "F5", 64 => "F6", 65 => "F7",
        66 => "F8", 67 => "F9", 68 => "F10", 69 => "NumLock", 70 => "ScrollLock",
        71 => "Numpad7", 72 => "Numpad8", 73 => "Numpad9", 74 => "NumpadSubtract",
        75 => "Numpad4", 76 => "Numpad5", 77 => "Numpad6", 78 => "NumpadAdd",
        79 => "Numpad1", 80 => "Numpad2", 81 => "Numpad3", 82 => "Numpad0", 83 => "NumpadDecimal",
        86 => "IntlBackslash", 87 => "F11", 88 => "F12",
        91 => "F13", 92 => "F14", 93 => "F15", 99 => "F16", 100 => "F17", 101 => "F18",
        102 => "F19", 103 => "F20",
        3612 => "NumpadEnter", 3613 => "ControlRight", 3637 => "NumpadDivide",
        3639 => "PrintScreen", 3640 => "AltRight", 3653 => "Pause",
        3655 => "Home", 3657 => "PageUp", 3663 => "End", 3665 => "PageDown",
        3666 => "Insert", 3667 => "Delete", 3675 => "MetaLeft", 3676 => "MetaRight",
        3677 => "ContextMenu",
        57416 => "ArrowUp", 57419 => "ArrowLeft", 57421 => "ArrowRight", 57424 => "ArrowDown",
        // Alternate (non-numpad) codes some packs use.
        60999 => "Home", 61000 => "ArrowUp", 61001 => "PageUp", 61003 => "ArrowLeft",
        61005 => "ArrowRight", 61007 => "End", 61008 => "ArrowDown", 61009 => "PageDown",
        61010 => "Insert", 61011 => "Delete",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::LoadedPack;
    use crate::sound::Sound;
    use keyhook::Dir;

    #[test]
    fn all_mapped_codes_are_known() {
        for vc in 0..70000 {
            if let Some(c) = uiohook_to_code(vc) {
                assert!(keyhook::intern(c).is_some(), "{vc} -> {c}");
            }
        }
    }

    #[test]
    fn single_sprite_pack() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        // 1 s sprite at 8 kHz; each 100 ms block has a distinct level.
        let samples: Vec<f32> = (0..8000).map(|i| 0.05 * (1 + i / 800) as f32).collect();
        Sound::from_mono(8000, &samples).write_wav(&src.path().join("sound.wav")).unwrap();
        std::fs::write(
            src.path().join("config.json"),
            r#"{"id":"sk-cherrymx-blue","name":"CherryMX Blue","key_define_type":"single",
                "includes_numpad":false,"sound":"sound.wav",
                "defines":{"30":[100,80],"57":[500,80],"28":[700,80],"9999":[0,10],"14":null}}"#,
        )
        .unwrap();
        assert!(is_mechvibes_pack(src.path()));
        let pf = convert(src.path(), dst.path(), "cherry").unwrap();
        assert_eq!(pf.name, "CherryMX Blue");
        assert_eq!(pf.keys.len(), 3);
        assert_eq!(pf.defaults.down, Some(vec!["#KeyA".to_string()]));

        let p = LoadedPack::load(dst.path()).unwrap();
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        let lvl = |code| (p.resolve(code, Dir::Down)[0].peak() * 100.0).round() as i32;
        assert_eq!(lvl("KeyA"), 10);
        assert_eq!(lvl("Space"), 30);
        assert_eq!(lvl("Enter"), 40);
        assert_eq!(lvl("KeyZ"), 10); // falls back to KeyA's sound
    }

    #[test]
    fn multi_file_pack() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        for n in ["a.wav", "space.wav"] {
            Sound::from_mono(8000, &vec![0.3; 400]).write_wav(&src.path().join(n)).unwrap();
        }
        std::fs::write(
            src.path().join("config.json"),
            r#"{"name":"Multi","key_define_type":"multi",
                "defines":{"30":"a.wav","57":"space.wav","28":"missing.wav","1":null}}"#,
        )
        .unwrap();
        let pf = convert(src.path(), dst.path(), "multi").unwrap();
        assert_eq!(pf.keys.len(), 2);
        assert!(dst.path().join("a.wav").is_file());
        let p = LoadedPack::load(dst.path()).unwrap();
        assert_eq!(p.resolve("Space", Dir::Down).len(), 1);
    }
}
